use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap},
};

use ratatui::style::Color;

use super::theme::*;
use crate::app::App;

pub fn draw_menu(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(BG_DEEP)),
        area,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(9),  // banner
            Constraint::Length(3),  // path input
            Constraint::Length(6),  // menu items
            Constraint::Min(0),     // error / padding
        ])
        .margin(3)
        .split(area);

    // ── ASCII banner ──────────────────────────────────────────────────────
    let banner_lines = [
        ("  ██╗███╗   ███╗ █████╗  ██████╗ ███████╗", Color::Rgb(99, 179, 237)),
        ("  ██║████╗ ████║██╔══██╗██╔════╝ ██╔════╝", Color::Rgb(99, 179, 237)),
        ("  ██║██╔████╔██║███████║██║  ███╗█████╗  ", Color::Rgb(129, 199, 247)),
        ("  ██║██║╚██╔╝██║██╔══██║██║   ██║██╔══╝  ", Color::Rgb(129, 199, 247)),
        ("  ██║██║ ╚═╝ ██║██║  ██║╚██████╔╝███████╗", Color::Rgb(159, 219, 255)),
        ("  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝", Color::Rgb(159, 219, 255)),
    ];

    let mut banner: Vec<Line> = banner_lines
        .iter()
        .map(|(text, color)| Line::from(Span::styled(*text, Style::default().fg(*color))))
        .collect();

    banner.push(Line::from(Span::styled(
        "          A U D I T O R  ·  v0.2.0",
        Style::default()
            .fg(Color::Rgb(90, 110, 155))
            .add_modifier(Modifier::ITALIC),
    )));

    f.render_widget(
        Paragraph::new(banner).alignment(Alignment::Left),
        chunks[1],
    );

    // ── Path input ────────────────────────────────────────────────────────
    let (input_style, border_color) = if app.input_mode {
        (
            Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD),
            ACCENT_GREEN,
        )
    } else {
        (Style::default().fg(TEXT_PRIMARY), BORDER_DEFAULT)
    };

    let path_text = if app.input_mode {
        format!("{}█", app.input_path)
    } else {
        app.input_path.clone()
    };

    f.render_widget(
        Paragraph::new(path_text)
            .style(input_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .title(Span::styled(
                        " 📁 Scan Path ",
                        Style::default().fg(TEXT_SECONDARY).add_modifier(Modifier::BOLD),
                    )),
            )
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    // ── Menu list ─────────────────────────────────────────────────────────
    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled("  ▶  ", Style::default().fg(ACCENT_GREEN)),
            Span::styled(
                "Scan Directory",
                Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   Enter to edit path",
                Style::default().fg(TEXT_MUTED),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("  ✕  ", Style::default().fg(SEV_ERROR)),
            Span::styled(
                "Quit",
                Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ])),
    ];

    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(BORDER_DIM))
                    .title(Span::styled(" Menu ", Style::default().fg(TEXT_LABEL))),
            )
            .highlight_style(Style::default().bg(BG_HIGHLIGHT).add_modifier(Modifier::BOLD))
            .highlight_symbol(""),
        chunks[3],
        &mut app.menu_state,
    );

    // ── Error message ─────────────────────────────────────────────────────
    if let Some(ref err) = app.scan_error {
        f.render_widget(
            Paragraph::new(format!("  ⚠  {}", err))
                .style(Style::default().fg(SEV_ERROR)),
            chunks[4],
        );
    }
}
