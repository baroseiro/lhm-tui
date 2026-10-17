//! Root layout: header, panel composition and footer (SPEC §6.1).
//!
//! Panels come from `ui.layout` expanded over the current bindings (preset
//! panel list); consecutive GPU panels share a row when the terminal is wide
//! enough. Below 60×20 a dedicated too-small screen is shown (SPEC N8).

pub mod format;
pub mod presets;
pub mod widgets;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, LinkStatus};
use crate::bindings::Bindings;
use crate::config::Config;
use crate::lhm::model::Snapshot;
use crate::theme::Theme;
use crate::ui::presets::PanelKind;
use crate::util::human_bytes;

pub fn draw(frame: &mut Frame, app: &AppState, cfg: &Config) {
    let theme = Theme::from_config(&cfg.theme);
    let area = frame.area();

    if area.width < 60 || area.height < 20 {
        render_too_small(frame, area, &theme);
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    if cfg.ui.show_header {
        frame.render_widget(header_line(app, cfg, area), header);
    }

    let stale = matches!(app.link, LinkStatus::Stale | LinkStatus::Reconnecting)
        && app.last_fetch.is_some();
    match (app.snapshot(), app.bindings.as_ref()) {
        (Some(snapshot), Some(bindings)) if bindings.has_any() => {
            render_panels(frame, body, app, snapshot, bindings, cfg, &theme, stale);
        }
        _ => render_status(frame, body, app, cfg, &theme),
    }

    if cfg.ui.show_footer {
        frame.render_widget(footer_line(cfg), footer);
    }
}

fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let block = Block::bordered().border_type(BorderType::Rounded);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = Text::from(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Terminal too small",
            Style::new().fg(theme.warn).bold(),
        )),
        Line::from(Span::styled(
            "lhm-tui needs at least 60×20 columns",
            Style::new().fg(theme.muted),
        )),
    ]);
    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn dot(app: &AppState) -> Span<'static> {
    match app.link {
        LinkStatus::Connecting => Span::styled("○ ", Style::new().fg(Color::DarkGray)),
        LinkStatus::Live => Span::styled("● ", Style::new().fg(Color::Green)),
        LinkStatus::Stale => Span::styled("● ", Style::new().fg(Color::Yellow)),
        LinkStatus::Reconnecting => Span::styled("● ", Style::new().fg(Color::Red)),
    }
}

fn link_label(app: &AppState) -> Span<'static> {
    match app.link {
        LinkStatus::Connecting => Span::styled("connecting", Style::new().fg(Color::DarkGray)),
        LinkStatus::Live => Span::styled("live", Style::new().fg(Color::Green)),
        LinkStatus::Stale => Span::styled(
            format!("stale {}s", app.seconds_since_live()),
            Style::new().fg(Color::Yellow),
        ),
        LinkStatus::Reconnecting => Span::styled(
            format!("reconnecting… (retry in {}s)", app.retry_seconds()),
            Style::new().fg(Color::Red),
        ),
    }
}

fn header_line(app: &AppState, cfg: &Config, size: Rect) -> Paragraph<'static> {
    let (dot_span, label) = if app.paused {
        (
            Span::styled("○ ", Style::new().fg(Color::DarkGray)),
            Span::styled("paused", Style::new().fg(Color::DarkGray)),
        )
    } else {
        (dot(app), link_label(app))
    };
    let line = Line::from(vec![
        Span::styled(" lhm-tui ", Style::new().bold().fg(Color::Cyan)),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::new().fg(Color::DarkGray),
        ),
        dot_span,
        label,
        Span::styled(
            format!("   {}×{}", size.width, size.height),
            Style::new().fg(Color::DarkGray),
        ),
        Span::styled("   ", Style::new()),
        Span::styled(cfg.server.url.clone(), Style::new().fg(Color::DarkGray)),
    ]);
    Paragraph::new(line)
}

fn footer_line(cfg: &Config) -> Paragraph<'static> {
    let first = |action: &str| {
        cfg.keys
            .get(action)
            .and_then(|keys| keys.first())
            .cloned()
            .unwrap_or_else(|| "?".to_string())
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", first("quit")),
            Style::new().bold().fg(Color::Cyan),
        ),
        Span::styled("quit", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "   ·   {} pause   ·   {} focus   ·   {} sort   ·   j/k scroll",
                first("pause"),
                first("focus_next"),
                first("sort"),
            ),
            Style::new().fg(Color::DarkGray),
        ),
    ]);
    Paragraph::new(line)
}

/// STATUS screen shown before the first snapshot (or without bindings).
fn render_status(frame: &mut Frame, area: Rect, app: &AppState, cfg: &Config, theme: &Theme) {
    let url = cfg.server.url.trim_end_matches('/').to_string();
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" LibreHardwareMonitor ");

    let mut lines: Vec<Line> = vec![Line::from("")];
    match app.link {
        LinkStatus::Connecting => {
            lines.push(Line::from(Span::styled(
                "Connecting to LibreHardwareMonitor…",
                Style::new().bold(),
            )));
        }
        LinkStatus::Live => {
            if let Some(fetched) = &app.last_fetch {
                let snapshot = &fetched.snapshot;
                lines.push(Line::from(vec![
                    Span::styled("● ", Style::new().fg(Color::Green)),
                    Span::styled("Live", Style::new().bold().fg(Color::Green)),
                ]));
                lines.push(Line::from(Span::styled(
                    format!(
                        "LHM {}  ·  {} hardware  ·  {} sensors",
                        snapshot
                            .lhm_version
                            .clone()
                            .unwrap_or_else(|| "?".to_string()),
                        snapshot.hardware_count(),
                        snapshot.sensor_count(),
                    ),
                    Style::new().fg(theme.muted),
                )));
                lines.push(Line::from(Span::styled(
                    format!(
                        "last poll {} in {} ms",
                        human_bytes(fetched.bytes),
                        fetched.latency.as_millis()
                    ),
                    Style::new().fg(theme.muted),
                )));
            }
        }
        LinkStatus::Stale => {
            lines.push(Line::from(vec![
                Span::styled("● ", Style::new().fg(Color::Yellow)),
                Span::styled(
                    format!(
                        "No data for {}s — showing last values",
                        app.seconds_since_live()
                    ),
                    Style::new().fg(Color::Yellow),
                ),
            ]));
        }
        LinkStatus::Reconnecting => {
            lines.push(Line::from(vec![
                Span::styled("● ", Style::new().fg(Color::Red)),
                Span::styled(
                    "Cannot reach LibreHardwareMonitor",
                    Style::new().fg(Color::Red),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "retrying in {}s (attempt {})",
                    app.retry_seconds(),
                    app.failed_attempts
                ),
                Style::new().fg(theme.muted),
            )));
        }
    }

    if let Some(error) = &app.last_error {
        if app.link != LinkStatus::Live {
            lines.push(Line::from(Span::styled(
                error.clone(),
                Style::new().fg(Color::Red),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("GET {url}/data.json"),
        Style::new().fg(theme.muted),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("session up {}s", app.started_at.elapsed().as_secs()),
        Style::new().fg(theme.muted),
    )));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .alignment(Alignment::Center),
        area,
    );
}

/// Compose the preset panels into rows; consecutive GPUs share a row when
/// width ≥ 120 and `panels.gpu.per_row` allows it (SPEC §6.1).
#[allow(clippy::too_many_arguments)]
fn render_panels(
    frame: &mut Frame,
    area: Rect,
    app: &AppState,
    snapshot: &Snapshot,
    bindings: &Bindings,
    cfg: &Config,
    theme: &Theme,
    stale: bool,
) {
    let panels = presets::panel_list(cfg, bindings);
    if panels.is_empty() {
        render_status(frame, area, app, cfg, theme);
        return;
    }

    let per_row = if area.width >= 120 {
        cfg.panels.gpu.per_row.max(1) as usize
    } else {
        1
    };

    let mut rows: Vec<Vec<&presets::PanelSpec>> = Vec::new();
    let mut heights: Vec<u16> = Vec::new();
    let mut index = 0;
    while index < panels.len() {
        let spec = &panels[index];
        if matches!(spec.kind, PanelKind::Gpu(_)) {
            let mut group = vec![spec];
            let mut next = index + 1;
            while group.len() < per_row
                && next < panels.len()
                && matches!(panels[next].kind, PanelKind::Gpu(_))
            {
                group.push(&panels[next]);
                next += 1;
            }
            heights.push(presets::panel_height(spec, bindings, cfg));
            rows.push(group);
            index = next;
        } else {
            heights.push(presets::panel_height(spec, bindings, cfg));
            rows.push(vec![spec]);
            index += 1;
        }
    }

    let constraints: Vec<Constraint> = heights.iter().map(|h| Constraint::Length(*h)).collect();
    let row_areas = Layout::vertical(constraints).split(area);
    let focused_index = app.focus % panels.len();
    let sort = app.effective_sort(cfg);

    for (row, row_area) in rows.iter().zip(row_areas.iter()) {
        let columns: Vec<Rect> = if row.len() == 1 {
            vec![*row_area]
        } else {
            Layout::horizontal(vec![Constraint::Ratio(1, row.len() as u32); row.len()])
                .split(*row_area)
                .to_vec()
        };
        for (spec, column) in row.iter().zip(columns) {
            let index = panels
                .iter()
                .position(|panel| panel.id == spec.id)
                .unwrap_or(0);
            let scroll = app.scrolls.get(&spec.id).copied().unwrap_or(0);
            presets::draw_panel(
                frame,
                column,
                spec,
                snapshot,
                bindings,
                cfg,
                theme,
                stale,
                index == focused_index,
                sort,
                scroll,
            );
        }
    }
}
