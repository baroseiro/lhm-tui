//! Widget + preset snapshot tests (PLAN T2.4/T2.5, T3.x) using `TestBackend`.

mod support;

use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::Terminal;

use lhm_tui::bindings::{self, Bindings};
use lhm_tui::charts::ChartsState;
use lhm_tui::config::{Config, Marker, ThemeConfig};
use lhm_tui::history::History;
use lhm_tui::lhm::model::{Hardware, HardwareKind, Sensor, SensorGroup, SensorKind, Snapshot};
use lhm_tui::lhm::parse::parse_payload;
use lhm_tui::theme::Theme;
use lhm_tui::ui::presets::{self, PanelCtx, PanelKind, PanelSpec};
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

fn fixture_snapshot(name: &str) -> Snapshot {
    parse_payload(&support::fixture(name)).expect("fixture parses")
}

#[allow(clippy::too_many_arguments)]
fn draw_preset(
    frame: &mut ratatui::Frame,
    area: Rect,
    spec: &PanelSpec,
    snapshot: &Snapshot,
    bindings: &Bindings,
    cfg: &Config,
    theme: &Theme,
    charts: &mut ChartsState,
    history: Option<&History>,
    scroll: u16,
    ascii: bool,
) {
    let mut ctx = PanelCtx {
        snapshot,
        bindings,
        cfg,
        theme,
        history,
        charts,
        charts_enabled: cfg.charts.enabled,
        stale: false,
        ascii,
        focused: false,
        sort: cfg.ui.table_sort,
        scroll,
        window: Duration::from_secs(300),
        window_label: "5m".to_string(),
        now: Instant::now(),
    };
    presets::draw_panel(frame, area, spec, &mut ctx);
}

// Synthetic machine with deterministic wave-shaped values so chart snapshots
// show real shapes without depending on wall-clock time.
fn dev_sensor(id: &str, name: &str, kind: SensorKind, value: f64) -> Sensor {
    Sensor {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        value: Some(value),
        value_text: String::new(),
        min: None,
        max: None,
    }
}

fn dev_machine(step: usize) -> Snapshot {
    let t = step as f64;
    let load = 45.0 + (t * 0.30).sin() * 30.0 + (t * 1.10).sin() * 6.0;
    let temp = 58.0 + (t * 0.18).sin() * 12.0 + (t * 0.65).sin() * 3.0;
    let hot_spot = temp + 9.0 + (t * 0.4).sin() * 2.0;
    let mem_temp = temp + 6.0;
    let gpu_load = 30.0 + (t * 0.22).sin() * 25.0 + 20.0;
    let mem_used = 8000.0 + (t * 0.1).sin() * 2000.0;

    let cpu = Hardware {
        id: "/amdcpu/0".to_string(),
        name: "Dev CPU 16-Core".to_string(),
        kind: HardwareKind::Cpu,
        groups: vec![
            SensorGroup {
                name: "Load".to_string(),
                sensors: vec![dev_sensor(
                    "/amdcpu/0/load/0",
                    "CPU Total",
                    SensorKind::Load,
                    load,
                )],
            },
            SensorGroup {
                name: "Temperatures".to_string(),
                sensors: vec![dev_sensor(
                    "/amdcpu/0/temperature/2",
                    "Core (Tctl/Tdie)",
                    SensorKind::Temperature,
                    temp,
                )],
            },
        ],
        sub: Vec::new(),
    };
    let gpu = Hardware {
        id: "/gpu-nvidia/0".to_string(),
        name: "Dev RTX".to_string(),
        kind: HardwareKind::GpuNvidia,
        groups: vec![
            SensorGroup {
                name: "Load".to_string(),
                sensors: vec![
                    dev_sensor(
                        "/gpu-nvidia/0/load/0",
                        "GPU Core",
                        SensorKind::Load,
                        gpu_load,
                    ),
                    dev_sensor(
                        "/gpu-nvidia/0/load/3",
                        "GPU Memory",
                        SensorKind::Load,
                        gpu_load * 0.2,
                    ),
                ],
            },
            SensorGroup {
                name: "Temperatures".to_string(),
                sensors: vec![
                    dev_sensor(
                        "/gpu-nvidia/0/temperature/0",
                        "GPU Core",
                        SensorKind::Temperature,
                        temp,
                    ),
                    dev_sensor(
                        "/gpu-nvidia/0/temperature/2",
                        "GPU Hot Spot",
                        SensorKind::Temperature,
                        hot_spot,
                    ),
                    dev_sensor(
                        "/gpu-nvidia/0/temperature/3",
                        "GPU Memory Junction",
                        SensorKind::Temperature,
                        mem_temp,
                    ),
                ],
            },
            SensorGroup {
                name: "Data".to_string(),
                sensors: vec![
                    dev_sensor(
                        "/gpu-nvidia/0/data/0",
                        "GPU Memory Used",
                        SensorKind::Data,
                        mem_used / 1024.0,
                    ),
                    dev_sensor(
                        "/gpu-nvidia/0/data/1",
                        "GPU Memory Total",
                        SensorKind::Data,
                        24.0,
                    ),
                ],
            },
        ],
        sub: Vec::new(),
    };
    Snapshot {
        fetched_at: std::time::SystemTime::now(),
        lhm_version: Some("0.9.6".to_string()),
        hardware: vec![cpu, gpu],
        skipped_nodes: 0,
    }
}

/// 120 steps of dev-machine history (capacity covers it at 300 s window).
fn dev_history() -> (Snapshot, History) {
    let mut history = History::new(300, 1000, 3600);
    let mut last = dev_machine(0);
    for step in 0..120 {
        last = dev_machine(step);
        history.append_snapshot(&last);
    }
    (last, history)
}

// ------------------------------------------------------------ leaf widgets

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
fn gauge_alignment_matches_across_labels() {
    let theme = theme();
    let out = render(44, 2, |frame| {
        let rows =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(frame.area());
        widgets::gauge(
            frame,
            rows[0],
            "Temp",
            "46.9 °C",
            0.47,
            Style::new(),
            &theme,
            false,
        );
        widgets::gauge(
            frame,
            rows[1],
            "Hot Spot",
            "61.5 °C",
            0.61,
            Style::new(),
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

// ------------------------------------------------------------ preset panels

#[test]
fn cpu_panel_gauges_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.enabled = false;
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(70, 7, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn cpu_panel_chart_snapshot() {
    let cfg = Config::default();
    let (snapshot, history) = dev_history();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(80, 13, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            Some(&history),
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn gpu_panel_gauges_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.enabled = false;
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "gpu:0".to_string(),
        kind: PanelKind::Gpu(0),
        scrollable: false,
    };
    let out = render(70, 8, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn gpu_panel_chart_snapshot() {
    let cfg = Config::default();
    let (snapshot, history) = dev_history();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "gpu:0".to_string(),
        kind: PanelKind::Gpu(0),
        scrollable: false,
    };
    let out = render(76, 12, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            Some(&history),
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn memory_panel_gauges_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.enabled = false;
    let snapshot = fixture_snapshot("lhm-live-5900xt.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "memory".to_string(),
        kind: PanelKind::Memory,
        scrollable: false,
    };
    let out = render(70, 6, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn memory_panel_chart_snapshot() {
    let cfg = Config::default();
    let (snapshot, history) = dev_history();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "memory".to_string(),
        kind: PanelKind::Memory,
        scrollable: false,
    };
    // The dev machine has no memory hardware → panel renders nothing; assert
    // the empty render is stable instead.
    let out = render(70, 12, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            Some(&history),
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn ascii_marker_chart_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.marker = Marker::Ascii;
    let (snapshot, history) = dev_history();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(80, 13, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            Some(&history),
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn thermal_panel_snapshot() {
    let cfg = Config::default();
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "thermal".to_string(),
        kind: PanelKind::Thermal,
        scrollable: true,
    };
    let out = render(64, 8, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn custom_panel_snapshot() {
    let mut cfg = Config::default();
    cfg.panels.custom.push(lhm_tui::config::CustomPanel {
        id: "nvme".to_string(),
        title: Some("NVMe".to_string()),
        widgets: vec![
            lhm_tui::config::CustomWidget {
                kind: lhm_tui::config::WidgetKind::Graph,
                sensor: Some("/nvme/0/temperature/0".to_string()),
                label: Some("Temp".to_string()),
                unit: Some("°C".to_string()),
                color: Some("storage".to_string()),
                ..Default::default()
            },
            lhm_tui::config::CustomWidget {
                kind: lhm_tui::config::WidgetKind::Gauge,
                sensor: Some("/nvme/0/load/0".to_string()),
                label: Some("Activity".to_string()),
                unit: Some("%".to_string()),
                y_mode: Some(lhm_tui::config::YMode::Percent),
                ..Default::default()
            },
        ],
    });
    cfg.ui.layout.push("custom:nvme".to_string());

    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let mut history = History::new(300, 1000, 3600);
    history.append_snapshot(&snapshot);
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "custom:nvme".to_string(),
        kind: PanelKind::Custom("nvme".to_string()),
        scrollable: false,
    };
    let out = render(64, 11, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            Some(&history),
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn custom_panel_missing_sensor_snapshot() {
    let mut cfg = Config::default();
    cfg.panels.custom.push(lhm_tui::config::CustomPanel {
        id: "ghost".to_string(),
        title: None,
        widgets: vec![
            lhm_tui::config::CustomWidget {
                kind: lhm_tui::config::WidgetKind::Gauge,
                sensor: Some("/nope/0".to_string()),
                ..Default::default()
            },
            lhm_tui::config::CustomWidget {
                kind: lhm_tui::config::WidgetKind::Sparkline,
                sensor: Some("/nope/1".to_string()),
                ..Default::default()
            },
        ],
    });
    cfg.ui.layout.push("custom:ghost".to_string());

    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "custom:ghost".to_string(),
        kind: PanelKind::Custom("ghost".to_string()),
        scrollable: false,
    };
    let out = render(56, 5, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn custom_table_snapshot() {
    let mut cfg = Config::default();
    cfg.panels.custom.push(lhm_tui::config::CustomPanel {
        id: "drives".to_string(),
        title: Some("Drive temps".to_string()),
        widgets: vec![lhm_tui::config::CustomWidget {
            kind: lhm_tui::config::WidgetKind::Table,
            filter: Some(lhm_tui::config::WidgetFilter {
                hardware: Some("/nvme/0".to_string()),
                types: vec!["Temperature".to_string()],
                name_regex: None,
            }),
            ..Default::default()
        }],
    });
    cfg.ui.layout.push("custom:drives".to_string());

    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "custom:drives".to_string(),
        kind: PanelKind::Custom("drives".to_string()),
        scrollable: true,
    };
    let out = render(64, 11, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn cpu_show_subset_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.enabled = false;
    cfg.panels.cpu.show = vec!["temp".to_string()];
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();
    let spec = PanelSpec {
        id: "cpu:0".to_string(),
        kind: PanelKind::Cpu(0),
        scrollable: false,
    };
    let out = render(60, 3, |frame| {
        draw_preset(
            frame,
            frame.area(),
            &spec,
            &snapshot,
            &bindings,
            &cfg,
            &theme,
            &mut charts,
            None,
            0,
            false,
        );
    });
    insta::assert_snapshot!(out);
}

#[test]
fn help_overlay_snapshot() {
    use lhm_tui::app::{AppState, PollEvent};
    use lhm_tui::lhm::client::Fetched;
    use std::sync::Arc;

    let cfg = Config::default();
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let mut app = AppState::new();
    app.apply_poll_event(
        PollEvent::Snapshot(Arc::new(Fetched {
            snapshot,
            latency: Duration::from_millis(5),
            bytes: 74_000,
        })),
        &cfg,
    );
    app.help_visible = true;

    let out = render(80, 26, |frame| {
        lhm_tui::ui::draw(frame, &mut app, &cfg);
    });
    insta::assert_snapshot!(out);
}
#[test]
fn fans_power_storage_panels_snapshot() {
    let mut cfg = Config::default();
    cfg.charts.enabled = false;
    cfg.panels.fans.enabled = true;
    cfg.panels.power.enabled = true;
    cfg.panels.storage.enabled = true;
    let snapshot = fixture_snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let theme = theme();
    let mut charts = ChartsState::new();

    let out = render(64, 16, |frame| {
        let areas = Layout::vertical([
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Length(4),
        ])
        .split(frame.area());
        for (area, kind) in [
            (areas[0], PanelKind::Fans),
            (areas[1], PanelKind::Power),
            (areas[2], PanelKind::Storage),
        ] {
            let id = match &kind {
                PanelKind::Fans => "fans",
                PanelKind::Power => "power",
                PanelKind::Storage => "storage",
                _ => unreachable!(),
            };
            let spec = PanelSpec {
                id: id.to_string(),
                kind,
                scrollable: true,
            };
            draw_preset(
                frame,
                area,
                &spec,
                &snapshot,
                &bindings,
                &cfg,
                &theme,
                &mut charts,
                None,
                0,
                false,
            );
        }
    });
    insta::assert_snapshot!(out);
}
