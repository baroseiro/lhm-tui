//! Application state and the async event loop (SPEC §4.2).
//!
//! The UI loop never blocks on input or network: crossterm events arrive over
//! an `mpsc` channel from a dedicated input thread, redraws happen on a frame
//! tick only when the screen is dirty (redraw-on-change policy).

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::config::Config;
use crate::ui;

pub struct AppState {
    pub should_quit: bool,
    /// Set when the screen needs a redraw (SPEC §4.2 redraw policy).
    pub dirty: bool,
    pub started_at: Instant,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            dirty: true,
            started_at: Instant::now(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Run the TUI until the user quits. Owns the terminal lifecycle, including
/// restore on every exit path (SPEC §4.2 shutdown).
pub async fn run(cfg: &Config) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, cfg).await;
    ratatui::restore();
    result
}

async fn run_loop(terminal: &mut DefaultTerminal, cfg: &Config) -> Result<()> {
    let mut app = AppState::new();
    let frame_ms = u64::from(1000 / cfg.ui.fps.max(1)).max(1);
    let mut tick = tokio::time::interval(Duration::from_millis(frame_ms));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    spawn_input_thread(tx);

    while !app.should_quit {
        tokio::select! {
            _ = tick.tick() => {
                if app.dirty {
                    terminal.draw(|frame| ui::draw(frame, &app, cfg))?;
                    app.dirty = false;
                }
            }
            event = rx.recv() => match event {
                Some(event) => handle_event(&mut app, event, cfg),
                // The input thread ended (stdin closed): do not hang forever.
                None => app.should_quit = true,
            },
        }
    }
    Ok(())
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

fn handle_event(app: &mut AppState, event: Event, cfg: &Config) {
    match event {
        Event::Resize(_, _) => app.dirty = true,
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if action_matches(cfg, "quit", &key) {
                app.should_quit = true;
            } else {
                // Other actions (pause, window cycling, …) land in later milestones.
                app.dirty = true;
            }
        }
        _ => {}
    }
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
    if key.code != expected.code {
        return false;
    }
    let mut expected_mods = expected.modifiers;
    let mut actual_mods = key.modifiers;
    // Terminals are inconsistent about reporting Shift for uppercase letters.
    if let KeyCode::Char(c) = key.code {
        if c.is_ascii_uppercase() {
            expected_mods.remove(KeyModifiers::SHIFT);
            actual_mods.remove(KeyModifiers::SHIFT);
        }
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
}
