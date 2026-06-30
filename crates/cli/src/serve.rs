//! Daemon mode – minimal JSON-RPC-ish protocol over Unix socket.

use engine::types::*;
use engine::{AppContext, CommandRegistry};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

pub async fn run_daemon(socket_path: PathBuf, ctx: AppContext, registry: CommandRegistry) {
    // Remove stale socket if it exists
    let _ = std::fs::remove_file(&socket_path);

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind socket {}: {}", socket_path.display(), e);
            std::process::exit(2);
        }
    };

    eprintln!("appctl daemon listening on {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    let response = handle_request(&line, &ctx, &registry).await;
                    let mut resp_json =
                        serde_json::to_string(&response).unwrap_or_else(|_| "{}".into());
                    resp_json.push('\n');
                    if writer.write_all(resp_json.as_bytes()).await.is_err() {
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("accept error: {}", e);
            }
        }
    }
}

async fn handle_request(
    line: &str,
    ctx: &AppContext,
    registry: &CommandRegistry,
) -> DaemonResponse {
    let req: DaemonRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return DaemonResponse {
                id: "unknown".into(),
                result: None,
                error: Some(ErrorInfo {
                    code: ErrorCode::InvalidInput,
                    message: format!("invalid JSON request: {}", e),
                    details: serde_json::Value::Null,
                }),
            };
        }
    };

    let result = match req.method.as_str() {
        "call" => {
            let cmd_name = req.params.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            let args = req
                .params
                .get("args")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            registry.execute(cmd_name, args, ctx)
        }
        "probe" => {
            let target = req
                .params
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            engine::probes::run_probe(target, ctx).await
        }
        "doctor" => engine::doctor::run_doctor(),
        other => {
            return DaemonResponse {
                id: req.id,
                result: None,
                error: Some(ErrorInfo {
                    code: ErrorCode::InvalidInput,
                    message: format!("unknown method: {}", other),
                    details: serde_json::Value::Null,
                }),
            };
        }
    };

    DaemonResponse {
        id: req.id,
        result: Some(result),
        error: None,
    }
}
