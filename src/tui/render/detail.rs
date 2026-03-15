use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use super::theme::*;
use super::widgets::{centered_rect, help_bar};
use crate::app::{App, PatchSuccess};
use crate::scanner::{IssueKind, IssueSeverity};

pub fn draw_detail(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG_DEEP)), area);

    // Check patch success overlay first (owns the data we need)
    let show_success = app
        .patch_success
        .as_ref()
        .map(|ps| ps.at.elapsed() < std::time::Duration::from_secs(4))
        .unwrap_or(false);

    if show_success {
        if let Some(ref ps) = app.patch_success {
            draw_patch_success(f, ps, area);
            return;
        }
    }

    // ── Extract owned data — scope ends before build_body re-borrows app ──
    let (sev_color, sev_label, kind_str, file_str, ext_str, line_str, msg_str, snip_str, kind_clone) = {
        let issue = match app.detail_issue.as_ref() {
            Some(i) => i,
            None => return,
        };
        let sev_color = match issue.severity {
            IssueSeverity::Error   => SEV_ERROR,
            IssueSeverity::Warning => SEV_WARNING,
            IssueSeverity::Info    => SEV_INFO,
        };
        let sev_label = match issue.severity {
            IssueSeverity::Error   => "ERROR",
            IssueSeverity::Warning => "WARNING",
            IssueSeverity::Info    => "INFO",
        };
        (
            sev_color,
            sev_label,
            issue.kind.to_string(),
            issue.file.to_string_lossy().into_owned(),
            issue.file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default(),
            issue.line.to_string(),
            issue.message.clone(),
            issue.snippet.clone(),
            issue.kind.clone(),
        )
    }; // borrow of app.detail_issue ends here

    let popup_area = centered_rect(84, 90, area);
    f.render_widget(Clear, popup_area);

    // Split popup: header + body
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(14), // metadata (taller for multi-line snippet)
            Constraint::Min(5),     // suggestion / diff
            Constraint::Length(2),  // help bar
        ])
        .margin(1)
        .split(popup_area);

    // ── Outer frame ───────────────────────────────────────────────────────
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(sev_color))
            .style(Style::default().bg(BG_SURFACE))
            .title(Span::styled(
                format!(" ◈ Issue Detail  [{}] ", sev_label),
                Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
            )),
        popup_area,
    );

    // ── Metadata block ────────────────────────────────────────────────────
    let mut meta_lines: Vec<Line<'static>> = vec![
        label_value("Kind    ", &kind_str,  ACCENT_GREEN),
        label_value("File    ", &file_str,  TEXT_PRIMARY),
        label_value("Ext     ", &ext_str,   TEXT_SECONDARY),
        label_value("Line    ", &line_str,  TEXT_SECONDARY),
        Line::from(""),
        label_value("Message ", &msg_str,   TEXT_PRIMARY),
        Line::from(""),
    ];
    meta_lines.extend(snippet_lines(&snip_str));
    meta_lines.push(Line::from(""));
    meta_lines.push(advice_line(&kind_clone));

    f.render_widget(
        Paragraph::new(meta_lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(BORDER_DIM)),
            ),
        layout[0],
    );

    // ── LLM / diff body ───────────────────────────────────────────────────
    let body_lines = build_body(app);

    let visible = layout[1].height.saturating_sub(2) as usize;
    let max_scroll = body_lines.len().saturating_sub(visible);
    let scroll = (app.detail_scroll as usize).min(max_scroll) as u16;

    f.render_widget(
        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        layout[1],
    );

    // ── Help ──────────────────────────────────────────────────────────────
    let help = if app.detail_patch_confirm_mode {
        help_bar(&[("y", "apply"), ("n / Esc", "cancel"), ("↑↓ j k", "scroll")])
    } else if app.detail_llm_confirm_mode {
        help_bar(&[("y", "confirm"), ("n / Esc", "cancel")])
    } else {
        help_bar(&[
            ("a", "ask LLM"),
            ("p", "apply patch"),
            ("↑↓ j k", "scroll"),
            ("Esc / q", "back"),
        ])
    };

    f.render_widget(
        Paragraph::new(help).style(Style::default().bg(BG_SURFACE)),
        layout[2],
    );

    // ── LLM confirmation modal (rendered last so it sits on top) ──────────
    if app.detail_llm_confirm_mode {
        draw_llm_confirm(f, area);
    }
}

// ─── Body content builder ────────────────────────────────────────────────────

fn build_body(app: &App) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if app.detail_loading_suggestion {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ⏳  Asking LLM for a fix suggestion…",
            Style::default().fg(ACCENT_BLUE),
        )));
        return lines;
    }

    if let Some(ref err) = app.detail_suggestion_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ⚠  LLM error: {}", err),
            Style::default().fg(SEV_ERROR),
        )));
        return lines;
    }

    if let Some(ref err) = app.detail_patch_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ⚠  Patch error: {}", err),
            Style::default().fg(SEV_ERROR),
        )));
        lines.push(Line::from(""));
    }

    // LLM suggestion text — always shown when present.
    // In verbose mode the response contains both prose AND a ---PATCH--- block;
    // we strip the raw patch block from the text display because the diff view
    // below renders it properly.
    if let Some(ref text) = app.detail_suggestion {
        let display_text: String = match text.find("---PATCH---") {
            Some(idx) => text[..idx].trim_end().to_string(),
            None => text.clone(),
        };
        if !display_text.is_empty() {
            lines.push(section_header("LLM Suggestion"));
            lines.push(Line::from(""));
            for l in display_text.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", l),
                    Style::default().fg(TEXT_SECONDARY),
                )));
            }
            lines.push(Line::from(""));
        }
    }

    // Diff view
    if app.detail_patch_confirm_mode {
        if let Some(ref patch) = app.detail_suggested_patch {
            lines.push(Line::from(""));
            lines.push(diff_header("  ── Patch Preview ──────────────────────────────────────────"));
            lines.push(Line::from(""));

            // File header bar
            if let Some(ref issue) = app.detail_issue {
                let label = format!(
                    "  📄 {}  :{}",
                    issue.file.display(),
                    issue.line
                );
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(DIFF_HDR_FG).bg(DIFF_HDR_BG),
                )));
            }
            lines.push(Line::from(""));

            // Deletions
            for l in patch.before.lines() {
                lines.push(Line::from(vec![
                    Span::styled("  ─ ", Style::default().fg(DIFF_DEL_FG).bg(DIFF_DEL_BG)),
                    Span::styled(
                        l.to_owned(),
                        Style::default().fg(DIFF_DEL_FG).bg(DIFF_DEL_BG),
                    ),
                ]));
            }

            lines.push(Line::from(Span::styled(
                "  ──────────────────────────────────────────────────────────",
                Style::default().fg(BORDER_DIM),
            )));

            // Additions
            for l in patch.after.lines() {
                lines.push(Line::from(vec![
                    Span::styled("  + ", Style::default().fg(DIFF_ADD_FG).bg(DIFF_ADD_BG)),
                    Span::styled(
                        l.to_owned(),
                        Style::default().fg(DIFF_ADD_FG).bg(DIFF_ADD_BG),
                    ),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Press y to apply, n / Esc to cancel.",
                Style::default()
                    .fg(SEV_WARNING)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            return lines;
        }
    }

    // Patch badge (non-confirm mode)
    if app.detail_suggested_patch.is_some() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ✦ ", Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(
                "Patch ready — press ",
                Style::default().fg(TEXT_SECONDARY),
            ),
            Span::styled(
                "p",
                Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to preview & apply.", Style::default().fg(TEXT_SECONDARY)),
        ]));
        return lines;
    }

    // Idle state
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press 'a' to ask the LLM for a fix suggestion.",
        Style::default().fg(TEXT_MUTED),
    )));

    lines
}

// ─── Success overlay ──────────────────────────────────────────────────────────

fn draw_patch_success(f: &mut Frame, ps: &PatchSuccess, area: Rect) {
    let popup = centered_rect(60, 50, area);
    f.render_widget(Clear, popup);

    let file_name = ps
        .file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ps.file.to_string_lossy().to_string());

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  ✔  Patch applied successfully!",
            Style::default()
                .fg(ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  File  ", Style::default().fg(TEXT_LABEL)),
            Span::styled(file_name, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Line  ", Style::default().fg(TEXT_LABEL)),
            Span::styled(
                ps.line.to_string(),
                Style::default().fg(TEXT_PRIMARY),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  The source file has been updated in-place.",
            Style::default().fg(TEXT_SECONDARY),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Results updated — press Esc to return to the list.",
            Style::default().fg(TEXT_MUTED),
        )),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(content)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Double)
                    .border_style(Style::default().fg(ACCENT_GREEN))
                    .style(Style::default().bg(BG_ELEVATED))
                    .title(Span::styled(
                        " ◈ Patch Success ",
                        Style::default()
                            .fg(ACCENT_GREEN)
                            .add_modifier(Modifier::BOLD),
                    )),
            ),
        popup,
    );
}

// ─── LLM confirmation modal ───────────────────────────────────────────────────

fn draw_llm_confirm(f: &mut Frame, area: Rect) {
    let popup = centered_rect(52, 36, area);
    f.render_widget(Clear, popup);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  This will send the issue context to your",
            Style::default().fg(TEXT_SECONDARY),
        )),
        Line::from(Span::styled(
            "  configured LLM provider and consume tokens.",
            Style::default().fg(TEXT_SECONDARY),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Set LLM_SKIP_CONFIRM=1 in .env to skip this.",
            Style::default().fg(TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Press ", Style::default().fg(TEXT_SECONDARY)),
            Span::styled("y", Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" to proceed or ", Style::default().fg(TEXT_SECONDARY)),
            Span::styled("n / Esc", Style::default().fg(SEV_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(" to cancel.", Style::default().fg(TEXT_SECONDARY)),
        ]),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(SEV_WARNING))
                    .style(Style::default().bg(BG_ELEVATED))
                    .title(Span::styled(
                        " ⚡ Ask LLM for a fix? ",
                        Style::default().fg(SEV_WARNING).add_modifier(Modifier::BOLD),
                    )),
            ),
        popup,
    );
}

// ─── Line helpers ─────────────────────────────────────────────────────────────

fn label_value(label: &str, value: &str, color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {}", label),
            Style::default().fg(TEXT_LABEL),
        ),
        Span::styled(value.to_owned(), Style::default().fg(color)),
    ])
}

fn snippet_lines(snippet: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    out.push(Line::from(Span::styled("  Snippet  ", Style::default().fg(TEXT_LABEL))));
    for l in snippet.lines() {
        out.push(Line::from(Span::styled(
            format!("    {}", l),
            Style::default().fg(ACCENT_CYAN).add_modifier(Modifier::ITALIC),
        )));
    }
    out
}

fn advice_line(kind: &IssueKind) -> Line<'static> {
    let text = match kind {
        IssueKind::WrongFormat =>
            "  💡 Convert to WebP using cwebp or squoosh. AVIF gives best compression.",
        IssueKind::MissingAlt =>
            "  💡 Add alt attribute for accessibility and SEO.",
        IssueKind::MissingWidthHeight =>
            "  💡 Add explicit width and height to prevent layout shift (CLS).",
        IssueKind::MissingLazyLoading =>
            "  💡 Add loading=\"lazy\" for below-fold images. Use loading=\"eager\" for LCP.",
        IssueKind::OversizedFile =>
            "  💡 Compress below 200 KiB via squoosh, imagemin, or a CDN transform.",
        IssueKind::MissingSrcset =>
            "  💡 Add srcset with multiple resolutions, e.g.: srcset=\"img-400.webp 400w, img-800.webp 800w\"",
    };
    Line::from(Span::styled(text, Style::default().fg(TEXT_MUTED)))
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  ── {} ", title),
        Style::default().fg(TEXT_LABEL).add_modifier(Modifier::BOLD),
    ))
}

fn diff_header(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_owned(),
        Style::default().fg(DIFF_HDR_FG).add_modifier(Modifier::BOLD),
    ))
}
