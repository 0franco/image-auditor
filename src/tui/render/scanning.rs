use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::theme::*;
use crate::app::App;

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw_scanning(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG_DEEP)), area);

    let elapsed = app.scan_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let spinner = SPINNER[(app.tick as usize / 2) % SPINNER.len()];

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Min(10),
            Constraint::Percentage(35),
        ])
        .split(area)[1];

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {} ", spinner),
                Style::default().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Scanning codebase…",
                Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Path  ", Style::default().fg(TEXT_LABEL)),
            Span::styled(&app.input_path, Style::default().fg(ACCENT_BLUE)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ext   ", Style::default().fg(TEXT_LABEL)),
            Span::styled(
                "html · phtml · js · ts · jsx · tsx · vue · svelte · php",
                Style::default().fg(TEXT_SECONDARY),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Time  ", Style::default().fg(TEXT_LABEL)),
            Span::styled(
                format!("{}s elapsed", elapsed),
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(BORDER_DEFAULT))
                    .style(Style::default().bg(BG_SURFACE))
                    .title(Span::styled(
                        " Image Auditor ",
                        Style::default().fg(ACCENT_CYAN).add_modifier(Modifier::BOLD),
                    )),
            ),
        inner,
    );
}
