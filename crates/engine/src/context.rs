//! Application context – holds capability trait objects and config.

use crate::platform::{HeadlessClipboard, ReqwestNetwork, StdFilesystem, SystemClipboard};
use crate::traits::*;
use crate::types::detect_headless;

/// Central context passed to all engine operations.
///
/// Holds trait-object capabilities so callers (CLI / Tauri) can swap
/// implementations (e.g. headless clipboard vs real clipboard).
pub struct AppContext {
    fs: Box<dyn FilesystemOps>,
    network: Box<dyn NetworkOps>,
    clipboard: Box<dyn ClipboardOps>,
    /// Target host for network probe (configurable).
    pub network_probe_host: String,
}

impl AppContext {
    pub fn new(
        fs: Box<dyn FilesystemOps>,
        network: Box<dyn NetworkOps>,
        clipboard: Box<dyn ClipboardOps>,
    ) -> Self {
        Self {
            fs,
            network,
            clipboard,
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    /// Create a context with real platform implementations, choosing the
    /// appropriate clipboard based on headless detection.
    pub fn default_platform() -> Self {
        let clipboard: Box<dyn ClipboardOps> = if detect_headless() {
            Box::new(HeadlessClipboard)
        } else {
            Box::new(SystemClipboard)
        };
        Self {
            fs: Box::new(StdFilesystem),
            network: Box::new(ReqwestNetwork),
            clipboard,
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    /// Create a context suitable for headless / CI environments.
    pub fn default_headless() -> Self {
        Self {
            fs: Box::new(StdFilesystem),
            network: Box::new(ReqwestNetwork),
            clipboard: Box::new(HeadlessClipboard),
            network_probe_host: "https://httpbin.org/get".to_string(),
        }
    }

    pub fn fs(&self) -> &dyn FilesystemOps {
        self.fs.as_ref()
    }

    pub fn network(&self) -> &dyn NetworkOps {
        self.network.as_ref()
    }

    pub fn clipboard(&self) -> &dyn ClipboardOps {
        self.clipboard.as_ref()
    }
}
