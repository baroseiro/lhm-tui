//! Root layout: header, panel composition, status line and overlays (SPEC §6.1).
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
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, LinkStatus};
use crate::bindings::Bindings;
use crate::charts::Series;
use crate::config::Config;
use crate::theme::Theme;
use crate::ui::presets::PanelCtx;
use crate::util::human_bytes;

pub fn draw(frame: &mut Frame, app: &mut AppState, cfg: &Config) {
    let mut theme = Theme::from_config(&cfg.theme);
    // NO_COLOR (environment) and `--no-color`/`ui.no_color` strip colours;
    // RGB values are then downgraded for the terminal's colour depth (T4.3).
    if cfg.ui.no_color || std::env::var_os("NO_COLOR").is_some() {
        theme = Theme::plain();
    }
    theme = theme.downgraded(crate::theme::detect_color_depth());
    let area = frame.area();

    if area.width < 60 || area.height < 20 {
        render_too_small(frame, area, &theme);
        return;
    }

    let [header, toast, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(app.toast.is_some())),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    if cfg.ui.show_header {
        frame.render_widget(header_line(app, cfg, area, &theme), header);
    }
    if let Some((message, _)) = &app.toast {
        frame.render_widget(
            Paragraph::new(Span::styled(message.clone(), Style::new().fg(theme.warn))),
            toast,
        );
    }

    let stale = matches!(app.link, LinkStatus::Stale | LinkStatus::Reconnecting)
        && app.last_fetch.is_some();
    let has_panels =
        app.snapshot().is_some() && app.bindings.as_ref().is_some_and(Bindings::has_any);
    if let Some(sensor_id) = app.adhoc_sensor.clone() {
        render_adhoc(frame, body, app, cfg, &theme, &sensor_id);
    } else if has_panels {
        render_panels(frame, body, app, cfg, &theme, stale);
    } else {
        frame.render_widget(status_paragraph(app, cfg, &theme), body);
    }

    if cfg.ui.show_footer {
        frame.render_widget(status_line(app, &theme), status);
    }
    if app.help_visible {
        render_help(frame, area, cfg, &theme);
    }
    if app.debug_visible {
        render_debug(frame, area, app, cfg, &theme);
    }
    if app.browser_open {
        render_browser(frame, area, app, &theme);
    }
}

/// Ad-hoc fullscreen graph for a sensor picked in the browser (T5.2).
fn render_adhoc(
    frame: &mut Frame,
    area: Rect,
    app: &mut AppState,
    cfg: &Config,
    theme: &Theme,
    sensor_id: &str,
) {
    let Some(fetched) = app.last_fetch.clone() else {
        return;
    };
    let snapshot = &fetched.snapshot;
    let mut found = None;
    for hardware in snapshot.all_hardware() {
        if let Some(sensor) = hardware.sensor(sensor_id) {
            found = Some((
                format!("{} · {}", hardware.name, sensor.name),
                sensor.clone(),
            ));
            break;
        }
    }

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" Sensor · {sensor_id} "))
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let Some((label, sensor)) = found else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "sensor not found in the current snapshot",
                Style::new().fg(theme.warn),
            )),
            inner,
        );
        return;
    };

    let (window, window_label) = app.effective_window(cfg);
    let poll_ms = cfg.server.poll_interval_ms.max(1);
    let points = app
        .history
        .as_ref()
        .map(|history| {
            history.window(
                sensor_id,
                (window.as_millis() / u128::from(poll_ms)).max(1) as usize,
            )
        })
        .unwrap_or_default();

    if cfg.charts.enabled && !points.is_empty() && inner.height >= 4 {
        let empty_bindings = crate::bindings::Bindings::default();
        let ascii = app.effective_ascii(cfg);
        let mut ctx = PanelCtx {
            snapshot,
            bindings: &empty_bindings,
            cfg,
            theme,
            history: app.history.as_ref(),
            charts: &mut app.charts,
            charts_enabled: cfg.charts.enabled,
            stale: false,
            ascii,
            focused: false,
            sort: cfg.ui.table_sort,
            scroll: 0,
            window,
            window_label,
            now: Instant::now(),
        };
        let series = Series {
            label,
            color: theme.accent,
            latest_text: format::sensor_value(&sensor),
            latest_style: theme.sensor_style(&sensor.kind, sensor.value),
            points,
        };
        presets::draw_series_graph(
            frame,
            inner,
            &mut ctx,
            format!("adhoc:{sensor_id}"),
            vec![series],
            crate::config::YMode::Auto,
        );
        return;
    }

    let lines = vec![
        Line::from(Span::styled(label, Style::new().fg(theme.muted))),
        Line::from(""),
        Line::from(Span::styled(
            format::sensor_value(&sensor),
            Style::new().fg(theme.accent).bold(),
        )),
        Line::from(Span::styled(
            format!(
                "LHM min {} · max {}",
                format::stat(&sensor.kind, sensor.min),
                format::stat(&sensor.kind, sensor.max)
            ),
            Style::new().fg(theme.muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "enable charts (c) for a live graph",
            Style::new().fg(theme.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Sensor browser overlay: filter, navigate, Enter graphs (T5.2).
fn render_browser(frame: &mut Frame, area: Rect, app: &AppState, theme: &Theme) {
    let width = area.width.saturating_sub(10).clamp(40, 90);
    let height = area.height.saturating_sub(6).clamp(8, 30);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let matches = app.browser_matches();
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" Sensors — {} match(es) ", matches.len()))
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" filter: ", Style::new().fg(theme.muted)),
        Span::styled(
            format!("{}▏", app.browser_query),
            Style::new().fg(theme.accent),
        ),
    ]));
    let visible = inner.height.saturating_sub(1) as usize;
    let start = app
        .browser_selected
        .saturating_sub(visible / 2)
        .min(matches.len().saturating_sub(visible.max(1)));
    for (index, (_, label)) in matches.iter().enumerate().skip(start).take(visible) {
        let selected = index == app.browser_selected;
        let style = if selected {
            Style::new().fg(theme.accent).bold()
        } else {
            Style::new().fg(theme.text)
        };
        lines.push(Line::from(Span::styled(
            format!(" {} {}", if selected { "▶" } else { " " }, label),
            style,
        )));
    }
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            " no sensors match the filter",
            Style::new().fg(theme.muted),
        )));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Debug overlay: poll stats, history sizes, last error (T5.1).
fn render_debug(frame: &mut Frame, area: Rect, app: &AppState, cfg: &Config, theme: &Theme) {
    let width = area.width.saturating_sub(12).clamp(40, 72);
    let rows = app.last_fetch.iter().count() + 15;
    let height = (rows as u16 + 2).min(area.height.saturating_sub(4));
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Debug — d closes ")
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let key_style = Style::new().fg(theme.muted);
    let value_style = Style::new().fg(theme.text);
    let mut lines: Vec<Line> = Vec::new();
    let mut push = |key: &str, value: String| {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", crate::ui::widgets::pad_display(key, 14)),
                key_style,
            ),
            Span::styled(value, value_style),
        ]));
    };

    push("version", env!("CARGO_PKG_VERSION").to_string());
    push("link", format!("{:?}", app.link));
    push(
        "flags",
        format!(
            "paused={} charts={} ascii={} zoom={} help={}",
            app.paused,
            app.effective_charts(cfg),
            app.effective_ascii(cfg),
            app.zoomed,
            app.help_visible
        ),
    );
    push("url", cfg.server.url.clone());
    push(
        "poll interval",
        format!("{} ms", cfg.server.poll_interval_ms),
    );
    push("window", app.effective_window(cfg).1);
    push(
        "uptime",
        format!("{} s", app.started_at.elapsed().as_secs()),
    );
    if let Some(fetched) = &app.last_fetch {
        let snapshot = &fetched.snapshot;
        push("last poll", format!("{} ms", fetched.latency.as_millis()));
        push("payload", human_bytes(fetched.bytes));
        push(
            "lhm version",
            snapshot
                .lhm_version
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        );
        push(
            "hardware",
            format!(
                "{} items · {} sensors · {} skipped",
                snapshot.hardware_count(),
                snapshot.sensor_count(),
                snapshot.skipped_nodes
            ),
        );
    }
    push(
        "history",
        match app.history.as_ref() {
            Some(history) => format!(
                "{} series · cap {} pts",
                history.series_count(),
                history.capacity()
            ),
            None => "not collected yet".to_string(),
        },
    );
    push("chart axes", format!("{} tracked", app.charts.len()));
    let guard_engaged: usize = app
        .fan_guard_state
        .values()
        .filter(|state| state.engaged)
        .count();
    let guard_controls: usize = app
        .fan_guard_state
        .values()
        .map(|state| state.control_ids.len())
        .sum();
    push(
        "fan guard",
        match app.effective_fan_guard(cfg) {
            true => format!("on · {guard_controls} controls · {guard_engaged} at floor"),
            false => "off".to_string(),
        },
    );
    push("failed polls", app.failed_attempts.to_string());
    if let Some(error) = &app.last_error {
        push("last error", error.clone());
    }
    if let Some((message, _)) = &app.toast {
        push("toast", message.clone());
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// `?`/`h` overlay: remapped keys with their action descriptions (T4.4).
fn render_help(frame: &mut Frame, area: Rect, cfg: &Config, theme: &Theme) {
    let width = area.width.saturating_sub(8).clamp(30, 66);
    let height = (crate::config::KNOWN_ACTIONS.len() as u16 + 5)
        .min(area.height.saturating_sub(2))
        .max(6);
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Help — any key closes ")
        .border_style(Style::new().fg(theme.accent));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line> = Vec::new();
    for (action, description) in crate::config::KNOWN_ACTIONS {
        let keys = cfg
            .keys
            .get(*action)
            .map(|specs| specs.join(", "))
            .unwrap_or_else(|| "-".to_string());
        lines.push(Line::from(vec![
            Span::styled(format!("  {keys:<18} "), Style::new().fg(theme.muted)),
            Span::styled((*description).to_string(), Style::new().fg(theme.text)),
        ]));
    }
    // Scroll/move keys are fixed shortcuts, not remappable actions (SPEC §6.4).
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {:<18} ", "↓ ↑, j k"),
            Style::new().fg(theme.muted),
        ),
        Span::styled(
            "Scroll focused table / move selection".to_string(),
            Style::new().fg(theme.text),
        ),
    ]));
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
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

fn dot(app: &AppState, theme: &Theme) -> Span<'static> {
    match app.link {
        LinkStatus::Connecting => Span::styled("○ ", Style::new().fg(theme.muted)),
        LinkStatus::Live => Span::styled("● ", Style::new().fg(theme.ok)),
        LinkStatus::Stale => Span::styled("● ", Style::new().fg(theme.warn)),
        LinkStatus::Reconnecting => Span::styled("● ", Style::new().fg(theme.crit)),
    }
}

fn link_label(app: &AppState, theme: &Theme) -> Span<'static> {
    match app.link {
        LinkStatus::Connecting => Span::styled("connecting", Style::new().fg(theme.muted)),
        LinkStatus::Live => Span::styled("live", Style::new().fg(theme.ok)),
        LinkStatus::Stale => Span::styled(
            format!("stale {}s", app.seconds_since_live()),
            Style::new().fg(theme.warn),
        ),
        LinkStatus::Reconnecting => Span::styled(
            format!("reconnecting… (retry in {}s)", app.retry_seconds()),
            Style::new().fg(theme.crit),
        ),
    }
}

fn header_line(app: &AppState, cfg: &Config, size: Rect, theme: &Theme) -> Paragraph<'static> {
    let (dot_span, label) = if app.paused {
        (
            Span::styled("○ ", Style::new().fg(theme.muted)),
            Span::styled("paused", Style::new().fg(theme.muted)),
        )
    } else {
        (dot(app, theme), link_label(app, theme))
    };
    let mut spans = vec![
        Span::styled(" lhm-tui ", Style::new().bold().fg(theme.accent)),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::new().fg(theme.muted),
        ),
        dot_span,
        label,
        Span::styled(
            format!("   {}×{}", size.width, size.height),
            Style::new().fg(theme.muted),
        ),
        Span::styled("   ", Style::new()),
        Span::styled(cfg.server.url.clone(), Style::new().fg(theme.muted)),
    ];
    // The window label used to live in the key bar; it moved to the header
    // (only when it still fits — narrow terminals keep the URL instead).
    if app.effective_charts(cfg) {
        use unicode_width::UnicodeWidthStr;
        let used: usize = spans.iter().map(|span| span.content.as_ref().width()).sum();
        let label = format!("   window {}", app.effective_window(cfg).1);
        if used + label.width() <= usize::from(size.width) {
            spans.push(Span::styled(label, Style::new().fg(theme.muted)));
        }
    }
    Paragraph::new(Line::from(spans))
}

/// Bottom status line: mirrors the newest status message (the transient toast
/// on line 2) and keeps it visible in a muted style after the toast fades.
/// Pressing `?`/`h` opens the full key map instead of a permanent key bar.
fn status_line(app: &AppState, theme: &Theme) -> Paragraph<'static> {
    let Some(message) = &app.status_line else {
        return Paragraph::new(Span::styled(String::new(), Style::new()));
    };
    let style = if app.toast.is_some() {
        Style::new().fg(theme.warn)
    } else {
        Style::new().fg(theme.muted)
    };
    Paragraph::new(Span::styled(format!(" {message}"), style))
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
                    Span::styled("● ", Style::new().fg(theme.ok)),
                    Span::styled("Live", Style::new().bold().fg(theme.ok)),
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
                Span::styled("● ", Style::new().fg(theme.warn)),
                Span::styled(
                    format!(
                        "No data for {}s — showing last values",
                        app.seconds_since_live()
                    ),
                    Style::new().fg(theme.warn),
                ),
            ]));
        }
        LinkStatus::Reconnecting => {
            lines.push(Line::from(vec![
                Span::styled("● ", Style::new().fg(theme.crit)),
                Span::styled(
                    "Cannot reach LibreHardwareMonitor",
                    Style::new().fg(theme.crit),
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
                Style::new().fg(theme.crit),
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
    let zoomed = app.zoomed;
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
    let focused_index = *focus % panels.len();

    // Zoomed: the focused panel owns the whole body (T5.2).
    if zoomed {
        let spec = &panels[focused_index];
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
            focused: true,
            sort,
            scroll,
            window,
            window_label: window_label.clone(),
            now,
        };
        presets::draw_panel(frame, area, spec, &mut ctx);
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
