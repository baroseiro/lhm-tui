//! Application state, poller integration and the async event loop (SPEC §4.2).
//!
//! The UI loop never blocks on input or network: crossterm events arrive over
//! an `mpsc` channel from a dedicated input thread, poll results arrive over a
//! `watch` channel from the poller task (latest-value-wins), and redraws happen
//! on a frame tick only when the screen is dirty (redraw-on-change policy).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::bindings::{self, Bindings};
use crate::charts::ChartsState;
use crate::config::{Config, RetryConfig, ServerConfig, TableSort};
use crate::history::History;
use crate::lhm::client::{Fetched, LhmClient};
use crate::lhm::model::Snapshot;
use crate::ui;

/// Latest poller state delivered over a `watch` channel (SPEC §4.2).
#[derive(Debug, Clone)]
pub enum PollEvent {
    /// Before the first poll attempt.
    Starting,
    /// A successful poll.
    Snapshot(Arc<Fetched>),
    /// A failed poll; `retry_in` is the computed backoff, `attempt` counts
    /// consecutive failures starting at 0.
    Failure {
        message: String,
        retry_in: Duration,
        attempt: u32,
    },
}

/// Connection status shown in the header (SPEC §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Connecting,
    Live,
    Stale,
    Reconnecting,
}

pub struct AppState {
    pub should_quit: bool,
    /// Set when the screen needs a redraw (SPEC §4.2 redraw policy).
    pub dirty: bool,
    pub started_at: Instant,
    pub link: LinkStatus,
    pub last_fetch: Option<Arc<Fetched>>,
    pub last_live_at: Option<Instant>,
    pub last_error: Option<String>,
    pub last_retry_in: Option<Duration>,
    pub failed_attempts: u32,
    /// Polling suspended via the `pause` key; shared with the poller task.
    pub paused: bool,
    pub pause_flag: Arc<AtomicBool>,
    /// Panel bindings, recomputed only when the hardware set changes (T2.2).
    pub bindings: Option<Bindings>,
    pub bindings_fingerprint: u64,
    /// Focused panel index into the current panel list (wraps at draw time).
    pub focus: usize,
    /// Per-panel scroll offsets, keyed by panel id.
    pub scrolls: BTreeMap<String, u16>,
    /// `s` key sort override; `None` → `ui.table_sort` from config.
    pub sort_override: Option<TableSort>,
    /// Rolling per-sensor history for charts (`None` until the first
    /// snapshot; only populated when `charts.enabled`).
    pub history: Option<History>,
    /// `g`/`G` override: index into `charts.window_cycle`;
    /// `None` → `charts.window` from config.
    pub window_override: Option<usize>,
    /// `a` key override for `ui.ascii`.
    pub ascii_override: Option<bool>,
    /// `t` key override for the temperatures table visibility.
    pub thermal_override: Option<bool>,
    /// `c` key override for `charts.enabled`.
    pub charts_override: Option<bool>,
    /// `?` help overlay visibility (any key press closes it).
    pub help_visible: bool,
    /// Startup warnings shown as a toast (unknown config keys, …).
    pub toast: Option<(String, Instant)>,
    /// Chart autoscale state (hysteresis per graph).
    pub charts: ChartsState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            dirty: true,
            started_at: Instant::now(),
            link: LinkStatus::Connecting,
            last_fetch: None,
            last_live_at: None,
            last_error: None,
            last_retry_in: None,
            failed_attempts: 0,
            paused: false,
            pause_flag: Arc::new(AtomicBool::new(false)),
            bindings: None,
            bindings_fingerprint: 0,
            focus: 0,
            scrolls: BTreeMap::new(),
            sort_override: None,
            history: None,
            window_override: None,
            ascii_override: None,
            thermal_override: None,
            charts_override: None,
            help_visible: false,
            toast: None,
            charts: ChartsState::new(),
        }
    }

    /// Latest snapshot, when one has been received.
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.last_fetch.as_deref().map(|fetched| &fetched.snapshot)
    }

    /// Effective table sort: `s` override when set, else the configured default.
    pub fn effective_sort(&self, cfg: &Config) -> TableSort {
        self.sort_override.unwrap_or(cfg.ui.table_sort)
    }

    /// Effective chart window (`g`/`G` override, else `charts.window`).
    pub fn effective_window(&self, cfg: &Config) -> (Duration, String) {
        let label = match self.window_override {
            Some(index) if !cfg.charts.window_cycle.is_empty() => {
                cfg.charts.window_cycle[index % cfg.charts.window_cycle.len()].clone()
            }
            _ => cfg.charts.window.clone(),
        };
        let duration = humantime::parse_duration(&label).unwrap_or(Duration::from_secs(300));
        (duration, label)
    }

    /// Cycle the chart window through `charts.window_cycle` (SPEC §6.4).
    pub fn window_next(&mut self, cfg: &Config, delta: i32) {
        let cycle = &cfg.charts.window_cycle;
        if cycle.is_empty() {
            return;
        }
        let (_, current) = self.effective_window(cfg);
        let position = cycle
            .iter()
            .position(|label| *label == current)
            .unwrap_or(0) as i32;
        let next = (position + delta).rem_euclid(cycle.len() as i32) as usize;
        self.window_override = Some(next);
        self.dirty = true;
    }

    /// Effective ASCII rendering: `a` override, else `ui.ascii` from config.
    pub fn effective_ascii(&self, cfg: &Config) -> bool {
        self.ascii_override.unwrap_or(cfg.ui.ascii)
    }

    /// Effective temperatures-table visibility: `t` override, else whether
    /// `thermal` is listed in `ui.layout` (hidden by default).
    pub fn effective_thermal(&self, cfg: &Config) -> bool {
        self.thermal_override
            .unwrap_or_else(|| cfg.ui.layout.iter().any(|key| key == "thermal"))
    }

    /// Effective charts mode: `c` override, else `charts.enabled` from config.
    pub fn effective_charts(&self, cfg: &Config) -> bool {
        self.charts_override.unwrap_or(cfg.charts.enabled)
    }

    /// Panel-visibility overrides for `presets::panel_list`.
    pub fn panel_overrides(&self, cfg: &Config) -> crate::ui::presets::PanelOverrides {
        crate::ui::presets::PanelOverrides {
            thermal: Some(self.effective_thermal(cfg)),
        }
    }

    /// Fold a poller event into the UI state (SPEC §10.1 failure matrix).
    pub fn apply_poll_event(&mut self, event: PollEvent, cfg: &Config) {
        match event {
            PollEvent::Starting => self.link = LinkStatus::Connecting,
            PollEvent::Snapshot(fetched) => {
                let fingerprint = bindings::fingerprint(&fetched.snapshot);
                if self.bindings.is_none() || fingerprint != self.bindings_fingerprint {
                    self.bindings = Some(bindings::resolve(&fetched.snapshot, &cfg.panels));
                    self.bindings_fingerprint = fingerprint;
                }
                // History is always collected: it is cheap (~1 MB for 500
                // series) and lets the `c` key switch to charts instantly.
                let history = self.history.get_or_insert_with(|| {
                    History::new(
                        cfg.history.window_s,
                        cfg.server.poll_interval_ms,
                        cfg.history.max_points_per_series,
                    )
                });
                history.append_snapshot(&fetched.snapshot);
                self.last_live_at = Some(Instant::now());
                self.last_fetch = Some(fetched);
                self.failed_attempts = 0;
                self.last_retry_in = None;
                self.link = LinkStatus::Live;
            }
            PollEvent::Failure {
                message,
                retry_in,
                attempt,
            } => {
                self.last_error = Some(message);
                self.failed_attempts = attempt.saturating_add(1);
                self.last_retry_in = Some(retry_in);
                self.link = if self.last_fetch.is_some() {
                    LinkStatus::Stale
                } else {
                    LinkStatus::Reconnecting
                };
            }
        }
        self.dirty = true;
    }

    pub fn seconds_since_live(&self) -> u64 {
        self.last_live_at
            .map(|at| at.elapsed().as_secs())
            .unwrap_or(0)
    }

    pub fn retry_seconds(&self) -> u64 {
        self.last_retry_in
            .map(|delay| delay.as_secs().max(1))
            .unwrap_or(0)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the TUI until the user quits. Owns the terminal lifecycle, including
/// restore on every exit path (SPEC §4.2 shutdown). Startup config warnings
/// are surfaced as a toast (T4.6).
pub async fn run(cfg: &Config, warnings: &[String]) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, cfg, warnings).await;
    ratatui::restore();
    result
}

async fn run_loop(terminal: &mut DefaultTerminal, cfg: &Config, warnings: &[String]) -> Result<()> {
    let mut app = AppState::new();
    if !warnings.is_empty() {
        app.toast = Some((warnings.join("  ·  "), Instant::now()));
    }
    let frame_ms = u64::from(1000 / cfg.ui.fps.max(1)).max(1);
    let mut tick = tokio::time::interval(Duration::from_millis(frame_ms));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    spawn_input_thread(tx);

    let mut poll_events = spawn_poller(&cfg.server, Arc::clone(&app.pause_flag));
    let mut poller_alive = true;
    let mut last_status_redraw = Instant::now();

    while !app.should_quit {
        tokio::select! {
            _ = tick.tick() => {
                // Toast expiry.
                if let Some((_, started)) = &app.toast {
                    if started.elapsed() >= Duration::from_secs(12) {
                        app.toast = None;
                        app.dirty = true;
                    }
                }
                // Age ticker: refresh stale/paused/toast status once a second.
                let unstable = app.link != LinkStatus::Live || app.paused || app.toast.is_some();
                if !app.dirty && unstable && last_status_redraw.elapsed() >= Duration::from_secs(1) {
                    app.dirty = true;
                    last_status_redraw = Instant::now();
                }
                if app.dirty {
                    terminal.draw(|frame| ui::draw(frame, &mut app, cfg))?;
                    app.dirty = false;
                }
            }
            event = rx.recv() => match event {
                Some(event) => handle_event(&mut app, event, cfg),
                // The input thread ended (stdin closed): do not hang forever.
                None => app.should_quit = true,
            },
            changed = poll_events.changed(), if poller_alive => {
                if changed.is_ok() {
                    let event = poll_events.borrow_and_update().clone();
                    app.apply_poll_event(event, cfg);
                } else {
                    // The poller task ended unexpectedly; keep the UI usable.
                    poller_alive = false;
                    app.apply_poll_event(PollEvent::Failure {
                        message: "internal poller task stopped".to_string(),
                        retry_in: Duration::ZERO,
                        attempt: 0,
                    }, cfg);
                }
            }
        }
    }
    Ok(())
}

/// Spawn the background poller task (SPEC §4.2) and return its event stream.
/// `pause` freezes polling without dropping the last snapshot.
pub fn spawn_poller(
    server: &ServerConfig,
    pause: Arc<AtomicBool>,
) -> tokio::sync::watch::Receiver<PollEvent> {
    let (sender, receiver) = tokio::sync::watch::channel(PollEvent::Starting);
    let server = server.clone();
    tokio::spawn(async move {
        poll_loop(server, sender, pause).await;
    });
    receiver
}

async fn poll_loop(
    server: ServerConfig,
    sender: tokio::sync::watch::Sender<PollEvent>,
    pause: Arc<AtomicBool>,
) {
    let client = match LhmClient::new(&server) {
        Ok(client) => client,
        Err(err) => {
            let _ = sender.send(PollEvent::Failure {
                message: format!("{err:#}"),
                retry_in: Duration::ZERO,
                attempt: 0,
            });
            return;
        }
    };

    let mut ticker = tokio::time::interval(Duration::from_millis(server.poll_interval_ms.max(1)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut attempt: u32 = 0;

    loop {
        ticker.tick().await;
        if sender.is_closed() {
            return; // the UI is gone
        }
        if pause.load(Ordering::Relaxed) {
            continue; // paused: keep the last snapshot, skip polling
        }
        match client.fetch_data_json().await {
            Ok(fetched) => {
                attempt = 0;
                if sender.send(PollEvent::Snapshot(Arc::new(fetched))).is_err() {
                    return;
                }
            }
            Err(err) => {
                let delay = jittered(
                    backoff_delay(&server.retry, attempt),
                    server.retry.jitter,
                    rand01(),
                );
                let event = PollEvent::Failure {
                    message: err.to_string(),
                    retry_in: delay,
                    attempt,
                };
                if sender.send(event).is_err() {
                    return;
                }
                attempt = attempt.saturating_add(1);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Deterministic exponential backoff: `initial × factor^attempt` capped at
/// `max_ms` (SPEC §4.2).
pub fn backoff_delay(retry: &RetryConfig, attempt: u32) -> Duration {
    let base = retry.initial_ms as f64 * retry.factor.powi(attempt.min(16) as i32);
    let capped = base.min(retry.max_ms as f64).max(1.0);
    Duration::from_millis(capped as u64)
}

/// Apply ±`jitter` around `delay`; `rand01` is a sample in `[0, 1)`
/// (injectable for tests).
pub fn jittered(delay: Duration, jitter: f64, rand01: f64) -> Duration {
    if jitter <= 0.0 {
        return delay;
    }
    let jitter = jitter.clamp(0.0, 0.5);
    let factor = 1.0 - jitter + 2.0 * jitter * rand01.clamp(0.0, 1.0);
    Duration::from_millis((delay.as_secs_f64() * factor * 1000.0).round().max(1.0) as u64)
}

/// No RNG dependency: entropy from the clock's sub-second nanos.
pub fn rand01() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos % 1_000_000) / 1_000_000.0
}

/// crossterm event reads are blocking, so they live on their own thread.
fn spawn_input_thread(tx: tokio::sync::mpsc::UnboundedSender<Event>) {
    std::thread::spawn(move || {
        while let Ok(event) = ratatui::crossterm::event::read() {
            if tx.send(event).is_err() {
                break; // the UI loop is gone
            }
        }
    });
}

/// Handle a UI event: help overlay, quit, pause, focus cycling, sort cycling
/// and scrolling.
pub fn handle_event(app: &mut AppState, event: Event, cfg: &Config) {
    match event {
        Event::Resize(_, _) => app.dirty = true,
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            // The help overlay is closed by any key press.
            if app.help_visible {
                app.help_visible = false;
                app.dirty = true;
                return;
            }
            if action_matches(cfg, "help", &key) {
                app.help_visible = true;
                app.dirty = true;
            } else if action_matches(cfg, "quit", &key) {
                app.should_quit = true;
            } else if action_matches(cfg, "pause", &key) {
                app.paused = !app.paused;
                app.pause_flag.store(app.paused, Ordering::Relaxed);
                app.dirty = true;
            } else if action_matches(cfg, "focus_next", &key) {
                focus_delta(app, cfg, 1);
            } else if action_matches(cfg, "focus_prev", &key) {
                focus_delta(app, cfg, -1);
            } else if action_matches(cfg, "sort", &key) {
                app.sort_override = next_sort(app.sort_override);
                app.dirty = true;
            } else if action_matches(cfg, "window_next", &key) {
                app.window_next(cfg, 1);
            } else if action_matches(cfg, "window_prev", &key) {
                app.window_next(cfg, -1);
            } else if action_matches(cfg, "ascii", &key) {
                let current = app.effective_ascii(cfg);
                app.ascii_override = Some(!current);
                app.dirty = true;
            } else if action_matches(cfg, "toggle_thermal", &key) {
                let current = app.effective_thermal(cfg);
                app.thermal_override = Some(!current);
                app.dirty = true;
            } else if action_matches(cfg, "toggle_charts", &key) {
                let current = app.effective_charts(cfg);
                app.charts_override = Some(!current);
                app.dirty = true;
            } else if key_matches(&key, "down") || key_matches(&key, "j") {
                scroll_focused(app, cfg, 1);
            } else if key_matches(&key, "up") || key_matches(&key, "k") {
                scroll_focused(app, cfg, -1);
            } else {
                app.dirty = true;
            }
        }
        _ => {}
    }
}

fn next_sort(current: Option<TableSort>) -> Option<TableSort> {
    match current {
        None => Some(TableSort::ValueDesc),
        Some(TableSort::ValueDesc) => Some(TableSort::ValueAsc),
        Some(TableSort::ValueAsc) => Some(TableSort::Name),
        Some(TableSort::Name) => Some(TableSort::Unspecified),
        Some(TableSort::Unspecified) => None,
    }
}

fn panel_count(app: &AppState, cfg: &Config) -> usize {
    app.bindings
        .as_ref()
        .map(|bindings| {
            crate::ui::presets::panel_list(cfg, bindings, app.panel_overrides(cfg)).len()
        })
        .unwrap_or(0)
}

fn focus_delta(app: &mut AppState, cfg: &Config, delta: i32) {
    let count = panel_count(app, cfg);
    if count == 0 {
        return;
    }
    let current = (app.focus % count) as i32;
    app.focus = (current + delta).rem_euclid(count as i32) as usize;
    app.dirty = true;
}

fn scroll_focused(app: &mut AppState, cfg: &Config, delta: i32) {
    // Resolve the focused panel id first; the borrow ends before mutation.
    let target = app.bindings.as_ref().and_then(|bindings| {
        let panels = crate::ui::presets::panel_list(cfg, bindings, app.panel_overrides(cfg));
        if panels.is_empty() {
            return None;
        }
        let focused = &panels[app.focus % panels.len()];
        focused.scrollable.then(|| focused.id.clone())
    });
    let Some(id) = target else {
        return;
    };
    let scroll = app.scrolls.entry(id).or_insert(0);
    *scroll = if delta >= 0 {
        scroll.saturating_add(delta as u16)
    } else {
        scroll.saturating_sub((-delta) as u16)
    };
    app.dirty = true;
}

/// True when `key` matches any key spec configured for `action` (SPEC §6.4).
pub fn action_matches(cfg: &Config, action: &str, key: &KeyEvent) -> bool {
    cfg.keys
        .get(action)
        .is_some_and(|specs| specs.iter().any(|spec| key_matches(key, spec)))
}

/// Match a key against a spec like `q`, `ctrl+c`, `shift+tab`, `f5`, `space`.
pub fn key_matches(key: &KeyEvent, spec: &str) -> bool {
    let Some(expected) = parse_key_spec(spec) else {
        return false;
    };
    let (mut expected_code, mut expected_mods) = (expected.code, expected.modifiers);
    let (mut actual_code, mut actual_mods) = (key.code, key.modifiers);

    // Shift+Tab arrives as Tab+SHIFT (kitty protocol) or BackTab (legacy
    // terminals); normalize both spellings before comparing.
    if expected_code == KeyCode::Tab && expected_mods.contains(KeyModifiers::SHIFT) {
        expected_code = KeyCode::BackTab;
    }
    if actual_code == KeyCode::Tab && actual_mods.contains(KeyModifiers::SHIFT) {
        actual_code = KeyCode::BackTab;
    }
    if expected_code == KeyCode::BackTab {
        expected_mods.remove(KeyModifiers::SHIFT);
    }
    if actual_code == KeyCode::BackTab {
        actual_mods.remove(KeyModifiers::SHIFT);
    }
    if actual_code != expected_code {
        return false;
    }

    // Character keys encode their case in the code itself; terminals differ
    // on whether they also report SHIFT (e.g. `?` = shift+/), so ignore the
    // SHIFT bit for characters on both sides.
    if let KeyCode::Char(_) = actual_code {
        expected_mods.remove(KeyModifiers::SHIFT);
        actual_mods.remove(KeyModifiers::SHIFT);
    }
    actual_mods == expected_mods
}

/// Parse a key spec: optional `ctrl+`/`shift+`/`alt+` prefixes followed by a
/// single character, a named key (`esc`, `space`, `tab`, `enter`, arrows, …)
/// or `f1`..`f12`. Case is preserved for plain characters.
pub fn parse_key_spec(spec: &str) -> Option<KeyEvent> {
    let mut rest = spec.trim();
    if rest.is_empty() {
        return None;
    }

    let mut modifiers = KeyModifiers::NONE;
    loop {
        let lower = rest.to_ascii_lowercase();
        let (prefix_len, modifier) = if lower.starts_with("ctrl+") {
            (5, KeyModifiers::CONTROL)
        } else if lower.starts_with("shift+") {
            (6, KeyModifiers::SHIFT)
        } else if lower.starts_with("alt+") {
            (4, KeyModifiers::ALT)
        } else {
            break;
        };
        modifiers.insert(modifier);
        rest = &rest[prefix_len..];
    }
    if rest.is_empty() {
        return None;
    }

    let code = match rest.to_ascii_lowercase().as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "enter" | "return" => KeyCode::Enter,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        "delete" | "del" => KeyCode::Delete,
        "backspace" => KeyCode::Backspace,
        other => match other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
            Some(number) if (1..=12).contains(&number) => KeyCode::F(number),
            Some(_) => return None,
            None => {
                let mut chars = rest.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None; // multi-character names must be known keys
                }
                KeyCode::Char(c)
            }
        },
    };
    Some(KeyEvent::new(code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_keys() {
        assert_eq!(parse_key_spec("q").expect("q").code, KeyCode::Char('q'));
        assert_eq!(parse_key_spec("esc").expect("esc").code, KeyCode::Esc);
        assert_eq!(
            parse_key_spec("space").expect("space").code,
            KeyCode::Char(' ')
        );
        assert_eq!(parse_key_spec("enter").expect("enter").code, KeyCode::Enter);
        assert_eq!(parse_key_spec("f5").expect("f5").code, KeyCode::F(5));
        assert!(parse_key_spec("").is_none());
        assert!(parse_key_spec("notakey").is_none());
        assert!(parse_key_spec("ctrl+").is_none());
        assert!(parse_key_spec("f13").is_none());
    }

    #[test]
    fn parses_modifier_combos() {
        let ctrl_c = parse_key_spec("ctrl+c").expect("ctrl+c");
        assert_eq!(ctrl_c.code, KeyCode::Char('c'));
        assert_eq!(ctrl_c.modifiers, KeyModifiers::CONTROL);

        let shift_tab = parse_key_spec("shift+tab").expect("shift+tab");
        assert_eq!(shift_tab.code, KeyCode::Tab);
        assert_eq!(shift_tab.modifiers, KeyModifiers::SHIFT);
    }

    #[test]
    fn key_matches_respects_modifiers() {
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(key_matches(&q, "q"));
        assert!(!key_matches(&q, "ctrl+q"));

        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(key_matches(&ctrl_c, "ctrl+c"));
        assert!(!key_matches(&ctrl_c, "c"));
    }

    #[test]
    fn uppercase_letters_match_regardless_of_shift() {
        let plain = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        let shifted = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert!(key_matches(&plain, "G"));
        assert!(key_matches(&shifted, "G"));
        assert!(!key_matches(&plain, "g"));
    }

    #[test]
    fn quit_action_uses_configured_keys() {
        let cfg = Config::default();
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(action_matches(&cfg, "quit", &q));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(action_matches(&cfg, "quit", &ctrl_c));
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(action_matches(&cfg, "quit", &esc));
    }

    #[test]
    fn backoff_schedule_matches_spec() {
        let retry = RetryConfig::default(); // 500 ms × 2, cap 30 s
        let ms = |attempt: u32| backoff_delay(&retry, attempt).as_millis() as u64;
        assert_eq!(ms(0), 500);
        assert_eq!(ms(1), 1000);
        assert_eq!(ms(2), 2000);
        assert_eq!(ms(3), 4000);
        assert_eq!(ms(4), 8000);
        assert_eq!(ms(5), 16_000);
        assert_eq!(ms(6), 30_000, "capped");
        assert_eq!(ms(20), 30_000, "stays capped");
    }

    #[test]
    fn jitter_stays_within_bounds() {
        let delay = Duration::from_millis(1000);
        assert_eq!(jittered(delay, 0.1, 0.0).as_millis(), 900);
        assert_eq!(jittered(delay, 0.1, 1.0).as_millis(), 1100);
        assert_eq!(jittered(delay, 0.1, 0.5).as_millis(), 1000);
        assert_eq!(jittered(delay, 0.0, 0.7), delay);
    }

    fn empty_fetched() -> Arc<Fetched> {
        Arc::new(Fetched {
            snapshot: Snapshot {
                fetched_at: std::time::SystemTime::now(),
                lhm_version: Some("0.9.6".to_string()),
                hardware: Vec::new(),
                skipped_nodes: 0,
            },
            latency: Duration::from_millis(12),
            bytes: 1024,
        })
    }

    #[test]
    fn link_status_transitions() {
        let cfg = Config::default();
        let mut app = AppState::new();
        assert_eq!(app.link, LinkStatus::Connecting);

        // Failure before any data → Reconnecting.
        app.apply_poll_event(
            PollEvent::Failure {
                message: "refused".to_string(),
                retry_in: Duration::from_millis(500),
                attempt: 0,
            },
            &cfg,
        );
        assert_eq!(app.link, LinkStatus::Reconnecting);
        assert_eq!(app.failed_attempts, 1);

        // Data → Live.
        app.apply_poll_event(PollEvent::Snapshot(empty_fetched()), &cfg);
        assert_eq!(app.link, LinkStatus::Live);
        assert_eq!(app.failed_attempts, 0);

        // Failure after data → Stale, keeps the last fetch.
        app.apply_poll_event(
            PollEvent::Failure {
                message: "timeout".to_string(),
                retry_in: Duration::from_secs(1),
                attempt: 0,
            },
            &cfg,
        );
        assert_eq!(app.link, LinkStatus::Stale);
        assert!(app.last_fetch.is_some());
        assert_eq!(app.last_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn sort_cycle_matches_spec() {
        assert_eq!(next_sort(None), Some(TableSort::ValueDesc));
        assert_eq!(
            next_sort(Some(TableSort::ValueDesc)),
            Some(TableSort::ValueAsc)
        );
        assert_eq!(next_sort(Some(TableSort::ValueAsc)), Some(TableSort::Name));
        assert_eq!(
            next_sort(Some(TableSort::Name)),
            Some(TableSort::Unspecified)
        );
        assert_eq!(next_sort(Some(TableSort::Unspecified)), None);
    }

    #[test]
    fn window_cycle_follows_config() {
        let cfg = Config::default();
        let mut app = AppState::new();
        assert_eq!(app.effective_window(&cfg).1, "5m");

        app.window_next(&cfg, 1);
        assert_eq!(app.effective_window(&cfg).1, "15m");
        app.window_next(&cfg, 1);
        assert_eq!(app.effective_window(&cfg).1, "1m", "wraps");
        app.window_next(&cfg, -1);
        assert_eq!(app.effective_window(&cfg).1, "15m", "backwards wraps");
    }

    #[test]
    fn history_always_collected_for_instant_toggle() {
        let cfg = Config::default();
        let mut app = AppState::new();
        app.apply_poll_event(PollEvent::Snapshot(empty_fetched()), &cfg);
        assert!(app.history.is_some());

        let mut disabled = Config::default();
        disabled.charts.enabled = false;
        let mut app = AppState::new();
        app.apply_poll_event(PollEvent::Snapshot(empty_fetched()), &disabled);
        assert!(app.history.is_some(), "collected even with charts disabled");
    }

    #[test]
    fn help_overlay_toggles_and_consumes_keys() {
        let cfg = Config::default();
        let mut app = AppState::new();
        let question = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        handle_event(&mut app, Event::Key(question), &cfg);
        assert!(app.help_visible);

        // Any key — including `q` — just closes the overlay.
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        handle_event(&mut app, Event::Key(q), &cfg);
        assert!(!app.help_visible);
        assert!(!app.should_quit, "q closed the help, not the app");

        // q quits once the overlay is closed.
        handle_event(&mut app, Event::Key(q), &cfg);
        assert!(app.should_quit);
    }

    #[test]
    fn thermal_and_charts_overrides() {
        let cfg = Config::default();
        let mut app = AppState::new();
        assert!(!app.effective_thermal(&cfg), "thermal hidden by default");
        app.thermal_override = Some(true);
        assert!(app.effective_thermal(&cfg));

        assert!(app.effective_charts(&cfg));
        app.charts_override = Some(false);
        assert!(!app.effective_charts(&cfg));

        let mut shown = Config::default();
        shown.ui.layout.push("thermal".to_string());
        let app = AppState::new();
        assert!(
            app.effective_thermal(&shown),
            "layout membership wins without override"
        );
    }

    #[test]
    fn ascii_override_beats_config() {
        let cfg = Config::default();
        let mut app = AppState::new();
        assert!(!app.effective_ascii(&cfg));
        app.ascii_override = Some(true);
        assert!(app.effective_ascii(&cfg));

        let mut ascii_cfg = Config::default();
        ascii_cfg.ui.ascii = true;
        let app = AppState::new();
        assert!(app.effective_ascii(&ascii_cfg));
    }
}
