//! T5.6: consolidated full-frame snapshots across widths and UI states.

mod support;

use std::sync::Arc;
use std::time::Duration;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use lhm_tui::app::{AppState, PollEvent};
use lhm_tui::config::Config;
use lhm_tui::lhm::client::Fetched;
use lhm_tui::lhm::parse::parse_payload;

fn frame_text(app: &mut AppState, cfg: &Config, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| lhm_tui::ui::draw(frame, app, cfg))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let mut line = String::new();
        for x in 0..buffer.area.width {
            line.push_str(buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn live_app(cfg: &Config) -> AppState {
    let snapshot = parse_payload(&support::fixture("lhm-live-5900xt.json")).expect("parses");
    let mut app = AppState::new();
    app.apply_poll_event(
        PollEvent::Snapshot(Arc::new(Fetched {
            snapshot,
            latency: Duration::from_millis(6),
            bytes: 74_000,
        })),
        cfg,
    );
    app
}

#[test]
fn frame_live_160x50_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 160, 50));
}

#[test]
fn frame_live_120x40_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 120, 40));
}

#[test]
fn frame_live_80x30_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 80, 30));
}

#[test]
fn frame_live_60x24_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 60, 24));
}

#[test]
fn frame_stale_after_failures_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.apply_poll_event(
        PollEvent::Failure {
            message:
                "cannot reach LibreHardwareMonitor at http://127.0.0.1:8085/data.json: timed out"
                    .to_string(),
            retry_in: Duration::from_secs(4),
            attempt: 2,
        },
        &cfg,
    );
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 120, 40));
}

#[test]
fn frame_disconnected_snapshot() {
    let cfg = Config::default();
    let mut app = AppState::new();
    app.apply_poll_event(
        PollEvent::Failure {
            message: "cannot reach LibreHardwareMonitor at http://127.0.0.1:8085/data.json: connection refused"
                .to_string(),
            retry_in: Duration::from_secs(1),
            attempt: 0,
        },
        &cfg,
    );
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 100, 30));
}

#[test]
fn frame_paused_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.paused = true;
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 120, 40));
}

#[test]
fn frame_zoomed_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.zoomed = true;
    app.focus = 1; // first GPU
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 100, 32));
}

#[test]
fn frame_debug_overlay_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.debug_visible = true;
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 100, 34));
}

#[test]
fn frame_browser_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.browser_open = true;
    app.browser_query = "gpu core".to_string();
    app.browser_selected = 2;
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 110, 32));
}

#[test]
fn frame_adhoc_sensor_snapshot() {
    let cfg = Config::default();
    let mut app = live_app(&cfg);
    app.history = Some({
        let mut history = lhm_tui::history::History::new(300, 1000, 3600);
        for _ in 0..30 {
            if let Some(fetched) = &app.last_fetch {
                history.append_snapshot(&fetched.snapshot);
            }
        }
        history
    });
    app.adhoc_sensor = Some("/amdcpu/0/temperature/2".to_string());
    insta::assert_snapshot!(frame_text(&mut app, &cfg, 100, 28));
}
