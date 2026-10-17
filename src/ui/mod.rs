//! Root layout and placeholder screen (SPEC §6; preset panels land in M2).

use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::config::Config;

pub fn draw(frame: &mut Frame, app: &AppState, cfg: &Config) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    if cfg.ui.show_header {
        frame.render_widget(header_line(cfg), header);
    }
    frame.render_widget(placeholder(app, cfg), body);
    if cfg.ui.show_footer {
        frame.render_widget(footer_line(cfg), footer);
    }
}

fn header_line(cfg: &Config) -> Paragraph<'static> {
    let line = Line::from(vec![
        Span::styled(" lhm-tui ", Style::new().bold().fg(Color::Cyan)),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::new().fg(Color::DarkGray),
        ),
        Span::styled("→  ", Style::new().fg(Color::DarkGray)),
        Span::styled(cfg.server.url.clone(), Style::new().fg(Color::Cyan)),
    ]);
    Paragraph::new(line)
}

fn placeholder(app: &AppState, cfg: &Config) -> Paragraph<'static> {
    let url = cfg.server.url.trim_end_matches('/').to_string();
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" LibreHardwareMonitor ");
    let text = Text::from(vec![
        Line::from(""),
        Line::from("Connecting to LibreHardwareMonitor…".bold()),
        Line::from(""),
        Line::from(Span::styled(
            format!("GET {url}/data.json"),
            Style::new().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "Polling and panels land in milestones M1–M2, charts in M3 (PLAN.md)",
            Style::new().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("session up {}s", app.started_at.elapsed().as_secs()),
            Style::new().fg(Color::DarkGray),
        )),
    ]);
    Paragraph::new(text)
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
        Span::styled("   ·   P0 shell", Style::new().fg(Color::DarkGray)),
    ]);
    Paragraph::new(line)
}
