use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, Table,
        Tabs,
    },
};

use super::theme::*;
use super::widgets::{health_bar, help_bar, render_toast};
use crate::app::App;
use crate::scanner::{IssueSeverity, ScanResult};

pub fn draw_results(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG_DEEP)), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Length(4), // summary bar
            Constraint::Min(5),    // table
            Constraint::Length(3), // search
            Constraint::Length(2), // help
        ])
        .split(area);

    // ── Category tabs ─────────────────────────────────────────────────────
    let tab_labels = [
        "  All  ",
        "  Format  ",
        "  Dimensions  ",
        "  Lazy  ",
        "  Oversized  ",
        "  Srcset  ",
    ];

    f.render_widget(
        Tabs::new(tab_labels)
            .select(app.active_tab)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(BORDER_DIM))
                    .title(Span::styled(
                        " 🖼  Image Auditor — Results ",
                        Style::default()
                            .fg(ACCENT_CYAN)
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .highlight_style(
                Style::default()
                    .fg(ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
            .style(Style::default().fg(TEXT_LABEL)),
        chunks[0],
    );

    // ── Summary bar ───────────────────────────────────────────────────────
    if let Some(ref result) = app.scan_result {
        f.render_widget(build_summary(result), chunks[1]);
    }

    // ── Issues table ──────────────────────────────────────────────────────
    let issues = app.filtered_issues();
    let total = issues.len();

    let rows: Vec<Row> = issues
        .iter()
        .map(|issue| {
            let (sev_color, sev_sym) = match issue.severity {
                IssueSeverity::Error => (SEV_ERROR, "● ERR"),
                IssueSeverity::Warning => (SEV_WARNING, "◆ WRN"),
                IssueSeverity::Info => (SEV_INFO, "○ INF"),
            };
            let file_name = issue
                .file
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("?");
            let ext = issue
                .file
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();

            Row::new(vec![
                Cell::from(Span::styled(
                    sev_sym,
                    Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(file_name, Style::default().fg(TEXT_PRIMARY))),
                Cell::from(Span::styled(
                    ext,
                    Style::default().fg(TEXT_SECONDARY),
                )),
                Cell::from(Span::styled(
                    issue.line.to_string(),
                    Style::default().fg(TEXT_MUTED),
                )),
                Cell::from(Span::styled(
                    issue.kind.to_string(),
                    Style::default().fg(ACCENT_GREEN),
                )),
                Cell::from(Span::styled(
                    issue.message.chars().take(100).collect::<String>(),
                    Style::default().fg(TEXT_SECONDARY),
                )),
            ])
            .height(1)
        })
        .collect();

    app.scroll_state = app.scroll_state.content_length(total);

    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DEFAULT))
        .title(Span::styled(
            format!(
                " {} issues  ↑↓ navigate · Enter detail · Tab category · s save JSON ",
                total
            ),
            Style::default().fg(TEXT_LABEL),
        ));

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(24),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(24),
            Constraint::Min(30),
        ],
    )
    .header(
        Row::new(["Sev", "File", "Ext", "Line", "Issue Type", "Message"]).style(
            Style::default()
                .fg(TEXT_LABEL)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
    )
    .block(table_block)
    .row_highlight_style(
        Style::default()
            .bg(BG_HIGHLIGHT)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, chunks[2], &mut app.table_state);

    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"))
            .style(Style::default().fg(BORDER_DIM)),
        chunks[2].inner(Margin { vertical: 1, horizontal: 0 }),
        &mut app.scroll_state,
    );

    // ── Search bar ────────────────────────────────────────────────────────
    let (search_fg, border_fg) = if app.search_mode {
        (ACCENT_GREEN, ACCENT_GREEN)
    } else {
        (TEXT_SECONDARY, BORDER_DIM)
    };

    let search_text = if app.search_mode {
        format!("{}█", app.search_query)
    } else if app.search_query.is_empty() {
        "  press 'f' or '/' to filter by filename…".to_string()
    } else {
        format!("  {}", app.search_query)
    };

    f.render_widget(
        Paragraph::new(search_text)
            .style(Style::default().fg(search_fg))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_fg))
                    .title(Span::styled(
                        " 🔍 Filter by filename ",
                        Style::default().fg(TEXT_LABEL),
                    )),
            ),
        chunks[3],
    );

    // ── Help bar ──────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(help_bar(&[
            ("q", "back"),
            ("Tab", "category"),
            ("1-4", "severity"),
            ("f /", "search"),
            ("s", "save JSON"),
            ("c", "copy path"),
            ("Enter", "detail"),
        ]))
        .style(Style::default().bg(BG_DEEP)),
        chunks[4],
    );

    // ── Toast notifications ───────────────────────────────────────────────
    if let Some(t) = app.save_success_time {
        if t.elapsed() < std::time::Duration::from_secs(3) {
            render_toast(f, "✓  Report saved to image-audit-report.json", ACCENT_GREEN);
        } else {
            app.save_success_time = None;
        }
    }
    if let Some(t) = app.copy_success_time {
        if t.elapsed() < std::time::Duration::from_secs(3) {
            render_toast(f, "✓  Path copied to clipboard", ACCENT_GREEN);
        } else {
            app.copy_success_time = None;
        }
    }
}

// ─── Summary widget ───────────────────────────────────────────────────────────

fn build_summary(result: &ScanResult) -> Paragraph<'_> {
    let errors = result
        .issues
        .iter()
        .filter(|i| matches!(i.severity, IssueSeverity::Error))
        .count();
    let warnings = result
        .issues
        .iter()
        .filter(|i| matches!(i.severity, IssueSeverity::Warning))
        .count();
    let infos = result
        .issues
        .iter()
        .filter(|i| matches!(i.severity, IssueSeverity::Info))
        .count();

    let total_issues = result.issues.len();
    let score = if result.images_found == 0 {
        1.0_f64
    } else {
        1.0 - (total_issues as f64 / (result.images_found as f64 * 4.0)).min(1.0)
    };
    let bar = health_bar(score, 20);
    let score_color = if score >= 0.8 {
        ACCENT_GREEN
    } else if score >= 0.5 {
        SEV_WARNING
    } else {
        SEV_ERROR
    };

    let line1 = Line::from(vec![
        Span::styled("  Files  ", Style::default().fg(TEXT_LABEL)),
        Span::styled(
            result.files_scanned.to_string(),
            Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::styled("    Images  ", Style::default().fg(TEXT_LABEL)),
        Span::styled(
            result.images_found.to_string(),
            Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::styled("    Total issues  ", Style::default().fg(TEXT_LABEL)),
        Span::styled(
            total_issues.to_string(),
            Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        ),
    ]);

    let line2 = Line::from(vec![
        Span::styled("  ● ", Style::default().fg(SEV_ERROR)),
        Span::styled(
            format!("{} errors", errors),
            Style::default().fg(SEV_ERROR),
        ),
        Span::styled("  ◆ ", Style::default().fg(SEV_WARNING)),
        Span::styled(
            format!("{} warnings", warnings),
            Style::default().fg(SEV_WARNING),
        ),
        Span::styled("  ○ ", Style::default().fg(SEV_INFO)),
        Span::styled(
            format!("{} info", infos),
            Style::default().fg(SEV_INFO),
        ),
        Span::styled("    Health  ", Style::default().fg(TEXT_LABEL)),
        Span::styled(bar, Style::default().fg(score_color)),
    ]);

    Paragraph::new(vec![Line::from(""), line1, line2]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER_DIM)),
    )
}
