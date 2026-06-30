use std::fs;
use std::path::Path;
use std::process::Command;

use dialoguer::{Input, Select, theme::ColorfulTheme};

use super::StepResult;
use crate::ui;

pub fn run(project_root: &Path) -> StepResult {
    ui::print_step(5, "Generate media assets");

    // Check for Gemini API key
    let env_path = project_root.join(".env");
    let has_key = if env_path.exists() {
        fs::read_to_string(&env_path)
            .unwrap_or_default()
            .lines()
            .any(|l| {
                if let Some((k, v)) = l.split_once('=') {
                    k.trim() == "APP__GEMINI_API_KEY" && !v.trim().is_empty()
                } else {
                    false
                }
            })
    } else {
        false
    };

    if !has_key {
        ui::print_warning("APP__GEMINI_API_KEY not set in .env.");
        ui::print_warning("Media generation requires the Gemini API key.");
        ui::print_skip("Skipping media generation.");
        return StepResult::Skipped;
    }

    let options = &["Both (banner + logo)", "Banner only", "Logo only", "Skip"];
    let selection = match Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What to generate?")
        .items(options)
        .default(0)
        .interact()
    {
        Ok(s) => s,
        Err(e) => return StepResult::Failed(format!("Selection error: {}", e)),
    };

    if selection == 3 {
        ui::print_skip("Skipped by user.");
        return StepResult::Skipped;
    }

    let suggestion: String = match Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Theme/style suggestion (or press Enter to skip)")
        .default(String::new())
        .allow_empty(true)
        .interact_text()
    {
        Ok(s) => s,
        Err(e) => return StepResult::Failed(format!("Input error: {}", e)),
    };

    let generate_banner = selection == 0 || selection == 1;
    let generate_logo = selection == 0 || selection == 2;

    // Generate logo first so banner can use it as a reference
    if generate_logo {
        println!();
        println!("  Generating logo...");
        if let Err(e) = run_asset_gen(project_root, "logo", &suggestion, None) {
            return StepResult::Failed(format!("Logo generation failed: {}", e));
        }
        ui::print_success("Logo assets saved to docs/public/");
    }

    if generate_banner {
        println!();
        println!("  Generating banner...");
        // Let the binary's own auto-detection find docs/public/icon-light.png with graceful fallback
        if let Err(e) = run_asset_gen(project_root, "banner", &suggestion, None) {
            return StepResult::Failed(format!("Banner generation failed: {}", e));
        }
        ui::print_success("Banner generated at media/banner.png");
    }

    StepResult::Success
}

/// Parse a `.env` file into key-value pairs, skipping comments and empty lines.
fn load_dotenv(path: &Path) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().starts_with('#') && l.contains('='))
        .filter_map(|l| {
            l.split_once('=').map(|(k, v)| {
                let v = v.trim();
                let v = if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    &v[1..v.len() - 1]
                } else {
                    v
                };
                (k.trim().to_string(), v.replace("\\\"", "\"").replace("\\\\", "\\"))
            })
        })
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

fn run_asset_gen(
    project_root: &Path,
    mode: &str,
    suggestion: &str,
    icon: Option<&Path>,
) -> Result<(), String> {
    let env_vars = load_dotenv(&project_root.join(".env"));

    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--bin")
        .arg("asset-gen")
        .arg("--")
        .arg(mode)
        .current_dir(project_root.join("src-tauri"))
        .envs(env_vars);

    if !suggestion.is_empty() {
        cmd.arg("--suggestion").arg(suggestion);
    }

    if let Some(icon_path) = icon {
        cmd.arg("--icon").arg(icon_path);
    }

    let status = cmd.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("asset-gen exited with {}", status))
    }
}
