//! Platform-specific implementations of OS capability traits.
//!
//! - [`StdFilesystem`]: real std::fs operations
//! - [`ReqwestNetwork`]: real HTTP via reqwest
//! - [`SystemClipboard`]: platform clipboard (pbcopy/xclip)
//! - [`HeadlessClipboard`]: always returns UNSUPPORTED/SKIP

use crate::traits::*;
use std::path::{Path, PathBuf};

// ===========================================================================
// Filesystem – wraps std::fs
// ===========================================================================

pub struct StdFilesystem;

impl FilesystemOps for StdFilesystem {
    fn read_file(&self, path: &Path) -> CapResult<Vec<u8>> {
        std::fs::read(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CapError::Io(e),
            std::io::ErrorKind::PermissionDenied => {
                CapError::PermissionDenied(format!("cannot read {}: {}", path.display(), e))
            }
            _ => CapError::Io(e),
        })
    }

    fn write_file(&self, path: &Path, data: &[u8]) -> CapResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, data).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                CapError::PermissionDenied(format!("cannot write {}: {}", path.display(), e))
            }
            _ => CapError::Io(e),
        })
    }

    fn remove_file(&self, path: &Path) -> CapResult<()> {
        std::fs::remove_file(path).map_err(CapError::Io)
    }

    fn create_dir_all(&self, path: &Path) -> CapResult<()> {
        std::fs::create_dir_all(path).map_err(CapError::Io)
    }

    fn remove_dir_all(&self, path: &Path) -> CapResult<()> {
        std::fs::remove_dir_all(path).map_err(CapError::Io)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn list_dir(&self, path: &Path) -> CapResult<Vec<DirEntry>> {
        let read_dir = std::fs::read_dir(path)?;
        let mut entries = Vec::new();
        for entry in read_dir {
            let entry = entry?;
            // Skip entries whose metadata is transiently unavailable
            let Ok(meta) = entry.metadata() else { continue };
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                size_bytes: meta.len(),
            });
        }
        Ok(entries)
    }
}

// ===========================================================================
// Network – wraps reqwest
// ===========================================================================

pub struct ReqwestNetwork;

#[async_trait::async_trait]
impl NetworkOps for ReqwestNetwork {
    async fn dns_resolve(&self, host: &str) -> CapResult<Vec<String>> {
        use tokio::net::lookup_host;
        let addrs: Vec<String> = lookup_host(format!("{}:443", host))
            .await
            .map_err(|e| CapError::Network(format!("DNS resolution failed for {}: {}", host, e)))?
            .map(|a| a.ip().to_string())
            .collect();
        if addrs.is_empty() {
            return Err(CapError::Network(format!(
                "DNS resolution returned no addresses for {}",
                host
            )));
        }
        Ok(addrs)
    }

    async fn https_get(&self, url: &str, timeout_ms: u64) -> CapResult<(u16, String)> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| CapError::Network(format!("failed to build HTTP client: {}", e)))?;

        let resp = client.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                CapError::Timeout
            } else {
                CapError::Network(format!("HTTPS GET {}: {}", url, e))
            }
        })?;

        let status = resp.status().as_u16();
        // Read at most 4 KiB for the snippet
        let body = resp
            .text()
            .await
            .map_err(|e| CapError::Network(format!("reading body: {}", e)))?;
        let snippet: String = body.chars().take(4096).collect();
        Ok((status, snippet))
    }
}

// ===========================================================================
// Clipboard – platform implementations
// ===========================================================================

/// System clipboard using platform CLI tools.
///
/// - macOS: pbcopy / pbpaste
/// - Linux: xclip / xsel / wl-copy+wl-paste
pub struct SystemClipboard;

impl ClipboardOps for SystemClipboard {
    fn read_text(&self) -> CapResult<String> {
        #[cfg(target_os = "macos")]
        {
            run_clipboard_cmd("pbpaste", &[])
        }
        #[cfg(target_os = "linux")]
        {
            linux_clipboard_read()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(CapError::Unsupported(
                "clipboard not implemented for this OS".into(),
            ))
        }
    }

    fn write_text(&self, text: &str) -> CapResult<()> {
        #[cfg(target_os = "macos")]
        {
            run_clipboard_write("pbcopy", &[], text)
        }
        #[cfg(target_os = "linux")]
        {
            linux_clipboard_write(text)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = text;
            Err(CapError::Unsupported(
                "clipboard not implemented for this OS".into(),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_clipboard_read() -> CapResult<String> {
    // Try xclip first, then xsel, then wl-paste
    if let Ok(out) = run_clipboard_cmd("xclip", &["-selection", "clipboard", "-o"]) {
        return Ok(out);
    }
    if let Ok(out) = run_clipboard_cmd("xsel", &["--clipboard", "--output"]) {
        return Ok(out);
    }
    if let Ok(out) = run_clipboard_cmd("wl-paste", &[]) {
        return Ok(out);
    }
    Err(CapError::DependencyMissing(
        "none of xclip, xsel, or wl-paste found".into(),
    ))
}

#[cfg(target_os = "linux")]
fn linux_clipboard_write(text: &str) -> CapResult<()> {
    if run_clipboard_write("xclip", &["-selection", "clipboard"], text).is_ok() {
        return Ok(());
    }
    if run_clipboard_write("xsel", &["--clipboard", "--input"], text).is_ok() {
        return Ok(());
    }
    if run_clipboard_write("wl-copy", &[], text).is_ok() {
        return Ok(());
    }
    Err(CapError::DependencyMissing(
        "none of xclip, xsel, or wl-copy found".into(),
    ))
}

#[allow(dead_code)]
fn run_clipboard_cmd(cmd: &str, args: &[&str]) -> CapResult<String> {
    let output = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CapError::DependencyMissing(format!("{} not found", cmd))
            } else {
                CapError::Io(e)
            }
        })?;

    if !output.status.success() {
        return Err(CapError::Other(format!(
            "{} exited with {}",
            cmd, output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[allow(dead_code)]
fn run_clipboard_write(cmd: &str, args: &[&str], text: &str) -> CapResult<()> {
    use std::io::Write;
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CapError::DependencyMissing(format!("{} not found", cmd))
            } else {
                CapError::Io(e)
            }
        })?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(CapError::Other(format!("{} exited with {}", cmd, status)));
    }
    Ok(())
}

// ===========================================================================
// Headless clipboard – returns SKIP / UNSUPPORTED cleanly
// ===========================================================================

/// Clipboard stub for headless environments. Never panics.
pub struct HeadlessClipboard;

impl ClipboardOps for HeadlessClipboard {
    fn read_text(&self) -> CapResult<String> {
        Err(CapError::Unsupported(
            "clipboard unavailable in headless environment".into(),
        ))
    }
    fn write_text(&self, _text: &str) -> CapResult<()> {
        Err(CapError::Unsupported(
            "clipboard unavailable in headless environment".into(),
        ))
    }
}
