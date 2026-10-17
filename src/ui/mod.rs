//! Root layout, status rendering and placeholder screen
//! (SPEC §6; preset panels land in M2).

use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, LinkStatus};
use crate::config::Config;
use crate::util::human_bytes;

pub fn draw(frame: &mut Frame, app: &AppState, cfg: &Config) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    if cfg.ui.show_header {
        frame.render_widget(header_line(app, cfg), header);
    }
    frame.render_widget(status_body(app, cfg), body);
    if cfg.ui.show_footer {
        frame.render_widget(footer_line(cfg), footer);
    }
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

fn header_line(app: &AppState, cfg: &Config) -> Paragraph<'static> {
    let line = Line::from(vec![
        Span::styled(" lhm-tui ", Style::new().bold().fg(Color::Cyan)),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::new().fg(Color::DarkGray),
        ),
        dot(app),
        link_label(app),
        Span::styled("   ", Style::new()),
        Span::styled(cfg.server.url.clone(), Style::new().fg(Color::DarkGray)),
    ]);
    Paragraph::new(line)
}

fn status_body(app: &AppState, cfg: &Config) -> Paragraph<'static> {
    let url = cfg.server.url.trim_end_matches('/').to_string();
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" LibreHardwareMonitor ");

    let mut lines: Vec<Line> = vec![Line::from("")];
    match app.link {
        LinkStatus::Connecting => {
            lines.push(Line::from("Connecting to LibreHardwareMonitor…".bold()));
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
                    Style::new().fg(Color::DarkGray),
                )));
                lines.push(Line::from(Span::styled(
                    format!(
                        "last poll {} in {} ms",
                        human_bytes(fetched.bytes),
                        fetched.latency.as_millis()
                    ),
                    Style::new().fg(Color::DarkGray),
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
                Style::new().fg(Color::DarkGray),
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
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Panels and charts land in milestones M2–M3 (PLAN.md)",
        Style::new().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("session up {}s", app.started_at.elapsed().as_secs()),
        Style::new().fg(Color::DarkGray),
    )));

    Paragraph::new(Text::from(lines))
        .block(block)
        .alignment(Alignment::Center)
}

fn footer_line(cfg: &Config) -> Paragraph<'static> {
    let quit = cfg
        .keys
        .get("quit")
        .and_then(|keys| keys.first())
        .cloned()
        .unwrap_or_else(|| "q".to_string());
    let line = Line::from(vec![
        Span::styled(format!(" {quit} "), Style::new().bold().fg(Color::Cyan)),
        Span::styled("quit", Style::new().fg(Color::DarkGray)),
        Span::styled("   ·   M1 client online", Style::new().fg(Color::DarkGray)),
    ]);
    Paragraph::new(line)
}
