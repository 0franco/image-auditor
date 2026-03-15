use std::io::Write;
use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use anyhow::Result;

use crate::app::{App, Screen};
use crate::llm::{LlmClient, LlmSuggestion, build_issue_prompt};
use crate::scanner::{Issue, scan_directory};

// ─── Scan ─────────────────────────────────────────────────────────────────────

pub fn start_scan(app: &mut App) {
    app.screen = Screen::Scanning;
    app.scan_start = Some(std::time::Instant::now());
    app.scan_result = None;
    app.scan_error = None;
    app.cached_filtered_indices.clear();

    let path = PathBuf::from(&app.input_path);
    let (tx, rx) = mpsc::channel();
    app.scan_rx = Some(rx);

    std::thread::spawn(move || {
        let _ = tx.send(scan_directory(&path));
    });
}

/// Like `start_scan` but keeps the current screen intact (used post-patch).
pub fn rescan_background(app: &mut App) {
    app.scan_start = Some(std::time::Instant::now());
    app.scan_error = None;
    app.cached_filtered_indices.clear();

    let path = PathBuf::from(&app.input_path);
    let (tx, rx) = mpsc::channel();
    app.scan_rx = Some(rx);

    std::thread::spawn(move || {
        let _ = tx.send(scan_directory(&path));
    });
}

// ─── Export ───────────────────────────────────────────────────────────────────

pub fn export_json(app: &App) -> Result<()> {
    if let Some(ref result) = app.scan_result {
        let json = serde_json::to_string_pretty(result)?;
        std::fs::write("image-audit-report.json", json)?;
    }
    Ok(())
}

// ─── Clipboard ───────────────────────────────────────────────────────────────

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    // macOS
    if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        return Ok(());
    }
    // Linux / WSL
    if let Ok(mut child) = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        return Ok(());
    }
    // xdotool / wl-copy fallback omitted — callers should handle the Err gracefully
    Err(anyhow::anyhow!("No clipboard helper found (pbcopy / xclip)"))
}

// ─── LLM ─────────────────────────────────────────────────────────────────────

pub fn trigger_llm_suggest(
    app: &mut App,
    issue: Issue,
    client: Arc<dyn LlmClient>,
) {
    let prompt = build_issue_prompt(&issue);
    let (tx, rx) = mpsc::channel::<Result<LlmSuggestion>>();
    app.detail_suggestion_rx = Some(rx);

    std::thread::spawn(move || {
        let _ = tx.send(client.suggest_fix(&prompt));
    });
}
