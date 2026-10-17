//! Live checks against a locally running LibreHardwareMonitor.
//!
//! Ignored by default (CI has no LHM). Run manually:
//! `cargo test --test live -- --ignored --nocapture`

use std::sync::Arc;

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

use lhm_tui::app::{handle_event, AppState, PollEvent};
use lhm_tui::bindings;
use lhm_tui::config::Config;
use lhm_tui::lhm::client::{Fetched, LhmClient};

async fn live_fetch(cfg: &Config) -> Fetched {
    let client = LhmClient::new(&cfg.server).expect("client");
    client
        .fetch_data_json()
        .await
        .expect("LibreHardwareMonitor must be running on localhost:8085")
}

fn render_to_string(
    terminal: &mut Terminal<TestBackend>,
    app: &mut AppState,
    cfg: &Config,
) -> String {
    terminal
        .draw(|frame| lhm_tui::ui::draw(frame, app, cfg))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "));
        }
        text.push('\n');
    }
    text
}

fn key(character: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
}

#[tokio::test]
#[ignore = "requires LibreHardwareMonitor on localhost:8085"]
async fn live_probe_and_bindings() {
    let cfg = Config::default();
    let fetched = live_fetch(&cfg).await;
    let snapshot = &fetched.snapshot;
    let bindings = bindings::resolve(snapshot, &cfg.panels);

    println!(
        "live: {} hardware, {} sensors, {} gpus, {} thermal rows, {} fans, {} power",
        snapshot.hardware_count(),
        snapshot.sensor_count(),
        bindings.gpus.len(),
        bindings.thermal.len(),
        bindings.fans.len(),
        bindings.power.len(),
    );
    for cpu in &bindings.cpu {
        println!(
            "  cpu {}: load={:?} temp={:?} clock={:?} power={:?} cores={}",
            cpu.name,
            cpu.load_total.as_ref().map(|slot| slot.sensor_id.as_str()),
            cpu.temp.as_ref().map(|slot| slot.sensor_id.as_str()),
            cpu.clock.as_ref().map(|slot| slot.sensor_id.as_str()),
            cpu.power.as_ref().map(|slot| slot.sensor_id.as_str()),
            cpu.core_loads.len(),
        );
    }
    for (index, gpu) in bindings.gpus.iter().enumerate() {
        println!(
            "  gpu {index} {}: load={:?} temp={:?} mem_temp={:?} vram={:?}",
            gpu.name,
            gpu.load.as_ref().map(|slot| slot.sensor_id.as_str()),
            gpu.temp.as_ref().map(|slot| slot.sensor_id.as_str()),
            gpu.mem_temp.as_ref().map(|slot| slot.sensor_id.as_str()),
            gpu.mem_used.as_ref().map(|slot| slot.sensor_id.as_str()),
        );
    }
    if let Some(memory) = &bindings.memory {
        println!(
            "  memory {}: used={:?} available={:?} virtual={:?}",
            memory.name,
            memory.used.as_ref().map(|slot| slot.sensor_id.as_str()),
            memory
                .available
                .as_ref()
                .map(|slot| slot.sensor_id.as_str()),
            memory
                .virtual_used
                .as_ref()
                .map(|slot| slot.sensor_id.as_str()),
        );
    }

    assert!(snapshot.hardware_count() >= 1, "hardware present");
    assert!(snapshot.sensor_count() > 20, "sensors present");
    assert!(!bindings.cpu.is_empty(), "cpu bound");
    assert!(!bindings.thermal.is_empty(), "thermal bound");
    assert!(bindings.memory.is_some(), "memory bound");
}

#[tokio::test]
#[ignore = "requires LibreHardwareMonitor on localhost:8085"]
async fn live_full_dashboard_render() {
    let cfg = Config::default();
    let mut app = AppState::new();

    // A few real polls so the charts have a small trend to draw.
    for _ in 0..4 {
        let fetched = live_fetch(&cfg).await;
        app.apply_poll_event(PollEvent::Snapshot(Arc::new(fetched)), &cfg);
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let backend = TestBackend::new(140, 44);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let text = render_to_string(&mut terminal, &mut app, &cfg);
    println!("{text}");

    assert!(text.contains("CPU"), "cpu panel rendered");
    assert!(text.contains("GPU 0"), "first gpu panel rendered");
    assert!(text.contains("GPU 1"), "second gpu panel rendered");
    assert!(text.contains("Memory"), "memory panel rendered");
    assert!(text.contains("live"), "status header rendered");
    assert!(
        !text.contains("Temperatures"),
        "thermal table hidden by default"
    );

    // `t` reveals the temperatures table.
    handle_event(&mut app, key('t'), &cfg);
    let text = render_to_string(&mut terminal, &mut app, &cfg);
    println!("{text}");
    assert!(text.contains("Temperatures"), "t shows the thermal table");

    // `c` switches to the gauges layout (which has a Mem Temp row).
    handle_event(&mut app, key('c'), &cfg);
    let text = render_to_string(&mut terminal, &mut app, &cfg);
    println!("{text}");
    assert!(text.contains("Mem Temp"), "c switches to gauge panels");
}
