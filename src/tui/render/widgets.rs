use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::theme::*;

/// Returns a centered `Rect` inside `r` by percentage.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
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
        .split(vert[1])[1]
}

/// Render a transient toast notification centered on screen.
/// Returns `true` if the toast is still active (caller should keep the `Option<Instant>`).
pub fn render_toast(f: &mut Frame, message: &str, color: ratatui::style::Color) {
    let area = centered_rect(36, 14, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .style(Style::default().fg(TEXT_PRIMARY))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(color))
                    .style(Style::default().bg(BG_ELEVATED)),
            ),
        area,
    );
}

/// Build a standard help-bar `Line` from (key, description) pairs.
pub fn help_bar(pairs: &[(&str, &str)]) -> Line<'static> {
    // Collect owned Strings first so no '1 reference from `pairs` flows into the spans.
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, d)| ((*k).to_string(), format!(" {}", d)))
        .collect();

    let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
    for (i, (key, desc)) in owned.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", Style::default().fg(TEXT_MUTED)));
        }
        spans.push(Span::styled(key, Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(desc, Style::default().fg(TEXT_MUTED)));
    }
    Line::from(spans)
}

/// A horizontal gauge rendered with block characters: `████░░░░ 57%`
pub fn health_bar(score: f64, width: usize) -> String {
    let filled = ((score.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let empty = width - filled;
    format!("{}{} {:.0}%", "█".repeat(filled), "░".repeat(empty), score * 100.0)
}
