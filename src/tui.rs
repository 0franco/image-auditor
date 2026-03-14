use std::{
    io,
    path::PathBuf,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Tabs, Wrap,
    },
    Frame, Terminal,
};

use crate::config::load_llm_config;
use crate::llm::{create_llm_client, LlmClient, LlmSuggestion, SuggestedPatch};
use crate::scanner::{scan_directory, Issue, IssueKind, IssueSeverity, ScanResult};

#[derive(PartialEq)]
enum Screen {
    Menu,
    Scanning,
    Results,
    Detail,
}

pub struct App {
    screen: Screen,
    // Menu
    menu_state: ListState,
    input_path: String,
    input_mode: bool,
    // Scanning
    scan_start: Option<Instant>,
    scan_result: Option<ScanResult>,
    scan_error: Option<String>,
    // Results table
    table_state: TableState,
    scroll_state: ScrollbarState,
    active_tab: usize,
    // Detail
    detail_issue: Option<Issue>,
    detail_suggestion: Option<String>,
    detail_suggested_patch: Option<SuggestedPatch>,
    detail_loading_suggestion: bool,
    detail_suggestion_error: Option<String>,
    detail_suggestion_rx: Option<mpsc::Receiver<anyhow::Result<LlmSuggestion>>>,
    detail_patch_confirm_mode: bool,
    detail_patch_error: Option<String>,
    // Filter
    filter_severity: Option<IssueSeverity>,
    search_query: String,
    search_mode: bool,
    // Caching
    cached_filtered_indices: Vec<usize>,
    scan_rx: Option<std::sync::mpsc::Receiver<Result<ScanResult>>>,
    running: bool,
    save_success_time: Option<Instant>,
    copy_success_time: Option<Instant>,
    llm_client: Option<Arc<dyn LlmClient>>,
    detail_scroll: u16,
}

impl App {
    pub fn new() -> Self {
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            screen: Screen::Menu,
            menu_state,
            input_path: String::from("."),
            input_mode: false,
            scan_start: None,
            scan_result: None,
            scan_error: None,
            table_state,
            scroll_state: ScrollbarState::default(),
            active_tab: 0,
            detail_issue: None,
            detail_suggestion: None,
            detail_suggested_patch: None,
            detail_loading_suggestion: false,
            detail_suggestion_error: None,
            detail_suggestion_rx: None,
            detail_patch_confirm_mode: false,
            detail_patch_error: None,
            filter_severity: None,
            search_query: String::new(),
            search_mode: false,
            cached_filtered_indices: Vec::new(),
            scan_rx: None,
            running: true,
            save_success_time: None,
            copy_success_time: None,
            llm_client: None,
            detail_scroll: 0,
        }
    }

    fn update_filter_cache(&mut self) {
        let Some(ref result) = self.scan_result else {
            self.cached_filtered_indices = Vec::new();
            return;
        };

        self.cached_filtered_indices = result.issues.iter().enumerate()
            .filter(|(_, i)| {
                if self.search_query.is_empty() {
                    return true;
                }
                let file_name = i.file.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                file_name.contains(&self.search_query.to_lowercase())
            })
            .filter(|(_, i)| match (&self.filter_severity, &i.severity) {
                (Some(IssueSeverity::Error), IssueSeverity::Error) => true,
                (Some(IssueSeverity::Warning), IssueSeverity::Warning) => true,
                (Some(IssueSeverity::Info), IssueSeverity::Info) => true,
                (None, _) => true,
                _ => false,
            })
            .filter(|(_, i)| match self.active_tab {
                0 => true,
                1 => matches!(i.kind, IssueKind::WrongFormat),
                2 => matches!(i.kind, IssueKind::MissingWidthHeight),
                3 => matches!(i.kind, IssueKind::MissingLazyLoading),
                4 => matches!(i.kind, IssueKind::OversizedFile),
                5 => matches!(i.kind, IssueKind::MissingSrcset),
                _ => true,
            })
            .map(|(idx, _)| idx)
            .collect();
    }

    fn filtered_issues(&self) -> Vec<Issue> {
        let Some(ref result) = self.scan_result else { return vec![] };
        self.cached_filtered_indices.iter()
            .filter_map(|&idx| result.issues.get(idx).cloned())
            .collect()
    }
}

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

    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {e}");
    }

    Ok(())
}

fn start_scan(app: &mut App) {
    app.screen = Screen::Scanning;
    app.scan_start = Some(Instant::now());
    app.scan_result = None;
    app.scan_error = None;

    let path = PathBuf::from(&app.input_path);
    let (tx, rx) = std::sync::mpsc::channel();
    app.scan_rx = Some(rx);

    std::thread::spawn(move || {
        let result = scan_directory(&path);
        let _ = tx.send(result);
    });
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, app)).map_err(|e| anyhow::anyhow!(e.to_string()))?;

        if !app.running {
            break Ok(());
        }

        // Poll scan result
        if let Some(ref rx) = app.scan_rx {
            if let Ok(result) = rx.try_recv() {
                app.scan_rx = None;
                match result {
                    Ok(res) => {
                        app.scan_result = Some(res);
                        app.update_filter_cache();
                        app.screen = Screen::Results;
                        app.table_state.select(Some(0));
                    }
                    Err(e) => {
                        app.scan_error = Some(e.to_string());
                        app.screen = Screen::Menu;
                    }
                }
            }
        }

        // Poll LLM suggestion result
        if let Some(ref rx) = app.detail_suggestion_rx {
            if let Ok(result) = rx.try_recv() {
                app.detail_loading_suggestion = false;
                match result {
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
                app.detail_suggestion_rx = None;
            }
        }

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.screen {
                    Screen::Menu => handle_menu_input(app, key.code, key.modifiers),
                    Screen::Results => handle_results_input(app, key.code),
                    Screen::Detail => handle_detail_input(app, key.code),
                    Screen::Scanning => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn handle_menu_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    if app.input_mode {
        match key {
            KeyCode::Enter => {
                if app.input_path.trim().is_empty() {
                    app.scan_error = Some("Please enter a path to scan.".to_string());
                } else {
                    app.input_mode = false;
                    start_scan(app);
                }
            }
            KeyCode::Esc => { app.input_mode = false; }
            KeyCode::Backspace => { app.input_path.pop(); }
            KeyCode::Char(c) => { app.input_path.push(c); }
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Char('q') | KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
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
        KeyCode::Enter | KeyCode::Char(' ') => {
            match app.menu_state.selected() {
                Some(0) => { app.input_mode = true; }
                Some(1) => { app.running = false; }
                _ => {}
            }
        }
        _ => {}
    }
}

fn handle_results_input(app: &mut App, key: KeyCode) {
    if app.search_mode {
        match key {
            KeyCode::Enter | KeyCode::Esc => {
                app.search_mode = false;
            }
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
            let i = app.table_state.selected().unwrap_or(0);
            let next = i.saturating_sub(1);
            app.table_state.select(Some(next));
            app.scroll_state = app.scroll_state.position(next);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app.table_state.selected().unwrap_or(0);
            let next = (i + 1).min(len.saturating_sub(1));
            app.table_state.select(Some(next));
            app.scroll_state = app.scroll_state.position(next);
        }
        KeyCode::Enter => {
            if let Some(idx) = app.table_state.selected() {
                if let Some(issue) = issues.get(idx) {
                    app.detail_issue = Some(issue.clone());
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
        KeyCode::Char('1') => {
            app.filter_severity = None;
            app.update_filter_cache();
        }
        KeyCode::Char('2') => {
            app.filter_severity = Some(IssueSeverity::Error);
            app.update_filter_cache();
        }
        KeyCode::Char('3') => {
            app.filter_severity = Some(IssueSeverity::Warning);
            app.update_filter_cache();
        }
        KeyCode::Char('4') => {
            app.filter_severity = Some(IssueSeverity::Info);
            app.update_filter_cache();
        }
        KeyCode::Char('s') => {
            if let Err(e) = export_json(app) {
                app.scan_error = Some(format!("Export failed: {e}"));
                app.screen = Screen::Menu;
            } else {
                app.save_success_time = Some(Instant::now());
            }
        }
        KeyCode::Char('f') | KeyCode::Char('/') => {
            app.search_mode = true;
        }
        KeyCode::Char('c') => {
            if let Some(idx) = app.table_state.selected() {
                if let Some(issue) = issues.get(idx) {
                    let path = issue.file.to_string_lossy().to_string();
                    if copy_to_clipboard(&path).is_ok() {
                        app.copy_success_time = Some(Instant::now());
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_detail_input(app: &mut App, key: KeyCode) {
    // Scroll in all detail modes (normal + patch preview)
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
        match key {
            KeyCode::Char('y') => {
                if let (Some(issue), Some(patch)) =
                    (app.detail_issue.as_ref(), app.detail_suggested_patch.as_ref())
                {
                    if let Err(e) = apply_suggested_patch(issue, patch) {
                        app.detail_patch_error = Some(format!("Patch failed: {}", e));
                    } else {
                        app.detail_patch_error = None;
                    }
                }
                app.detail_patch_confirm_mode = false;
            }
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
                app.detail_patch_confirm_mode = false;
            }
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
            app.screen = Screen::Results;
            app.detail_issue = None;
            app.detail_suggestion = None;
            app.detail_suggested_patch = None;
            app.detail_suggestion_error = None;
            app.detail_loading_suggestion = false;
            app.detail_patch_confirm_mode = false;
            app.detail_patch_error = None;
            app.detail_scroll = 0;
        }
        KeyCode::Char('a') => {
            if app.detail_loading_suggestion {
                return;
            }
            let Some(issue) = app.detail_issue.clone() else { return };
            let Some(client) = app.llm_client.clone() else {
                app.detail_suggestion_error = Some("LLM provider not configured.".to_string());
                return;
            };

            app.detail_loading_suggestion = true;
            app.detail_suggestion = None;
            app.detail_suggested_patch = None;
            app.detail_suggestion_error = None;
            app.detail_scroll = 0;

            let prompt = build_issue_prompt(&issue);
            let (tx, rx) = mpsc::channel();
            app.detail_suggestion_rx = Some(rx);

            let prompt_owned = prompt.clone();

            std::thread::spawn(move || {
                let result = client.suggest_fix(&prompt_owned);
                let _ = tx.send(result);
            });
        }
        KeyCode::Char('p') => {
            if app.detail_suggested_patch.is_some() {
                app.detail_patch_confirm_mode = true;
                app.detail_patch_error = None;
            } else {
                app.detail_patch_error =
                    Some("No patch available from LLM. Press 'a' to ask first.".to_string());
            }
        }
        _ => {}
    }
}

fn export_json(app: &App) -> Result<()> {
    if let Some(ref result) = app.scan_result {
        let json = serde_json::to_string_pretty(&result)?;
        std::fs::write("image-audit-report.json", json)?;
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::process::{Command, Stdio};
    use std::io::Write;

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    child.wait()?;
    Ok(())
}

// ─── UI ─────────────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Menu => draw_menu(f, app),
        Screen::Scanning => draw_scanning(f, app),
        Screen::Results => draw_results(f, app),
        Screen::Detail => draw_detail(f, app),
    }
}

fn draw_menu(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Background
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(15, 15, 25))),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(9),
            Constraint::Length(3),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .margin(3)
        .split(area);

    // Title banner
    let banner = vec![
        Line::from(vec![
            Span::styled("  ██╗███╗   ███╗ █████╗  ██████╗ ███████╗", Style::default().fg(Color::Rgb(99, 179, 237))),
        ]),
        Line::from(vec![
            Span::styled("  ██║████╗ ████║██╔══██╗██╔════╝ ██╔════╝", Style::default().fg(Color::Rgb(99, 179, 237))),
        ]),
        Line::from(vec![
            Span::styled("  ██║██╔████╔██║███████║██║  ███╗█████╗  ", Style::default().fg(Color::Rgb(129, 199, 247))),
        ]),
        Line::from(vec![
            Span::styled("  ██║██║╚██╔╝██║██╔══██║██║   ██║██╔══╝  ", Style::default().fg(Color::Rgb(129, 199, 247))),
        ]),
        Line::from(vec![
            Span::styled("  ██║██║ ╚═╝ ██║██║  ██║╚██████╔╝███████╗", Style::default().fg(Color::Rgb(159, 219, 255))),
        ]),
        Line::from(vec![
            Span::styled("  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝", Style::default().fg(Color::Rgb(159, 219, 255))),
        ]),
        Line::from(vec![
            Span::styled("          A U D I T O R  ·  v0.1.0", Style::default().fg(Color::Rgb(100, 120, 160)).add_modifier(Modifier::ITALIC)),
        ]),
    ];

    f.render_widget(Paragraph::new(banner).alignment(Alignment::Left), chunks[1]);

    // Path input
    let input_style = if app.input_mode {
        Style::default().fg(Color::Rgb(99, 235, 99)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(200, 200, 220))
    };

    let border_color = if app.input_mode {
        Color::Rgb(99, 235, 99)
    } else {
        Color::Rgb(70, 80, 120)
    };

    let path_display = if app.input_mode {
        format!("{}█", app.input_path)
    } else {
        app.input_path.clone()
    };

    let input = Paragraph::new(path_display)
        .style(input_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " 📁 Scan Path ",
                    Style::default().fg(Color::Rgb(180, 180, 220)).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(input, chunks[2]);

    // Menu items
    let menu_items = vec![
        ListItem::new(Line::from(vec![
            Span::styled("  ▶  ", Style::default().fg(Color::Rgb(99, 235, 99))),
            Span::styled("Scan Directory", Style::default().fg(Color::Rgb(220, 220, 240)).add_modifier(Modifier::BOLD)),
            Span::styled("   (press Enter to edit path, then Enter to scan)", Style::default().fg(Color::Rgb(100, 110, 140))),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("  ✕  ", Style::default().fg(Color::Rgb(235, 99, 99))),
            Span::styled("Quit", Style::default().fg(Color::Rgb(220, 220, 240)).add_modifier(Modifier::BOLD)),
        ])),
    ];

    let menu = List::new(menu_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(60, 70, 110)))
                .title(Span::styled(" Menu ", Style::default().fg(Color::Rgb(150, 160, 200)))),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(30, 40, 80))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    f.render_stateful_widget(menu, chunks[3], &mut app.menu_state);

    // Hint
    if let Some(ref err) = app.scan_error {
        let error = Paragraph::new(format!("⚠ Error: {}", err))
            .style(Style::default().fg(Color::Rgb(235, 99, 99)));
        f.render_widget(error, chunks[4]);
    }
}

fn draw_scanning(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(15, 15, 25))), area);

    let elapsed = app.scan_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let dots = ".".repeat((elapsed % 4) as usize);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  🔍 Scanning codebase...",
            Style::default().fg(Color::Rgb(99, 179, 237)).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Path: {}", app.input_path),
            Style::default().fg(Color::Rgb(150, 160, 200)),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Analyzing HTML,PHTML, JS, TS, JSX, TSX files{}", dots),
            Style::default().fg(Color::Rgb(100, 200, 130)),
        )),
    ];

    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(60, 80, 140)))
                .title(" Image Auditor "),
        ),
        area,
    );
}

fn draw_results(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(12, 12, 20))), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Length(3), // summary bar
            Constraint::Min(5),    // table
            Constraint::Length(3), // search
            Constraint::Length(2), // help
        ])
        .split(area);

    // Tabs
    let tab_titles = vec!["All", "Format", "Dimensions", "Lazy Load", "Oversized", "Srcset"];
    let tabs = Tabs::new(tab_titles)
        .select(app.active_tab)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(60, 70, 110)))
                .title(Span::styled(
                    " 🖼  Image Auditor — Results ",
                    Style::default().fg(Color::Rgb(159, 219, 255)).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(99, 235, 180))
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .style(Style::default().fg(Color::Rgb(140, 150, 190)));

    f.render_widget(tabs, chunks[0]);

    // Summary bar
    if let Some(ref result) = app.scan_result {
        let errors = result.issues.iter().filter(|i| matches!(i.severity, IssueSeverity::Error)).count();
        let warnings = result.issues.iter().filter(|i| matches!(i.severity, IssueSeverity::Warning)).count();
        let infos = result.issues.iter().filter(|i| matches!(i.severity, IssueSeverity::Info)).count();

        let summary = Paragraph::new(Line::from(vec![
            Span::styled("  Files: ", Style::default().fg(Color::Rgb(120, 130, 160))),
            Span::styled(result.files_scanned.to_string(), Style::default().fg(Color::Rgb(200, 210, 240)).add_modifier(Modifier::BOLD)),
            Span::styled("   Images: ", Style::default().fg(Color::Rgb(120, 130, 160))),
            Span::styled(result.images_found.to_string(), Style::default().fg(Color::Rgb(200, 210, 240)).add_modifier(Modifier::BOLD)),
            Span::styled("   Issues: ", Style::default().fg(Color::Rgb(120, 130, 160))),
            Span::styled(result.issues.len().to_string(), Style::default().fg(Color::Rgb(200, 210, 240)).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled("● ", Style::default().fg(Color::Rgb(235, 80, 80))),
            Span::styled(format!("{} errors", errors), Style::default().fg(Color::Rgb(200, 100, 100))),
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(Color::Rgb(235, 180, 60))),
            Span::styled(format!("{} warnings", warnings), Style::default().fg(Color::Rgb(200, 160, 80))),
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(Color::Rgb(99, 179, 237))),
            Span::styled(format!("{} info", infos), Style::default().fg(Color::Rgb(100, 150, 200))),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(40, 50, 90))),
        );

        f.render_widget(summary, chunks[1]);
    }

    // Issues table
    let issues = app.filtered_issues();
    let total = issues.len();

    let rows: Vec<Row> = issues.iter().map(|issue| {
        let (sev_color, sev_sym) = match issue.severity {
            IssueSeverity::Error =>   (Color::Rgb(235, 80, 80),  "● ERR "),
            IssueSeverity::Warning => (Color::Rgb(235, 180, 60), "◆ WRN "),
            IssueSeverity::Info =>    (Color::Rgb(99, 179, 237), "○ INF "),
        };

        let file_name = issue.file
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("?");

        let file_type = issue.file.extension().map(|ext| ext.to_string_lossy().to_string()).unwrap_or_default();

        let kind_str = issue.kind.to_string();

        Row::new(vec![
            Cell::from(Span::styled(sev_sym, Style::default().fg(sev_color).add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled(file_name, Style::default().fg(Color::Rgb(180, 190, 220)))),
            Cell::from(Span::styled(file_type, Style::default().fg(Color::Rgb(180, 190, 220)))),
            Cell::from(Span::styled(issue.line.to_string(), Style::default().fg(Color::Rgb(120, 130, 160)))),
            Cell::from(Span::styled(kind_str, Style::default().fg(Color::Rgb(150, 200, 180)))),
            Cell::from(Span::styled(
                issue.message.chars().take(100).collect::<String>(),
                Style::default().fg(Color::Rgb(170, 175, 200)),
            )),
        ])
        .height(1)
    }).collect();

    app.scroll_state = app.scroll_state.content_length(total);

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(22),
            Constraint::Length(5),
            Constraint::Length(24),
            Constraint::Min(50),
        ],
    )
    .header(
        Row::new(vec!["Sev", "File", "FileType", "Line", "Issue Type", "Message"])
            .style(Style::default().fg(Color::Rgb(100, 110, 160)).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(50, 60, 100)))
            .title(Span::styled(
                format!(" {} issues (↑↓ navigate · Enter = detail · Tab = filter · s = save JSON) ", total),
                Style::default().fg(Color::Rgb(120, 130, 170)),
            )),
    )
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(25, 35, 70))
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, chunks[2], &mut app.table_state);

    // Scrollbar
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"))
            .style(Style::default().fg(Color::Rgb(60, 70, 110))),
        chunks[2].inner(Margin { vertical: 1, horizontal: 0 }),
        &mut app.scroll_state,
    );

    // Search bar
    let search_style = if app.search_mode {
        Style::default().fg(Color::Rgb(99, 235, 99)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(140, 150, 190))
    };

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.search_mode { Color::Rgb(99, 235, 99) } else { Color::Rgb(40, 50, 90) }))
        .title(Span::styled(" 🔍 Search by filename ", Style::default().fg(Color::Rgb(120, 130, 170))));

    let search_display = if app.search_mode {
        format!("{}█", app.search_query)
    } else if app.search_query.is_empty() {
        " (press 'f' or '/' to search)".to_string()
    } else {
        app.search_query.clone()
    };

    let search_p = Paragraph::new(search_display)
        .style(search_style)
        .block(search_block);

    f.render_widget(search_p, chunks[3]);

    // Help line
    let help = Paragraph::new(Line::from(vec![
        Span::styled("  q", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=back  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("Tab", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=category  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("1-4", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=severity filter  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("f", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=search  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("s", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=save JSON  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("c", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=copy path  ", Style::default().fg(Color::Rgb(80, 90, 130))),
        Span::styled("Enter", Style::default().fg(Color::Rgb(99, 235, 180)).add_modifier(Modifier::BOLD)),
        Span::styled("=detail", Style::default().fg(Color::Rgb(80, 90, 130))),
    ]))
    .style(Style::default().bg(Color::Rgb(12, 12, 20)));

    f.render_widget(help, chunks[4]);
    
    // Success message overlay
    if let Some(instant) = app.save_success_time {
        if instant.elapsed() < Duration::from_secs(3) {
            let area = centered_rect(30, 10, f.area());
            f.render_widget(Clear, area);
            f.render_widget(
                Paragraph::new("Report saved successfully!")
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::Rgb(99, 235, 180))),
                    ),
                area,
            );
        } else {
            app.save_success_time = None;
        }
    }

    if let Some(instant) = app.copy_success_time {
        if instant.elapsed() < Duration::from_secs(3) {
            let area = centered_rect(30, 10, f.area());
            f.render_widget(Clear, area);
            f.render_widget(
                Paragraph::new("Path copied to clipboard!")
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::Rgb(99, 235, 180))),
                    ),
                area,
            );
        } else {
            app.copy_success_time = None;
        }
    }
}

fn draw_detail(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(10, 10, 18))), area);

    let Some(ref issue) = app.detail_issue else { return };

    // Overlay popup
    let popup_area = centered_rect(80, 60, area);
    f.render_widget(Clear, popup_area);

    let (sev_color, sev_label) = match issue.severity {
        IssueSeverity::Error =>   (Color::Rgb(235, 80, 80), "ERROR"),
        IssueSeverity::Warning => (Color::Rgb(235, 180, 60), "WARNING"),
        IssueSeverity::Info =>    (Color::Rgb(99, 179, 237), "INFO"),
    };

    let file_path = issue.file.to_string_lossy().to_string();
    let file_type = issue.file.extension().map(|ext| ext.to_string_lossy().to_string()).unwrap_or_default();

    let mut content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Severity  ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(sev_label, Style::default().fg(sev_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Issue     ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(issue.kind.to_string(), Style::default().fg(Color::Rgb(150, 220, 200)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  File      ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(&file_path, Style::default().fg(Color::Rgb(180, 190, 220))),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  File type      ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(&file_type, Style::default().fg(Color::Rgb(180, 190, 220))),
        ]),
        Line::from(vec![
            Span::styled("  Line      ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(issue.line.to_string(), Style::default().fg(Color::Rgb(180, 190, 220))),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Message   ", Style::default().fg(Color::Rgb(100, 110, 160))),
            Span::styled(&issue.message, Style::default().fg(Color::Rgb(220, 220, 240))),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Snippet   ", Style::default().fg(Color::Rgb(100, 110, 160))),
        ]),
        Line::from(vec![
            Span::styled(format!("  {}", issue.snippet), Style::default().fg(Color::Rgb(100, 200, 130)).add_modifier(Modifier::ITALIC)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            fix_advice(&issue.kind),
            Style::default().fg(Color::Rgb(140, 150, 180)),
        )),
        Line::from(""),
    ];

    // LLM suggestion state
    if app.detail_loading_suggestion {
        content.push(Line::from(Span::styled(
            "  ⏳ Asking LLM for fix suggestion...",
            Style::default().fg(Color::Rgb(99, 179, 237)),
        )));
        content.push(Line::from(""));
    } else if let Some(ref err) = app.detail_suggestion_error {
        content.push(Line::from(Span::styled(
            format!("  ⚠ LLM error: {}", err),
            Style::default().fg(Color::Rgb(235, 80, 80)),
        )));
        content.push(Line::from(""));
    } else if let Some(ref suggestion) = app.detail_suggestion {
        content.push(Line::from(Span::styled(
            "  LLM Suggestion",
            Style::default()
                .fg(Color::Rgb(150, 220, 200))
                .add_modifier(Modifier::BOLD),
        )));
        content.push(Line::from(""));
        for line in suggestion.lines() {
            content.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::Rgb(200, 210, 240)),
            )));
        }
        content.push(Line::from(""));
    }

    // Patch availability / preview
    if app.detail_patch_confirm_mode {
        content.push(Line::from(Span::styled(
            "  Patch Preview",
            Style::default()
                .fg(Color::Rgb(99, 235, 180))
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(ref patch) = app.detail_suggested_patch {
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                "  Before:",
                Style::default().fg(Color::Rgb(235, 80, 80)),
            )));
            for line in patch.before.lines() {
                content.push(Line::from(Span::styled(
                    format!("- {}", line),
                    Style::default().fg(Color::Rgb(235, 120, 120)),
                )));
            }
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                "  After:",
                Style::default().fg(Color::Rgb(99, 235, 180)),
            )));
            for line in patch.after.lines() {
                content.push(Line::from(Span::styled(
                    format!("+ {}", line),
                    Style::default().fg(Color::Rgb(140, 240, 200)),
                )));
            }
            content.push(Line::from(""));
        }
        content.push(Line::from(Span::styled(
            "  [↑/↓ or j/k] Scroll  ·  [y] Apply patch   [n / Esc / q] Cancel",
            Style::default().fg(Color::Rgb(80, 100, 150)),
        )));
    } else {
        if app.detail_suggested_patch.is_some() {
            content.push(Line::from(Span::styled(
                "  Patch available (press 'p' to preview & apply)",
                Style::default().fg(Color::Rgb(150, 220, 200)),
            )));
        }
        if let Some(ref err) = app.detail_patch_error {
            content.push(Line::from(Span::styled(
                format!("  ⚠ Patch error: {}", err),
                Style::default().fg(Color::Rgb(235, 80, 80)),
            )));
        }
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            "  [a] Ask LLM for fix  ·  [p] Preview/apply patch  ·  [↑/↓ or j/k] Scroll  ·  [Esc / q] Back",
            Style::default().fg(Color::Rgb(80, 100, 150)),
        )));
    }

    // Clamp scroll so we don't go past the end unnecessarily
    let visible_lines = popup_area.height.saturating_sub(4) as usize;
    let max_scroll = content.len().saturating_sub(visible_lines);
    let clamped_scroll = app
        .detail_scroll
        .min(max_scroll as u16);

    f.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .scroll((clamped_scroll, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Double)
                    .border_style(Style::default().fg(sev_color))
                    .title(Span::styled(
                        " Issue Detail ",
                        Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
                    )),
            ),
        popup_area,
    );
}

fn fix_advice(kind: &IssueKind) -> String {
    match kind {
        IssueKind::WrongFormat =>
            "  💡 Fix: Convert to WebP using cwebp or squoosh. Use AVIF for best compression.".to_string(),
        IssueKind::MissingWidthHeight =>
            "  💡 Fix: Add explicit width and height attributes to prevent layout shift (CLS).".to_string(),
        IssueKind::MissingLazyLoading =>
            "  💡 Fix: Add loading=\"lazy\" for below-the-fold images. Use loading=\"eager\" for LCP image.".to_string(),
        IssueKind::OversizedFile =>
            "  💡 Fix: Compress image below 200 KiB. Use squoosh, imagemin, or a CDN transform.".to_string(),
        IssueKind::MissingSrcset =>
            "  💡 Fix: Add srcset with multiple resolutions, e.g.: srcset=\"img-400.webp 400w, img-800.webp 800w\"".to_string(),
    }
}

fn build_issue_prompt(issue: &Issue) -> String {
    let file_path = issue.file.to_string_lossy();
    let file_type = issue
        .file
        .extension()
        .map(|ext| ext.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let verbose = std::env::var("AI_VERBOSE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    if !verbose {
        return format!(
            "You are helping improve image delivery and related HTML/JS/TS code.\n\
File: {file_path}\n\
File type: {file_type}\n\
Line: {line}\n\
Issue kind: {kind}\n\
Message: {msg}\n\
Snippet:\n{snippet}\n\n\
Your task:\n\
- Propose a safe, minimal code change that fixes this specific issue.\n\
- Output STRICTLY ONE structured patch block in this exact format, with NO explanations, NO prose, and NO markdown fences:\n\
---PATCH---\n\
file: {file_path}\n\
BEFORE:\n\
<code to replace>\n\
---END_BEFORE---\n\
AFTER:\n\
<replacement code>\n\
---END_AFTER---\n\
---END_PATCH---\n\
If and only if you truly cannot propose a safe patch, output exactly:\n\
NO_PATCH",
            line = issue.line,
            kind = issue.kind.to_string(),
            msg = issue.message,
            snippet = issue.snippet,
        );
    }

    format!(
        "You are helping improve image delivery and related HTML/JS/TS code.\n\
File: {file_path}\n\
File type: {file_type}\n\
Line: {line}\n\
Issue kind: {kind}\n\
Message: {msg}\n\
Snippet:\n{snippet}\n\n\
First, briefly explain how to fix this issue and, if helpful, show a small corrected code example.\n\
Then, if possible, emit a structured patch block using exactly this format:\n\
---PATCH---\n\
file: {file_path}\n\
BEFORE:\n\
<code to replace>\n\
---END_BEFORE---\n\
AFTER:\n\
<replacement code>\n\
---END_AFTER---\n\
---END_PATCH---\n\
If you cannot safely propose a concrete patch, omit the PATCH block entirely.",
        line = issue.line,
        kind = issue.kind.to_string(),
        msg = issue.message,
        snippet = issue.snippet,
    )
}

fn apply_suggested_patch(issue: &Issue, patch: &SuggestedPatch) -> Result<()> {
    use std::fs;

    let file_path = &issue.file;
    let contents = fs::read_to_string(file_path)?;

    let before = patch.before.trim_matches('\n');
    let after = patch.after.trim_matches('\n');

    let idx = contents
        .find(before)
        .ok_or_else(|| anyhow::anyhow!("Original snippet not found in file"))?;

    let mut new_contents = String::with_capacity(contents.len() + after.len());
    new_contents.push_str(&contents[..idx]);
    new_contents.push_str(after);
    new_contents.push_str(&contents[idx + before.len()..]);

    fs::write(file_path, new_contents)?;
    Ok(())
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}