//! Root layout: header, panel composition and footer (SPEC §6.1).
//!
//! Panels come from `ui.layout` expanded over the current bindings (preset
//! panel list); consecutive GPU panels share a row when the terminal is wide
//! enough. Chart panels shrink first when space runs out, and below 60×20 a
//! dedicated too-small screen is shown (SPEC N8).

pub mod format;
pub mod presets;
pub mod widgets;

use std::time::Instant;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, LinkStatus};
use crate::bindings::Bindings;
use crate::config::Config;
use crate::theme::Theme;
use crate::ui::presets::PanelCtx;
use crate::util::human_bytes;

pub fn draw(frame: &mut Frame, app: &mut AppState, cfg: &Config) {
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
    let has_panels =
        app.snapshot().is_some() && app.bindings.as_ref().is_some_and(Bindings::has_any);
    if has_panels {
        render_panels(frame, body, app, cfg, &theme, stale);
    } else {
        frame.render_widget(status_paragraph(app, cfg, &theme), body);
    }

    if cfg.ui.show_footer {
        frame.render_widget(footer_line(app, cfg), footer);
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

fn footer_line(app: &AppState, cfg: &Config) -> Paragraph<'static> {
    let first = |action: &str| {
        cfg.keys
            .get(action)
            .and_then(|keys| keys.first())
            .cloned()
            .unwrap_or_else(|| "?".to_string())
    };
    let window = app.effective_window(cfg).1;
    let line = Line::from(vec![
        Span::styled(format!(" {} ", first("quit")), Style::new().bold().fg(Color::Cyan)),
        Span::styled("quit", Style::new().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "   ·   {} pause   ·   {} focus   ·   {} sort   ·   j/k scroll   ·   {} window({window})   ·   {} temps   ·   {} charts",
                first("pause"),
                first("focus_next"),
                first("sort"),
                first("window_next"),
                first("toggle_thermal"),
                first("toggle_charts"),
            ),
            Style::new().fg(Color::DarkGray),
        ),
    ]);
    Paragraph::new(line)
}

/// Status screen shown before the first snapshot (or without bindings).
fn status_paragraph(app: &AppState, cfg: &Config, theme: &Theme) -> Paragraph<'static> {
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

    Paragraph::new(Text::from(lines))
        .block(block)
        .alignment(Alignment::Center)
}

/// Compose the preset panels into rows; consecutive GPUs share a row when
/// width ≥ 120 and `panels.gpu.per_row` allows it (SPEC §6.1).
fn render_panels(
    frame: &mut Frame,
    area: Rect,
    app: &mut AppState,
    cfg: &Config,
    theme: &Theme,
    stale: bool,
) {
    // Values that only need `&AppState` are read before the split borrows.
    let sort = app.effective_sort(cfg);
    let ascii = app.effective_ascii(cfg);
    let charts_on = app.effective_charts(cfg);
    let overrides = presets::PanelOverrides {
        thermal: Some(app.effective_thermal(cfg)),
    };
    let (window, window_label) = app.effective_window(cfg);
    let now = Instant::now();

    // Disjoint field borrows so history/data and chart state coexist.
    let AppState {
        last_fetch,
        bindings,
        history,
        charts,
        scrolls,
        focus,
        ..
    } = app;
    let (Some(fetched), Some(bindings)) = (last_fetch.as_deref(), bindings.as_ref()) else {
        return;
    };
    let snapshot = &fetched.snapshot;
    if !bindings.has_any() {
        return;
    }

    let panels = presets::panel_list(cfg, bindings, overrides);
    if panels.is_empty() {
        return;
    }

    let per_row = if area.width >= 120 {
        cfg.panels.gpu.per_row.max(1) as usize
    } else {
        1
    };
    let rows = presets::layout_rows(&panels, per_row);
    let heights = presets::fit_heights(&panels, bindings, cfg, area.height, charts_on, &rows);
    let row_heights: Vec<u16> = rows.iter().map(|row| heights[row[0]]).collect();

    let constraints: Vec<Constraint> = row_heights
        .iter()
        .map(|height| Constraint::Length(*height))
        .collect();
    let row_areas = Layout::vertical(constraints).split(area);
    let focused_index = *focus % panels.len();

    for (row, row_area) in rows.iter().zip(row_areas.iter()) {
        let columns: Vec<Rect> = if row.len() == 1 {
            vec![*row_area]
        } else {
            Layout::horizontal(vec![Constraint::Ratio(1, row.len() as u32); row.len()])
                .split(*row_area)
                .to_vec()
        };
        for (panel_index, column) in row.iter().zip(columns) {
            let spec = &panels[*panel_index];
            let scroll = scrolls.get(&spec.id).copied().unwrap_or(0);
            let mut ctx = PanelCtx {
                snapshot,
                bindings,
                cfg,
                theme,
                history: history.as_ref(),
                charts: &mut *charts,
                charts_enabled: charts_on,
                stale,
                ascii,
                focused: *panel_index == focused_index,
                sort,
                scroll,
                window,
                window_label: window_label.clone(),
                now,
            };
            presets::draw_panel(frame, column, spec, &mut ctx);
        }
    }
}
