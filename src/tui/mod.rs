pub mod actions;
pub mod input;
pub mod render;

use std::{io, path::PathBuf, time::{Duration, Instant}};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};

use crate::app::{App, Screen};
use crate::config::load_llm_config;
use crate::llm::create_llm_client;

use actions::start_scan;
use input::{handle_detail, handle_menu, handle_results};
use render::{
    detail::draw_detail,
    menu::draw_menu,
    results::draw_results,
    scanning::draw_scanning,
};

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run(path: Option<PathBuf>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    if let Ok(Some(cfg)) = load_llm_config() {
        if let Ok(client) = create_llm_client(&cfg) {
            app.llm_client = Some(client);
        }
    }
    if let Some(p) = path {
        app.input_path = p.to_string_lossy().to_string();
        start_scan(&mut app);
    }

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

// ─── Event loop ───────────────────────────────────────────────────────────────

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    const TICK: Duration = Duration::from_millis(80);
    let mut last_tick = Instant::now();

    loop {
        terminal
            .draw(|f| draw(f, app))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

        if !app.running {
            break Ok(());
        }

        poll_scan(app);
        poll_llm(app);
        poll_patch_success(app);

        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.screen {
                    Screen::Menu => handle_menu(app, key.code, key.modifiers),
                    Screen::Results => handle_results(app, key.code),
                    Screen::Detail => handle_detail(app, key.code),
                    Screen::Scanning => {} // no input while scanning
                }
            }
        }

        if last_tick.elapsed() >= TICK {
            app.tick = app.tick.wrapping_add(1);
            last_tick = Instant::now();
        }
    }
}

// ─── Async result polling ────────────────────────────────────────────────────

fn poll_scan(app: &mut App) {
    let result = app.scan_rx.as_ref().and_then(|rx| rx.try_recv().ok());
    if let Some(outcome) = result {
        app.scan_rx = None;
        match outcome {
            Ok(res) => {
                app.scan_result = Some(res);
                app.update_filter_cache();
                // Post-patch background scan: stay on Detail so the success
                // overlay remains visible; reset_detail will go to Results.
                if app.screen != Screen::Detail {
                    app.screen = Screen::Results;
                    app.table_state.select(Some(0));
                }
            }
            Err(e) => {
                app.scan_error = Some(e.to_string());
                app.screen = Screen::Menu;
            }
        }
    }
}

fn poll_llm(app: &mut App) {
    let result = app
        .detail_suggestion_rx
        .as_ref()
        .and_then(|rx| rx.try_recv().ok());
    if let Some(outcome) = result {
        app.detail_suggestion_rx = None;
        app.detail_loading_suggestion = false;
        match outcome {
            Ok(suggestion) => {
                app.detail_suggestion = Some(suggestion.text);
                app.detail_suggested_patch = suggestion.patch;
                app.detail_suggestion_error = None;
            }
            Err(e) => {
                app.detail_suggestion_error = Some(e.to_string());
                app.detail_suggestion = None;
                app.detail_suggested_patch = None;
            }
        }
    }
}

// ─── Draw dispatch ────────────────────────────────────────────────────────────

fn poll_patch_success(app: &mut App) {
    if app
        .patch_success
        .as_ref()
        .map(|ps| ps.at.elapsed() >= Duration::from_secs(4))
        .unwrap_or(false)
    {
        // Timer expired with no keypress — navigate to Results and clear all
        // detail state so the user never lands back on stale issue data.
        app.patch_success = None;
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
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Menu => draw_menu(f, app),
        Screen::Scanning => draw_scanning(f, app),
        Screen::Results => draw_results(f, app),
        Screen::Detail => draw_detail(f, app),
    }
}
