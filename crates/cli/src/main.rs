//! `appctl` – headless CLI test harness for the Tauri template engine.
//!
//! Runs the same engine logic that powers the GUI, but without a window
//! server. Designed for VM-based compatibility testing on macOS + Linux.

mod serve;

use clap::{Parser, Subcommand};
use engine::types::*;
use engine::{AppContext, CommandRegistry, CommandResult};
use std::path::PathBuf;

// ===========================================================================
// CLI definition
// ===========================================================================

#[derive(Parser)]
#[command(
    name = "appctl",
    version,
    about = "CLI test harness for the Tauri template app"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Collect environment facts and emit an env summary.
    Doctor {
        /// Output as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
        /// Write result JSON to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Invoke a backend command by name with JSON args.
    Call {
        /// Command name (e.g. "ping", "read_file", "write_file").
        cmd: String,
        /// JSON args to pass to the command.
        #[arg(long, default_value = "{}")]
        args: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Timeout duration (e.g. "30s", "5000ms"). Currently informational.
        #[arg(long)]
        timeout: Option<String>,
        /// Directory for artifacts output.
        #[arg(long)]
        artifacts: Option<PathBuf>,
    },

    /// Targeted capability check: filesystem, network, or clipboard.
    Probe {
        /// Probe target: filesystem | network | clipboard
        target: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Directory for artifacts output.
        #[arg(long)]
        artifacts: Option<PathBuf>,
    },

    /// Run a scripted scenario from a YAML file.
    RunScenario {
        /// Path to the scenario YAML file.
        file: PathBuf,
        /// Directory for artifacts output.
        #[arg(long)]
        artifacts: Option<PathBuf>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
        /// Run interactively with go-back navigation.
        #[arg(long)]
        interactive: bool,
    },

    /// Start daemon mode over a Unix socket.
    Serve {
        /// Path for the Unix domain socket.
        #[arg(long)]
        socket: PathBuf,
    },

    /// Emit a desktop event (skeleton – returns UNIMPLEMENTED).
    Emit {
        /// Event type: tray-click | deep-link | file-drop | app-focus
        event: String,
        /// Optional event payload as JSON.
        #[arg(long, default_value = "{}")]
        payload: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

// ===========================================================================
// Main
// ===========================================================================

#[tokio::main]
async fn main() {
    // Install ring as the rustls crypto provider (reqwest needs this with rustls-no-provider)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Initialise tracing for CLI (structured, no tauri config dependency)
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let ctx = AppContext::default_platform();
    let registry = CommandRegistry::new();

    match cli.command {
        Commands::Doctor { json, out } => cmd_doctor(json, out).await,
        Commands::Call {
            cmd,
            args,
            json,
            timeout: _,
            artifacts,
        } => cmd_call(&cmd, &args, json, artifacts, &ctx, &registry).await,
        Commands::Probe {
            target,
            json,
            artifacts,
        } => cmd_probe(&target, json, artifacts, &ctx).await,
        Commands::RunScenario {
            file,
            artifacts,
            json,
            interactive,
        } => cmd_run_scenario(&file, json, interactive, artifacts, &ctx, &registry).await,
        Commands::Serve { socket } => serve::run_daemon(socket, ctx, registry).await,
        Commands::Emit {
            event,
            payload: _,
            json,
        } => cmd_emit(&event, json).await,
    }
}

// ===========================================================================
// Subcommand implementations
// ===========================================================================

async fn cmd_doctor(json: bool, out: Option<PathBuf>) {
    let result = engine::doctor::run_doctor();
    if let Some(ref path) = out {
        write_result_file(path, &result);
    }
    output_result(&result, json);
}

async fn cmd_call(
    cmd: &str,
    args_str: &str,
    json: bool,
    artifacts: Option<PathBuf>,
    ctx: &AppContext,
    registry: &CommandRegistry,
) {
    let args: serde_json::Value = match serde_json::from_str(args_str) {
        Ok(v) => v,
        Err(e) => {
            let r = result_err(
                "call",
                cmd,
                &new_run_id(),
                0,
                ErrorCode::InvalidInput,
                format!("invalid JSON args: {}", e),
            );
            output_result(&r, json);
            return;
        }
    };

    let result = registry.execute(cmd, args, ctx);
    if let Some(ref dir) = artifacts {
        write_artifacts(dir, &result);
    }
    output_result(&result, json);
}

async fn cmd_probe(target: &str, json: bool, artifacts: Option<PathBuf>, ctx: &AppContext) {
    let result = engine::probes::run_probe(target, ctx).await;
    if let Some(ref dir) = artifacts {
        write_artifacts(dir, &result);
    }
    output_result(&result, json);
}

async fn cmd_run_scenario(
    file: &PathBuf,
    json: bool,
    interactive: bool,
    artifacts: Option<PathBuf>,
    ctx: &AppContext,
    registry: &CommandRegistry,
) {
    let yaml = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            let r = result_err(
                "run-scenario",
                &file.display().to_string(),
                &new_run_id(),
                0,
                ErrorCode::IoError,
                format!("cannot read scenario file: {}", e),
            );
            output_result(&r, json);
            return;
        }
    };

    let scenario = match engine::scenario::load_scenario(&yaml) {
        Ok(s) => s,
        Err(e) => {
            let r = result_err(
                "run-scenario",
                &file.display().to_string(),
                &new_run_id(),
                0,
                ErrorCode::InvalidInput,
                e,
            );
            output_result(&r, json);
            return;
        }
    };

    let scenario_result = if interactive {
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!("error: --interactive requires a TTY (stdin is not a terminal)");
            std::process::exit(1);
        }
        engine::scenario::run_scenario_interactive(
            &scenario,
            ctx,
            registry,
            |idx, total, label, can_go_back| {
                use engine::scenario::StepChoice;

                // block_in_place tells Tokio this closure will block on TTY I/O,
                // so it can move async tasks off this worker thread.
                tokio::task::block_in_place(|| {
                    eprintln!("\n--- Step {}/{}: {} ---", idx + 1, total, label);

                    let mut choices = vec!["Run", "Skip"];
                    if can_go_back {
                        choices.push("\u{2190} Go back");
                    }

                    let selection = match dialoguer::Select::new()
                        .with_prompt("Run this step?")
                        .items(&choices)
                        .default(0)
                        .interact_opt()
                    {
                        Ok(Some(s)) => s,
                        Ok(None) => return None,
                        Err(e) => {
                            eprintln!("error: interactive prompt failed: {e}");
                            return None;
                        }
                    };

                    Some(match choices[selection] {
                        "Run" => StepChoice::Run,
                        "Skip" => StepChoice::Skip,
                        _ => StepChoice::GoBack,
                    })
                })
            },
            |idx, total, label| {
                use engine::scenario::FailureChoice;

                tokio::task::block_in_place(|| {
                    eprintln!("\n--- Step {}/{}: {} FAILED ---", idx + 1, total, label);

                    let choices = ["Continue to next step", "Abort scenario"];
                    let selection = match dialoguer::Select::new()
                        .with_prompt("Step failed. What would you like to do?")
                        .items(choices)
                        .default(0)
                        .interact_opt()
                    {
                        Ok(Some(s)) => s,
                        Ok(None) => return None,
                        Err(e) => {
                            eprintln!("error: interactive prompt failed: {e}");
                            return None;
                        }
                    };

                    Some(match choices[selection] {
                        "Continue to next step" => FailureChoice::Continue,
                        _ => FailureChoice::Abort,
                    })
                })
            },
        )
        .await
    } else {
        engine::scenario::run_scenario(&scenario, ctx, registry).await
    };

    if json {
        let j = serde_json::to_string_pretty(&scenario_result).unwrap_or_default();
        println!("{}", j);
    } else {
        println!(
            "Scenario: {}",
            scenario_result.name.as_deref().unwrap_or("<unnamed>")
        );
        println!("Overall: {:?}", scenario_result.overall_status);
        for (i, sr) in scenario_result.step_results.iter().enumerate() {
            println!(
                "  Step {}: {} -> {:?} ({}ms)",
                i, sr.target, sr.status, sr.timing_ms.total
            );
        }
    }

    if let Some(ref dir) = artifacts {
        let run_id = new_run_id();
        let art_dir = dir.join(&run_id);
        let _ = std::fs::create_dir_all(&art_dir);
        let result_path = art_dir.join("result.json");
        let j = serde_json::to_string_pretty(&scenario_result).unwrap_or_default();
        let _ = std::fs::write(&result_path, j);

        // Write per-step results as events.jsonl
        let events_path = art_dir.join("events.jsonl");
        let mut lines = String::new();
        for sr in &scenario_result.step_results {
            if let Ok(line) = serde_json::to_string(sr) {
                lines.push_str(&line);
                lines.push('\n');
            }
        }
        let _ = std::fs::write(&events_path, lines);
    }
}

async fn cmd_emit(event: &str, json: bool) {
    let run_id = new_run_id();
    let headless = detect_headless();

    let (status, code, msg) = if headless {
        (
            Status::Skip,
            ErrorCode::Unsupported,
            format!("event '{}' unsupported in headless environment", event),
        )
    } else {
        (
            Status::Skip,
            ErrorCode::Unimplemented,
            format!("event '{}' is not yet implemented (skeleton)", event),
        )
    };

    let result = CommandResult {
        run_id,
        command: "emit".to_string(),
        target: event.to_string(),
        status,
        error: Some(ErrorInfo {
            code,
            message: msg,
            details: serde_json::Value::Null,
        }),
        timing_ms: TimingInfo::default(),
        artifacts: vec![],
        env_summary: EnvSummary::default(),
        data: None,
    };
    output_result(&result, json);
}

// ===========================================================================
// Output helpers
// ===========================================================================

fn output_result(result: &CommandResult, json: bool) {
    if json {
        let j = serde_json::to_string_pretty(result).unwrap_or_default();
        println!("{}", j);
    } else {
        print_human(result);
    }

    // Exit with non-zero status on error/fail
    match result.status {
        Status::Pass | Status::Skip => {}
        Status::Fail => std::process::exit(1),
        Status::Error => std::process::exit(2),
    }
}

fn print_human(r: &CommandResult) {
    let status_icon = match r.status {
        Status::Pass => "PASS",
        Status::Fail => "FAIL",
        Status::Skip => "SKIP",
        Status::Error => "ERROR",
    };

    println!("[{}] {} {}", status_icon, r.command, r.target);
    println!("  run_id: {}", r.run_id);
    println!("  timing: {}ms", r.timing_ms.total);

    if !r.timing_ms.steps.is_empty() {
        for (step, ms) in &r.timing_ms.steps {
            println!("    {}: {}ms", step, ms);
        }
    }

    if let Some(ref err) = r.error {
        println!("  error:  {} – {}", err.code, err.message);
    }

    if let Some(ref data) = r.data {
        // Print compact data for human output
        if let Ok(s) = serde_json::to_string_pretty(data) {
            // Indent each line
            for line in s.lines() {
                println!("  {}", line);
            }
        }
    }

    println!(
        "  env: os={} arch={} headless={}",
        r.env_summary.os, r.env_summary.arch, r.env_summary.headless
    );
}

// ===========================================================================
// Artifact helpers
// ===========================================================================

fn write_result_file(path: &std::path::Path, result: &CommandResult) {
    let j = serde_json::to_string_pretty(result).unwrap_or_default();
    if let Err(e) = std::fs::write(path, &j) {
        eprintln!(
            "warning: failed to write result to {}: {}",
            path.display(),
            e
        );
    }
}

fn write_artifacts(dir: &std::path::Path, result: &CommandResult) {
    let art_dir = dir.join(&result.run_id);
    if let Err(e) = std::fs::create_dir_all(&art_dir) {
        eprintln!(
            "warning: failed to create artifacts dir {}: {}",
            art_dir.display(),
            e
        );
        return;
    }

    // result.json
    let result_path = art_dir.join("result.json");
    let j = serde_json::to_string_pretty(result).unwrap_or_default();
    let _ = std::fs::write(&result_path, &j);

    // events.jsonl (single event for non-scenario)
    let events_path = art_dir.join("events.jsonl");
    if let Ok(line) = serde_json::to_string(result) {
        let _ = std::fs::write(&events_path, format!("{}\n", line));
    }
}
