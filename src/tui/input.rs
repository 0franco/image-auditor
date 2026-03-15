use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

use crate::app::{App, PatchSuccess, Screen};
use crate::patch::apply_suggested_patch;
use crate::scanner::{IssueSeverity, IssueKind};
use crate::tui::actions::{copy_to_clipboard, export_json, rescan_background, start_scan, trigger_llm_suggest};

pub fn handle_menu(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    if app.input_mode {
        handle_menu_input(app, key);
        return;
    }

    match key {
        KeyCode::Char('q') | KeyCode::Char('c')
            if modifiers.contains(KeyModifiers::CONTROL) =>
        {
            app.running = false;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app.menu_state.selected().unwrap_or(0);
            app.menu_state.select(Some(i.saturating_sub(1)));
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app.menu_state.selected().unwrap_or(0);
            app.menu_state.select(Some((i + 1).min(1)));
        }
        KeyCode::Enter | KeyCode::Char(' ') => match app.menu_state.selected() {
            Some(0) => app.input_mode = true,
            Some(1) => app.running = false,
            _ => {}
        },
        _ => {}
    }
}

fn handle_menu_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter => {
            if app.input_path.trim().is_empty() {
                app.scan_error = Some("Please enter a path to scan.".into());
            } else {
                app.input_mode = false;
                start_scan(app);
            }
        }
        KeyCode::Esc => app.input_mode = false,
        KeyCode::Backspace => { app.input_path.pop(); }
        KeyCode::Char(c) => app.input_path.push(c),
        _ => {}
    }
}

pub fn handle_results(app: &mut App, key: KeyCode) {
    if app.search_mode {
        handle_results_search(app, key);
        return;
    }

    let issues = app.filtered_issues();
    let len = issues.len();

    match key {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.screen = Screen::Menu;
            app.scan_result = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let next = app.table_state.selected().unwrap_or(0).saturating_sub(1);
            app.table_state.select(Some(next));
            app.scroll_state = app.scroll_state.position(next);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let next = (app.table_state.selected().unwrap_or(0) + 1)
                .min(len.saturating_sub(1));
            app.table_state.select(Some(next));
            app.scroll_state = app.scroll_state.position(next);
        }
        KeyCode::Enter => {
            if let Some(idx) = app.table_state.selected() {
                if let Some(issue) = issues.get(idx) {
                    app.detail_issue = Some(issue.clone());
                    app.detail_scroll = 0;
                    app.screen = Screen::Detail;
                }
            }
        }
        KeyCode::Tab => {
            app.active_tab = (app.active_tab + 1) % 6;
            app.update_filter_cache();
            app.table_state.select(Some(0));
        }
        KeyCode::BackTab => {
            app.active_tab = (app.active_tab + 5) % 6;
            app.update_filter_cache();
            app.table_state.select(Some(0));
        }
        KeyCode::Char('1') => { app.filter_severity = None; app.update_filter_cache(); }
        KeyCode::Char('2') => { app.filter_severity = Some(IssueSeverity::Error); app.update_filter_cache(); }
        KeyCode::Char('3') => { app.filter_severity = Some(IssueSeverity::Warning); app.update_filter_cache(); }
        KeyCode::Char('4') => { app.filter_severity = Some(IssueSeverity::Info); app.update_filter_cache(); }
        KeyCode::Char('s') => {
            match export_json(app) {
                Ok(_) => app.save_success_time = Some(std::time::Instant::now()),
                Err(e) => {
                    app.scan_error = Some(format!("Export failed: {e}"));
                    app.screen = Screen::Menu;
                }
            }
        }
        KeyCode::Char('f') | KeyCode::Char('/') => app.search_mode = true,
        KeyCode::Char('c') => {
            if let Some(idx) = app.table_state.selected() {
                if let Some(issue) = issues.get(idx) {
                    let path = issue.file.to_string_lossy().to_string();
                    if copy_to_clipboard(&path).is_ok() {
                        app.copy_success_time = Some(std::time::Instant::now());
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_results_search(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter | KeyCode::Esc => app.search_mode = false,
        KeyCode::Backspace => {
            app.search_query.pop();
            app.update_filter_cache();
            app.table_state.select(Some(0));
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.update_filter_cache();
            app.table_state.select(Some(0));
        }
        _ => {}
    }
}

pub fn handle_detail(app: &mut App, key: KeyCode) {
    // If the patch success overlay is active, any key navigates to Results.
    if app.patch_success.is_some() {
        reset_detail(app);
        return;
    }

    // Scrolling applies in all detail sub-states
    match key {
        KeyCode::Up | KeyCode::Char('k') => {
            app.detail_scroll = app.detail_scroll.saturating_sub(1);
            return;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.detail_scroll = app.detail_scroll.saturating_add(1);
            return;
        }
        _ => {}
    }

    if app.detail_patch_confirm_mode {
        handle_patch_confirm(app, key);
        return;
    }

    if app.detail_llm_confirm_mode {
        handle_llm_confirm(app, key);
        return;
    }

    match key {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => reset_detail(app),
        KeyCode::Char('a') => handle_ask_llm(app),
        KeyCode::Char('p') => handle_apply_patch(app),
        _ => {}
    }
}

fn handle_patch_confirm(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('y') => {
            let result: Result<()> = (|| {
                let issue = app.detail_issue.as_ref().ok_or_else(|| anyhow::anyhow!("no issue"))?;
                let patch = app.detail_suggested_patch.as_ref().ok_or_else(|| anyhow::anyhow!("no patch"))?;
                apply_suggested_patch(issue, patch)
            })();

            match result {
                Ok(_) => {
                    let (file, line) = app.detail_issue.as_ref()
                        .map(|i| (i.file.clone(), i.line))
                        .unwrap_or_default();
                    app.detail_suggestion = None;
                    app.detail_suggested_patch = None;
                    app.detail_patch_error = None;
                    app.detail_patch_confirm_mode = false;
                    app.patch_success = Some(PatchSuccess {
                        file,
                        line,
                        at: std::time::Instant::now(),
                    });
                    // Rescan in background — results will be ready by the time
                    // the user dismisses the success overlay.
                    rescan_background(app);
                }
                Err(e) => {
                    app.detail_patch_error = Some(format!("Patch failed: {}", e));
                    app.detail_patch_confirm_mode = false;
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
            app.detail_patch_confirm_mode = false;
        }
        _ => {}
    }
}

fn handle_ask_llm(app: &mut App) {
    if matches!(
        app.detail_issue.as_ref().map(|i| &i.kind),
        Some(IssueKind::WrongFormat) | Some(IssueKind::OversizedFile)
    ) {
        app.detail_suggestion_error =
            Some("Patching for format conversion or oversized file issues is not supported.".into());
        return;
    }
    if app.detail_loading_suggestion {
        return;
    }

    // Skip confirmation when LLM_SKIP_CONFIRM=1/true/yes
    let skip = std::env::var("LLM_SKIP_CONFIRM")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    if skip {
        fire_llm(app);
    } else {
        app.detail_llm_confirm_mode = true;
    }
}

fn handle_llm_confirm(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('y') => {
            app.detail_llm_confirm_mode = false;
            fire_llm(app);
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
            app.detail_llm_confirm_mode = false;
        }
        _ => {}
    }
}

fn fire_llm(app: &mut App) {
    let Some(issue) = app.detail_issue.clone() else { return };
    let Some(client) = app.llm_client.clone() else {
        app.detail_suggestion_error = Some("LLM provider not configured. Check your .env file.".into());
        return;
    };
    app.detail_loading_suggestion = true;
    app.detail_suggestion = None;
    app.detail_suggested_patch = None;
    app.detail_suggestion_error = None;
    app.detail_scroll = 0;
    trigger_llm_suggest(app, issue, client);
}

fn handle_apply_patch(app: &mut App) {
    if matches!(
        app.detail_issue.as_ref().map(|i| &i.kind),
        Some(IssueKind::WrongFormat)
    ) {
        app.detail_patch_error =
            Some("Patching is not supported for format conversion issues.".into());
        return;
    }
    if app.detail_suggested_patch.is_some() {
        app.detail_patch_confirm_mode = true;
        app.detail_patch_error = None;
        app.detail_scroll = 0;
    } else {
        app.detail_patch_error =
            Some("No patch available. Press 'a' to ask the LLM first.".into());
    }
}

fn reset_detail(app: &mut App) {
    app.screen = Screen::Results;
    app.detail_issue = None;
    app.detail_suggestion = None;
    app.detail_suggested_patch = None;
    app.detail_suggestion_error = None;
    app.detail_loading_suggestion = false;
    app.detail_patch_confirm_mode = false;
    app.detail_llm_confirm_mode = false;
    app.detail_patch_error = None;
    app.detail_scroll = 0;
    app.patch_success = None;
}
