//! Targeted capability probes – filesystem, network, clipboard.

use crate::context::AppContext;
use crate::traits::CapError;
use crate::types::*;
use std::collections::HashMap;
use std::time::Instant;

/// Run a probe by name and return a full CommandResult.
pub async fn run_probe(name: &str, ctx: &AppContext) -> CommandResult {
    match name {
        "filesystem" => probe_filesystem(ctx),
        "network" => probe_network(ctx).await,
        "clipboard" => probe_clipboard(ctx),
        _ => {
            let run_id = new_run_id();
            result_err(
                "probe",
                name,
                &run_id,
                0,
                ErrorCode::InvalidInput,
                format!(
                    "unknown probe: {} (available: filesystem, network, clipboard)",
                    name
                ),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Filesystem probe
// ---------------------------------------------------------------------------

fn probe_filesystem(ctx: &AppContext) -> CommandResult {
    let run_id = new_run_id();
    let start = Instant::now();
    let mut steps = HashMap::new();

    let tmp_dir = ctx
        .fs()
        .temp_dir()
        .join(format!("engine_probe_{}", &run_id[..8]));

    // Step 1: create temp directory
    let t0 = Instant::now();
    if let Err(e) = ctx.fs().create_dir_all(&tmp_dir) {
        return probe_fs_err(&run_id, start, steps, "create_dir", e);
    }
    steps.insert("create_dir".into(), t0.elapsed().as_millis() as u64);

    // Step 2: write a test file
    let test_file = tmp_dir.join("probe_test.txt");
    let payload = b"engine filesystem probe";
    let t1 = Instant::now();
    if let Err(e) = ctx.fs().write_file(&test_file, payload) {
        let _ = ctx.fs().remove_dir_all(&tmp_dir);
        return probe_fs_err(&run_id, start, steps, "write_file", e);
    }
    steps.insert("write_file".into(), t1.elapsed().as_millis() as u64);

    // Step 3: read it back and verify
    let t2 = Instant::now();
    match ctx.fs().read_file(&test_file) {
        Ok(data) => {
            if data != payload {
                let _ = ctx.fs().remove_dir_all(&tmp_dir);
                return result_err(
                    "probe",
                    "filesystem",
                    &run_id,
                    start.elapsed().as_millis() as u64,
                    ErrorCode::ExternalInterference,
                    "read-back data does not match written data",
                );
            }
        }
        Err(e) => {
            let _ = ctx.fs().remove_dir_all(&tmp_dir);
            return probe_fs_err(&run_id, start, steps, "read_file", e);
        }
    }
    steps.insert("read_verify".into(), t2.elapsed().as_millis() as u64);

    // Step 4: cleanup
    let t3 = Instant::now();
    let _ = ctx.fs().remove_dir_all(&tmp_dir);
    steps.insert("cleanup".into(), t3.elapsed().as_millis() as u64);

    let mut r = result_ok(
        "probe",
        "filesystem",
        &run_id,
        start.elapsed().as_millis() as u64,
    );
    r.timing_ms.steps = steps;
    r.data = Some(serde_json::json!({
        "temp_dir_used": tmp_dir.display().to_string(),
    }));
    r
}

fn probe_fs_err(
    run_id: &str,
    start: Instant,
    steps: HashMap<String, u64>,
    failed_step: &str,
    err: CapError,
) -> CommandResult {
    let code = match &err {
        CapError::PermissionDenied(_) => ErrorCode::PermissionDenied,
        CapError::Io(_) => ErrorCode::IoError,
        _ => ErrorCode::InternalError,
    };
    let mut r = result_err(
        "probe",
        "filesystem",
        run_id,
        start.elapsed().as_millis() as u64,
        code,
        format!("filesystem probe failed at {}: {}", failed_step, err),
    );
    r.timing_ms.steps = steps;
    r
}

// ---------------------------------------------------------------------------
// Network probe
// ---------------------------------------------------------------------------

async fn probe_network(ctx: &AppContext) -> CommandResult {
    let run_id = new_run_id();
    let start = Instant::now();
    let mut steps = HashMap::new();

    let host = &ctx.network_probe_host;
    // Extract hostname for DNS (strip scheme + path)
    let dns_host = host
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(host);

    // Step 1: DNS resolve
    let t0 = Instant::now();
    match ctx.network().dns_resolve(dns_host).await {
        Ok(addrs) => {
            steps.insert("dns_resolve".into(), t0.elapsed().as_millis() as u64);

            // Step 2: HTTPS GET
            let t1 = Instant::now();
            match ctx.network().https_get(host, 10_000).await {
                Ok((status, _snippet)) => {
                    steps.insert("https_get".into(), t1.elapsed().as_millis() as u64);

                    // Collect proxy env vars
                    let proxy_vars = collect_proxy_env();

                    let mut r = result_ok(
                        "probe",
                        "network",
                        &run_id,
                        start.elapsed().as_millis() as u64,
                    );
                    r.timing_ms.steps = steps;
                    r.data = Some(serde_json::json!({
                        "dns_addresses": addrs,
                        "http_status": status,
                        "target_url": host,
                        "proxy_env": proxy_vars,
                    }));
                    r
                }
                Err(e) => {
                    steps.insert("https_get".into(), t1.elapsed().as_millis() as u64);
                    let code = match &e {
                        CapError::Timeout => ErrorCode::Timeout,
                        _ => ErrorCode::NetworkError,
                    };
                    let mut r = result_err(
                        "probe",
                        "network",
                        &run_id,
                        start.elapsed().as_millis() as u64,
                        code,
                        format!("HTTPS GET failed: {}", e),
                    );
                    r.timing_ms.steps = steps;
                    r
                }
            }
        }
        Err(e) => {
            steps.insert("dns_resolve".into(), t0.elapsed().as_millis() as u64);
            let mut r = result_err(
                "probe",
                "network",
                &run_id,
                start.elapsed().as_millis() as u64,
                ErrorCode::NetworkError,
                format!("DNS resolution failed: {}", e),
            );
            r.timing_ms.steps = steps;
            r
        }
    }
}

fn collect_proxy_env() -> HashMap<String, String> {
    let keys = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "NO_PROXY",
        "no_proxy",
    ];
    let mut out = HashMap::new();
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            out.insert(k.to_string(), v);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Clipboard probe
// ---------------------------------------------------------------------------

fn probe_clipboard(ctx: &AppContext) -> CommandResult {
    let run_id = new_run_id();
    let start = Instant::now();
    let mut steps = HashMap::new();

    // If headless, skip immediately
    if detect_headless() {
        return result_skip(
            "probe",
            "clipboard",
            &run_id,
            start.elapsed().as_millis() as u64,
            "headless environment – no clipboard access",
        );
    }

    let test_text = format!("engine_clipboard_probe_{}", &run_id[..8]);

    // Step 1: write
    let t0 = Instant::now();
    match ctx.clipboard().write_text(&test_text) {
        Ok(()) => {
            steps.insert("write".into(), t0.elapsed().as_millis() as u64);
        }
        Err(e) => {
            steps.insert("write".into(), t0.elapsed().as_millis() as u64);
            return clipboard_err_result(&run_id, start, steps, "write", &e);
        }
    }

    // Step 2: read back
    let t1 = Instant::now();
    match ctx.clipboard().read_text() {
        Ok(text) => {
            steps.insert("read".into(), t1.elapsed().as_millis() as u64);
            if text.trim() != test_text {
                let mut r = result_err(
                    "probe",
                    "clipboard",
                    &run_id,
                    start.elapsed().as_millis() as u64,
                    ErrorCode::ExternalInterference,
                    "clipboard read-back does not match written text",
                );
                r.timing_ms.steps = steps;
                return r;
            }
        }
        Err(e) => {
            steps.insert("read".into(), t1.elapsed().as_millis() as u64);
            return clipboard_err_result(&run_id, start, steps, "read", &e);
        }
    }

    let mut r = result_ok(
        "probe",
        "clipboard",
        &run_id,
        start.elapsed().as_millis() as u64,
    );
    r.timing_ms.steps = steps;
    r
}

fn clipboard_err_result(
    run_id: &str,
    start: Instant,
    steps: HashMap<String, u64>,
    failed_step: &str,
    err: &CapError,
) -> CommandResult {
    let code = match err {
        CapError::Unsupported(_) => ErrorCode::Unsupported,
        CapError::DependencyMissing(_) => ErrorCode::DependencyMissing,
        CapError::PermissionDenied(_) => ErrorCode::PermissionDenied,
        _ => ErrorCode::InternalError,
    };
    // For unsupported/dependency-missing, return skip rather than error
    let status = match code {
        ErrorCode::Unsupported | ErrorCode::DependencyMissing => Status::Skip,
        _ => Status::Error,
    };
    let mut r = CommandResult {
        run_id: run_id.to_string(),
        command: "probe".to_string(),
        target: "clipboard".to_string(),
        status,
        error: Some(ErrorInfo {
            code,
            message: format!("clipboard probe failed at {}: {}", failed_step, err),
            details: serde_json::Value::Null,
        }),
        timing_ms: TimingInfo {
            total: start.elapsed().as_millis() as u64,
            steps,
        },
        artifacts: vec![],
        env_summary: EnvSummary::default(),
        data: None,
    };
    // Ensure timing is set
    r.timing_ms.total = start.elapsed().as_millis() as u64;
    r
}
