//! Widget + preset snapshot tests (PLAN T2.4/T2.5) using `TestBackend`.

mod support;

use ratatui::backend::TestBackend;
use ratatui::style::{Color, Style};
use ratatui::Terminal;

use lhm_tui::bindings;
use lhm_tui::config::{Config, ThemeConfig};
use lhm_tui::lhm::parse::parse_payload;
use lhm_tui::theme::Theme;
use lhm_tui::ui::presets::{self, PanelKind, PanelSpec};
use lhm_tui::ui::{format, widgets};

fn render(width: u16, height: u16, draw: impl FnOnce(&mut ratatui::Frame)) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| draw(frame)).expect("draw");
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

fn theme() -> Theme {
    Theme::from_config(&ThemeConfig::default())
}

#[test]
fn gauge_snapshot() {
    let theme = theme();
    let out = render(44, 1, |frame| {
        widgets::gauge(
            frame,
            frame.area(),
            "Temp",
            &format::temperature(46.9),
            0.47,
            Style::new().fg(Color::Gray),
            &theme,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn gauge_stale_snapshot() {
    let theme = theme();
    let out = render(44, 1, |frame| {
        widgets::gauge(
            frame,
            frame.area(),
            "Load",
            &format::percent(41.2),
            0.41,
            Style::new().fg(Color::Gray),
            &theme,
            true,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn bars_snapshot() {
    let theme = theme();
    let values = [
        Some(0.0),
        Some(12.5),
        Some(25.0),
        Some(50.0),
        Some(75.0),
        Some(100.0),
        None,
        Some(42.0),
    ];
    let out = render(40, 2, |frame| {
        widgets::bars(frame, frame.area(), &values, &theme, false);
    });
    insta::assert_snapshot!(out);
}

#[test]
fn sparkline_snapshot() {
    let theme = theme();
    let values = [1.0, 3.0, 2.0, 5.0, 4.0, 8.0, 6.0, 9.0, 7.0, 9.5];
    let out = render(40, 1, |frame| {
        widgets::sparkline(
            frame,
            frame.area(),
            &values,
            Style::new().fg(theme.memory),
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn cpu_panel_snapshot() {
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(70, 7, |frame| {
        presets::draw_panel(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            false,
            true,
            cfg.ui.table_sort,
            0,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn cpu_panel_live_snapshot() {
    // The real 32-core machine: exercises two rows of core bars.
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-live-5900xt.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(80, 7, |frame| {
        presets::draw_panel(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            false,
            false,
            cfg.ui.table_sort,
            0,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn gpu_panel_snapshot() {
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let spec = PanelSpec {
        id: "gpu:0".to_string(),
        kind: PanelKind::Gpu(0),
        scrollable: false,
    };
    let out = render(70, 7, |frame| {
        presets::draw_panel(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            false,
            false,
            cfg.ui.table_sort,
            0,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn memory_panel_snapshot() {
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-live-5900xt.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let spec = PanelSpec {
        id: "memory".to_string(),
        kind: PanelKind::Memory,
        scrollable: false,
    };
    let out = render(70, 6, |frame| {
        presets::draw_panel(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            false,
            false,
            cfg.ui.table_sort,
            0,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn thermal_panel_snapshot() {
    let cfg = Config::default();
    let snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let spec = PanelSpec {
        id: "thermal".to_string(),
        kind: PanelKind::Thermal,
        scrollable: true,
    };
    let out = render(64, 8, |frame| {
        presets::draw_panel(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            false,
            false,
            cfg.ui.table_sort,
            0,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn fans_power_storage_panels_snapshot() {
    let mut cfg = Config::default();
    cfg.panels.fans.enabled = true;
    cfg.panels.power.enabled = true;
    cfg.panels.storage.enabled = true;
    let snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();

    let out = render(64, 16, |frame| {
        let areas = ratatui::layout::Layout::vertical([
            ratatui::layout::Constraint::Length(4),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Length(4),
        ])
        .split(frame.area());
        for (area, kind, scrollable) in [
            (areas[0], PanelKind::Fans, true),
            (areas[1], PanelKind::Power, true),
            (areas[2], PanelKind::Storage, true),
        ] {
            let spec = PanelSpec {
                id: match &kind {
                    PanelKind::Fans => "fans",
                    PanelKind::Power => "power",
                    PanelKind::Storage => "storage",
                    _ => unreachable!(),
                }
                .to_string(),
                kind,
                scrollable,
            };
            presets::draw_panel(
                frame,
                area,
                &spec,
                &snapshot,
                &bindings,
                &cfg,
                &theme,
                false,
                false,
                cfg.ui.table_sort,
                0,
            );
        }
    });
    insta::assert_snapshot!(out);
}
