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
    // Live layout: cpu:0, gpu:0, gpu:1, memory, thermal → 5 panels.
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
    for _ in 0..4 {
        handle_event(&mut app, key(KeyCode::Tab), &cfg);
    }
    assert_eq!(app.focus, 0);
}

#[test]
fn scroll_applies_to_focused_scrollable_panel_only() {
    let (mut app, cfg) = live_app();

    // Focus the thermal table (index 4) and scroll down.
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
