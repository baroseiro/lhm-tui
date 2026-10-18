//! Interaction tests (PLAN T2.1/T2.4): pause, focus cycling, sort cycling,
//! and table scrolling over real bindings from fixtures.

mod support;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use lhm_tui::app::{handle_event, AppState, PollEvent};
use lhm_tui::config::{Config, TableSort};
use lhm_tui::lhm::client::Fetched;
use lhm_tui::lhm::parse::parse_payload;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn live_app() -> (AppState, Config) {
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-live-5900xt.json")).expect("parses");
    let mut app = AppState::new();
    app.apply_poll_event(
        PollEvent::Snapshot(Arc::new(Fetched {
            snapshot,
            latency: Duration::from_millis(5),
            bytes: 74_000,
        })),
        &cfg,
    );
    (app, cfg)
}

#[test]
fn pause_key_toggles_pause_and_shared_flag() {
    let (mut app, cfg) = live_app();
    assert!(!app.paused);
    assert!(!app.pause_flag.load(Ordering::Relaxed));

    handle_event(&mut app, key(KeyCode::Char('p')), &cfg);
    assert!(app.paused, "p pauses polling");
    assert!(
        app.pause_flag.load(Ordering::Relaxed),
        "poller sees the flag"
    );

    handle_event(&mut app, key(KeyCode::Char(' ')), &cfg);
    assert!(!app.paused, "space toggles back (default pause keys)");
    assert!(!app.pause_flag.load(Ordering::Relaxed));
}

#[test]
fn tab_cycles_focus_over_panel_list() {
    let (mut app, cfg) = live_app();
    // Default layout with thermal hidden: cpu:0, gpu:0, gpu:1, memory → 4 panels.
    assert_eq!(app.focus, 0);

    handle_event(&mut app, key(KeyCode::Tab), &cfg);
    assert_eq!(app.focus, 1);
    handle_event(&mut app, key(KeyCode::Tab), &cfg);
    assert_eq!(app.focus, 2);

    // Shift+Tab arrives as BackTab on real terminals.
    handle_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        &cfg,
    );
    assert_eq!(app.focus, 1);

    // Wraps at the end.
    for _ in 0..3 {
        handle_event(&mut app, key(KeyCode::Tab), &cfg);
    }
    assert_eq!(app.focus, 0);
}

#[test]
fn scroll_applies_to_focused_scrollable_panel_only() {
    let (mut app, cfg) = live_app();
    // Reveal the temperatures table with the `t` override, then focus it
    // (cpu:0, gpu:0, gpu:1, memory, thermal → index 4).
    app.thermal_override = Some(true);
    app.focus = 4;
    handle_event(&mut app, key(KeyCode::Char('j')), &cfg);
    handle_event(&mut app, key(KeyCode::Char('j')), &cfg);
    assert_eq!(app.scrolls.get("thermal").copied(), Some(2));

    handle_event(&mut app, key(KeyCode::Char('k')), &cfg);
    assert_eq!(app.scrolls.get("thermal").copied(), Some(1));

    // Saturates at zero.
    handle_event(&mut app, key(KeyCode::Up), &cfg);
    handle_event(&mut app, key(KeyCode::Up), &cfg);
    assert_eq!(app.scrolls.get("thermal").copied(), Some(0));

    // Focused CPU panel is not scrollable → no scroll entry appears.
    app.focus = 0;
    handle_event(&mut app, key(KeyCode::Char('j')), &cfg);
    assert!(!app.scrolls.contains_key("cpu:0"));
}

#[test]
fn t_and_c_keys_toggle_thermal_and_charts() {
    let (mut app, cfg) = live_app();
    assert!(!app.effective_thermal(&cfg), "hidden by default");
    handle_event(&mut app, key(KeyCode::Char('t')), &cfg);
    assert!(app.effective_thermal(&cfg));
    handle_event(&mut app, key(KeyCode::Char('t')), &cfg);
    assert!(!app.effective_thermal(&cfg));

    assert!(app.effective_charts(&cfg));
    handle_event(&mut app, key(KeyCode::Char('c')), &cfg);
    assert!(!app.effective_charts(&cfg));
    handle_event(&mut app, key(KeyCode::Char('c')), &cfg);
    assert!(app.effective_charts(&cfg));
}

#[test]
fn enter_toggles_zoom() {
    let (mut app, cfg) = live_app();
    handle_event(&mut app, key(KeyCode::Enter), &cfg);
    assert!(app.zoomed);
    handle_event(&mut app, key(KeyCode::Enter), &cfg);
    assert!(!app.zoomed);
}

#[test]
fn d_toggles_debug_overlay() {
    let (mut app, cfg) = live_app();
    handle_event(&mut app, key(KeyCode::Char('d')), &cfg);
    assert!(app.debug_visible);
    handle_event(&mut app, key(KeyCode::Char('d')), &cfg);
    assert!(!app.debug_visible);
}

#[test]
fn browser_filters_selects_and_graphs() {
    let (mut app, cfg) = live_app();
    handle_event(&mut app, key(KeyCode::Char('b')), &cfg);
    assert!(app.browser_open);

    for character in "gpu core".chars() {
        handle_event(&mut app, key(KeyCode::Char(character)), &cfg);
    }
    assert_eq!(app.browser_query, "gpu core");
    let matches = app.browser_matches();
    assert!(!matches.is_empty(), "query should match GPU Core sensors");
    assert!(matches
        .iter()
        .all(|(_, label)| label.to_ascii_lowercase().contains("gpu core")));

    handle_event(&mut app, key(KeyCode::Char('j')), &cfg);
    assert_eq!(app.browser_selected, 1);

    handle_event(&mut app, key(KeyCode::Enter), &cfg);
    assert!(!app.browser_open, "enter closes the browser");
    let picked = app.adhoc_sensor.clone().expect("sensor picked");
    assert!(picked.contains("/gpu-"), "picked {picked}");

    // q closes the ad-hoc view instead of quitting.
    handle_event(&mut app, key(KeyCode::Char('q')), &cfg);
    assert!(app.adhoc_sensor.is_none());
    assert!(!app.should_quit);

    // Backspace edits the filter.
    handle_event(&mut app, key(KeyCode::Char('b')), &cfg);
    handle_event(&mut app, key(KeyCode::Char('x')), &cfg);
    handle_event(&mut app, key(KeyCode::Backspace), &cfg);
    assert!(app.browser_query.is_empty());
    handle_event(&mut app, key(KeyCode::Esc), &cfg);
    assert!(!app.browser_open);
}

#[test]
fn reload_applies_valid_config_and_keeps_old_on_error() {
    use clap::Parser;

    let dir = std::env::temp_dir().join(format!("lhm-tui-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("config.json");

    let (mut app, mut cfg) = live_app();

    // Valid reload: layout changes and a toast confirms.
    std::fs::write(&path, r##"{"ui":{"layout":["cpu","memory"]}}"##).expect("write config");
    let cli = lhm_tui::cli::Cli::parse_from(["lhm-tui", "--config", path.to_str().expect("utf8")]);
    app.cli = Some(cli.clone());
    handle_event(&mut app, key(KeyCode::Char('r')), &cfg);
    assert!(app.pending_reload, "run_loop performs the reload");
    lhm_tui::app::reload_config(&mut app, &mut cfg, Some(cli.clone()));
    assert_eq!(cfg.ui.layout, vec!["cpu", "memory"]);
    assert!(app.toast.as_ref().expect("toast").0.contains("reloaded"));

    // Broken reload: previous config stays, toast reports the error.
    std::fs::write(&path, "{ not json").expect("write bad config");
    lhm_tui::app::reload_config(&mut app, &mut cfg, Some(cli));
    assert_eq!(cfg.ui.layout, vec!["cpu", "memory"], "old config kept");
    assert!(app
        .toast
        .as_ref()
        .expect("toast")
        .0
        .contains("reload failed"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reset_minmax_is_a_noop_without_notice_channel() {
    let (mut app, cfg) = live_app();
    // Unit tests have no tokio runtime / notice channel: pressing `m` must
    // not panic and must not fabricate a toast.
    let before = app.toast.clone();
    handle_event(&mut app, key(KeyCode::Char('m')), &cfg);
    assert_eq!(app.toast, before);
}

#[test]
fn sort_key_cycles_modes() {
    let (mut app, cfg) = live_app();
    assert_eq!(
        app.effective_sort(&cfg),
        TableSort::ValueDesc,
        "config default"
    );

    handle_event(&mut app, key(KeyCode::Char('s')), &cfg);
    assert_eq!(app.effective_sort(&cfg), TableSort::ValueDesc);
    handle_event(&mut app, key(KeyCode::Char('s')), &cfg);
    assert_eq!(app.effective_sort(&cfg), TableSort::ValueAsc);
    handle_event(&mut app, key(KeyCode::Char('s')), &cfg);
    assert_eq!(app.effective_sort(&cfg), TableSort::Name);
    handle_event(&mut app, key(KeyCode::Char('s')), &cfg);
    assert_eq!(app.effective_sort(&cfg), TableSort::Unspecified);
    handle_event(&mut app, key(KeyCode::Char('s')), &cfg);
    assert_eq!(
        app.effective_sort(&cfg),
        TableSort::ValueDesc,
        "wraps to config default"
    );
}

#[test]
fn g_and_a_keys_cycle_window_and_ascii() {
    let (mut app, cfg) = live_app();
    assert_eq!(app.effective_window(&cfg).1, "5m");
    handle_event(&mut app, key(KeyCode::Char('g')), &cfg);
    assert_eq!(app.effective_window(&cfg).1, "15m");
    handle_event(&mut app, key(KeyCode::Char('G')), &cfg);
    assert_eq!(app.effective_window(&cfg).1, "5m");

    assert!(!app.effective_ascii(&cfg));
    handle_event(&mut app, key(KeyCode::Char('a')), &cfg);
    assert!(app.effective_ascii(&cfg));
}
