use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use console::Style;
use dialoguer::{Input, MultiSelect, Password, theme::ColorfulTheme};

use super::StepResult;
use crate::ui;

struct EnvEntry {
    key: String,
    default: String,
    comment: String,
    is_secret: bool,
}

/// Mask a value for display: show first 4 chars, replace the rest with '*'.
fn mask_value(val: &str) -> String {
    if val.is_empty() {
        return "(empty)".to_string();
    }
    let chars: Vec<char> = val.chars().collect();
    let visible = chars.len().min(4);
    let masked = chars.len().saturating_sub(visible);
    let prefix: String = chars[..visible].iter().collect();
    format!("{}{}", prefix, "*".repeat(masked))
}

pub fn run(project_root: &Path) -> StepResult {
    ui::print_step(3, "Configure environment variables");

    let example_path = project_root.join(".env.example");
    if !example_path.exists() {
        ui::print_warning("No .env.example found. Skipping.");
        return StepResult::Skipped;
    }

    let example_content = match fs::read_to_string(&example_path) {
        Ok(c) => c,
        Err(e) => return StepResult::Failed(format!("Cannot read .env.example: {}", e)),
    };

    // Parse .env.example into entries
    let mut entries: Vec<EnvEntry> = Vec::new();
    let mut current_comment = String::new();

    for line in example_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let comment_text = trimmed.trim_start_matches('#').trim();
            if !comment_text.is_empty() {
                current_comment = comment_text.to_string();
            }
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let is_secret = ["KEY", "SECRET", "TOKEN", "PASSWORD"]
                .iter()
                .any(|s| key.to_uppercase().contains(s));
            entries.push(EnvEntry {
                key,
                default: val.trim().to_string(),
                comment: std::mem::take(&mut current_comment),
                is_secret,
            });
        }
    }

    if entries.is_empty() {
        ui::print_warning("No variables found in .env.example.");
        return StepResult::Skipped;
    }

    // Load existing .env values
    let env_path = project_root.join(".env");
    let existing: BTreeMap<String, String> = if env_path.exists() {
        fs::read_to_string(&env_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().starts_with('#') && l.contains('='))
            .filter_map(|l| l.split_once('=').map(|(k, v)| {
                let v = v.trim();
                let v = if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
                    &v[1..v.len() - 1]
                } else {
                    v
                };
                (k.trim().to_string(), v.replace("\\\"", "\"").replace("\\\\", "\\"))
            }))
            .collect()
    } else {
        BTreeMap::new()
    };

    // Show current state
    let set_count = entries
        .iter()
        .filter(|e| {
            existing
                .get(&e.key)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        })
        .count();
    println!();
    println!(
        "  Current environment variables ({}/{} configured):",
        set_count,
        entries.len()
    );
    let highlight = Style::new().on_green().black().bold();
    let dim = Style::new().dim();
    for entry in &entries {
        let current = existing
            .get(&entry.key)
            .cloned()
            .unwrap_or_else(|| entry.default.clone());
        if current.is_empty() {
            println!(
                "    {} {} = {}",
                dim.apply_to("\u{2717}"),
                dim.apply_to(&entry.key),
                dim.apply_to("(not set)")
            );
        } else {
            println!(
                "    {} {} = {}",
                highlight.apply_to("\u{2714}"),
                highlight.apply_to(format!(" {} ", &entry.key)),
                mask_value(&current)
            );
        }
    }
    println!();

    // Select which to configure
    let labels: Vec<String> = entries
        .iter()
        .map(|e| {
            let current = existing.get(&e.key).cloned().unwrap_or_default();
            let status = if !current.is_empty() {
                " \u{2714}" // green checkmark rendered by terminal
            } else {
                ""
            };
            if e.comment.is_empty() {
                format!("{}{}", e.key, status)
            } else {
                format!("{} ({}){}", e.key, e.comment, status)
            }
        })
        .collect();

    let selections = match MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select variables to configure (Space to toggle, Enter to confirm)")
        .items(&labels)
        .interact()
    {
        Ok(s) => s,
        Err(e) => return StepResult::Failed(format!("Selection error: {}", e)),
    };

    if selections.is_empty() {
        ui::print_skip("No variables selected.");
        return StepResult::Skipped;
    }

    // Collect new values
    let mut new_values: BTreeMap<String, String> = existing.clone();
    for &idx in &selections {
        let entry = &entries[idx];
        let current = new_values
            .get(&entry.key)
            .cloned()
            .unwrap_or_else(|| entry.default.clone());

        let value = if entry.is_secret {
            match Password::with_theme(&ColorfulTheme::default())
                .with_prompt(&format!("{} (leave empty to keep current)", entry.key))
                .allow_empty_password(true)
                .interact()
            {
                Ok(v) if v.is_empty() => current,
                Ok(v) => v,
                Err(e) => return StepResult::Failed(format!("Input error: {}", e)),
            }
        } else {
            match Input::with_theme(&ColorfulTheme::default())
                .with_prompt(&entry.key)
                .default(current)
                .allow_empty(true)
                .interact_text()
            {
                Ok(v) => v,
                Err(e) => return StepResult::Failed(format!("Input error: {}", e)),
            }
        };

        // Show masked confirmation so the user can verify what was entered
        println!("  \u{2192} {} = {}", entry.key, mask_value(&value));

        new_values.insert(entry.key.clone(), value);
    }

    // Write .env preserving structure from .env.example
    let mut output = String::new();
    for line in example_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if let Some((key, _)) = trimmed.split_once('=') {
            let key = key.trim();
            if let Some(val) = new_values.get(key) {
                if val.is_empty() {
                    // Comment out unconfigured vars
                    output.push_str(&format!("# {}=\n", key));
                } else {
                    output.push_str(&format!("{}=\"{}\"\n", key, val.replace('\\', "\\\\").replace('"', "\\\"")));
                }
            } else {
                output.push_str(&format!("# {}=\n", key));
            }
        }
    }

    if let Err(e) = fs::write(&env_path, output) {
        return StepResult::Failed(format!("Cannot write .env: {}", e));
    }

    ui::print_success("Wrote .env");
    StepResult::Success
}
