//! Binding engine tests (PLAN T2.2): slot→SensorId resolution across the
//! fixture matrix, `panels.sensor_overrides`, and panel expansion.

mod support;

use lhm_tui::bindings::{self, Slot};
use lhm_tui::config::{Config, SensorMatch, SensorOverride};
use lhm_tui::lhm::model::{HardwareKind, SensorKind, Snapshot};
use lhm_tui::lhm::parse::parse_payload;
use lhm_tui::ui::presets;

fn snapshot(name: &str) -> Snapshot {
    parse_payload(&support::fixture(name)).expect("fixture parses")
}

fn slot_id(slot: &Option<Slot>) -> Option<&str> {
    slot.as_ref().map(|slot| slot.sensor_id.as_str())
}

fn slot_label(snapshot: &Snapshot, slot: &Option<Slot>) -> Option<String> {
    let slot = slot.as_ref()?;
    bindings::slot_sensor(snapshot, slot).map(|sensor| sensor.name.clone())
}

#[test]
fn reference_fixture_slots() {
    let snapshot = snapshot("lhm-0.9.6.json");
    let cfg = Config::default();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);

    // CPU slots (SPEC §5.2 candidate tables).
    assert_eq!(bindings.cpu.len(), 1);
    let cpu = &bindings.cpu[0];
    assert_eq!(cpu.hardware_id, "/amdcpu/0");
    assert_eq!(slot_id(&cpu.load_total), Some("/amdcpu/0/load/0"));
    assert_eq!(slot_id(&cpu.temp), Some("/amdcpu/0/temperature/2"));
    assert_eq!(slot_id(&cpu.clock), Some("/amdcpu/0/clock/1"));
    assert_eq!(slot_id(&cpu.power), Some("/amdcpu/0/power/0"));
    let core_ids: Vec<&str> = cpu
        .core_loads
        .iter()
        .map(|slot| slot.sensor_id.as_str())
        .collect();
    assert_eq!(core_ids, vec!["/amdcpu/0/load/1", "/amdcpu/0/load/2"]);

    // GPU slots.
    assert_eq!(bindings.gpus.len(), 1);
    let gpu = &bindings.gpus[0];
    assert_eq!(slot_id(&gpu.load), Some("/nvidiagpu/0/load/0"));
    assert_eq!(slot_id(&gpu.mem_load), Some("/nvidiagpu/0/load/1"));
    assert_eq!(slot_id(&gpu.mem_used), Some("/nvidiagpu/0/smalldata/1"));
    assert_eq!(slot_id(&gpu.mem_total), Some("/nvidiagpu/0/smalldata/0"));
    assert_eq!(slot_id(&gpu.temp), Some("/nvidiagpu/0/temperature/0"));
    assert_eq!(slot_id(&gpu.hotspot), Some("/nvidiagpu/0/temperature/2"));
    assert_eq!(
        slot_id(&gpu.mem_temp),
        None,
        "no memory junction in this fixture"
    );
    assert_eq!(slot_id(&gpu.power), Some("/nvidiagpu/0/power/1"));
    assert_eq!(slot_id(&gpu.fan), Some("/nvidiagpu/0/fan/0"));

    // Memory: primary = /ram, no virtual in this fixture.
    let memory = bindings.memory.as_ref().expect("memory binding");
    assert_eq!(memory.hardware_id, "/ram/0");
    assert_eq!(slot_id(&memory.used), Some("/ram/0/data/0"));
    assert_eq!(slot_id(&memory.available), Some("/ram/0/data/1"));
    assert_eq!(slot_id(&memory.load), Some("/ram/0/load/0"));
    assert_eq!(slot_id(&memory.virtual_used), None);

    // Thermal covers every temperature sensor.
    assert_eq!(bindings.thermal.len(), 5);

    // Storage: temperature + used space + activity + read throughput.
    assert_eq!(bindings.storage.len(), 1);
    let drive = &bindings.storage[0];
    assert_eq!(drive.hardware_id, "/nvme/0");
    assert_eq!(slot_id(&drive.temperature), Some("/nvme/0/temperature/0"));
    assert_eq!(slot_id(&drive.used_space), Some("/nvme/0/load/0"));
    assert_eq!(slot_id(&drive.activity), Some("/nvme/0/load/1"));
    assert_eq!(slot_id(&drive.read_rate), Some("/nvme/0/throughput/0"));

    assert!(bindings.network.is_empty());
    assert_eq!(bindings.fans.len(), 1, "the NVIDIA fan binds");
    assert_eq!(bindings.fans[0].fan.sensor_id, "/nvidiagpu/0/fan/0");
    assert!(
        bindings.fans[0].control.is_none(),
        "no Controls group in this fixture"
    );
}

#[test]
fn live_fixture_dual_gpu_machine() {
    let snapshot = snapshot("lhm-live-5900xt.json");
    let cfg = Config::default();
    let bindings = bindings::resolve(&snapshot, &cfg.panels);

    // 32-core CPU: every "CPU Core #N" bar binds, no false positives.
    let cpu = &bindings.cpu[0];
    assert_eq!(cpu.core_loads.len(), 32);
    assert_eq!(slot_id(&cpu.load_total), Some("/amdcpu/0/load/0"));
    assert_eq!(
        slot_label(&snapshot, &cpu.temp).as_deref(),
        Some("Core (Tctl/Tdie)")
    );

    // Both GPUs, NVIDIA first (hardware order), each fully bound.
    assert_eq!(bindings.gpus.len(), 2);
    let nvidia = &bindings.gpus[0];
    let amd = &bindings.gpus[1];
    assert_eq!(nvidia.hardware_id, "/gpu-nvidia/0");
    assert_eq!(amd.hardware_id, "/gpu-amd/0");
    assert_eq!(
        slot_label(&snapshot, &nvidia.mem_load).as_deref(),
        Some("GPU Memory")
    );
    assert_eq!(
        slot_label(&snapshot, &nvidia.fan).as_deref(),
        Some("GPU Fan 1")
    );
    assert_eq!(
        slot_label(&snapshot, &nvidia.mem_temp).as_deref(),
        Some("GPU Memory Junction"),
        "NVIDIA names the VRAM temperature 'GPU Memory Junction'"
    );
    assert_eq!(
        slot_label(&snapshot, &amd.mem_temp).as_deref(),
        Some("GPU Memory"),
        "AMD names the VRAM temperature 'GPU Memory'"
    );
    assert_eq!(
        slot_label(&snapshot, &amd.hotspot).as_deref(),
        Some("GPU Hot Spot")
    );
    assert_eq!(
        slot_label(&snapshot, &amd.mem_used).as_deref(),
        Some("GPU Memory Used")
    );

    // Memory: "Total Memory" primary, "Virtual Memory" as virtual (both kind Memory).
    let memory = bindings.memory.as_ref().expect("memory");
    assert_eq!(memory.hardware_id, "/ram");
    assert_eq!(memory.name, "Total Memory");
    assert!(slot_id(&memory.used).is_some());
    let virtual_used = memory.virtual_used.as_ref().expect("virtual memory bound");
    assert_eq!(virtual_used.hardware_id, "/vram");

    // Thermal: 6 CPU + 3 + 3 GPU sensors; DIMM timings don't leak in.
    assert_eq!(bindings.thermal.len(), 12);

    // Fans: both NVIDIA fans paired with their controls; AMD fan too.
    assert_eq!(bindings.fans.len(), 3);
    assert!(bindings.fans.iter().all(|row| row.control.is_some()));

    // Power: many sensors, CPU package and GPU packages included.
    assert!(bindings.power.len() >= 3);
}

#[test]
fn sensor_overrides_pin_and_match() {
    let snapshot = snapshot("lhm-0.9.6.json");
    let mut cfg = Config::default();

    // Exact pin (heuristic would pick temperature/2).
    cfg.panels.sensor_overrides.insert(
        "cpu.temp".to_string(),
        SensorOverride {
            sensor: Some("/amdcpu/0/temperature/0".to_string()),
            matcher: None,
        },
    );
    // Match rule by hardware kind + sensor type + exact name.
    cfg.panels.sensor_overrides.insert(
        "gpu.0.load".to_string(),
        SensorOverride {
            sensor: None,
            matcher: Some(SensorMatch {
                hardware: Some("GpuNvidia".to_string()),
                sensor_type: Some("Load".to_string()),
                name: Some("GPU Memory".to_string()),
                name_regex: None,
            }),
        },
    );
    // Regex match.
    cfg.panels.sensor_overrides.insert(
        "cpu.clock".to_string(),
        SensorOverride {
            sensor: None,
            matcher: Some(SensorMatch {
                sensor_type: Some("Clock".to_string()),
                name_regex: Some(r"^Cores \(Average\)$".to_string()),
                ..SensorMatch::default()
            }),
        },
    );
    // Unresolvable pin → slot is missing, not an error.
    cfg.panels.sensor_overrides.insert(
        "cpu.power".to_string(),
        SensorOverride {
            sensor: Some("/nope/0".to_string()),
            matcher: None,
        },
    );

    let bindings = bindings::resolve(&snapshot, &cfg.panels);
    let cpu = &bindings.cpu[0];
    assert_eq!(
        slot_id(&cpu.temp),
        Some("/amdcpu/0/temperature/0"),
        "pinned sensor wins"
    );
    assert_eq!(
        slot_id(&cpu.clock),
        Some("/amdcpu/0/clock/1"),
        "regex match"
    );
    assert_eq!(slot_id(&cpu.power), None, "unresolvable override → missing");

    let gpu = &bindings.gpus[0];
    assert_eq!(
        slot_label(&snapshot, &gpu.load).as_deref(),
        Some("GPU Memory")
    );
}

#[test]
fn fingerprint_tracks_hardware_set() {
    let snapshot_a = snapshot("lhm-0.9.6.json");
    let snapshot_b = snapshot("lhm-0.9.6.json");
    let snapshot_other = snapshot("lhm-2gpu.json");

    assert_eq!(
        bindings::fingerprint(&snapshot_a),
        bindings::fingerprint(&snapshot_b)
    );
    assert_ne!(
        bindings::fingerprint(&snapshot_a),
        bindings::fingerprint(&snapshot_other)
    );

    // Same set → same bindings (SPEC §5.3: no rebinding churn).
    let cfg = Config::default();
    assert_eq!(
        bindings::resolve(&snapshot_a, &cfg.panels),
        bindings::resolve(&snapshot_b, &cfg.panels)
    );
}

#[test]
fn panel_list_expands_layout() {
    let cfg = Config::default();
    let reference = snapshot("lhm-0.9.6.json");
    let bindings = bindings::resolve(&reference, &cfg.panels);

    // Thermal is hidden by default (not in the default ui.layout).
    let panels = presets::panel_list(&cfg, &bindings, Default::default());
    let ids: Vec<&str> = panels.iter().map(|panel| panel.id.as_str()).collect();
    assert_eq!(ids, vec!["cpu:0", "gpu:0", "memory"]);

    // The `t` override reveals the temperatures table (appended last).
    let overrides = presets::PanelOverrides {
        thermal: Some(true),
    };
    let panels = presets::panel_list(&cfg, &bindings, overrides);
    let ids: Vec<&str> = panels.iter().map(|panel| panel.id.as_str()).collect();
    assert_eq!(ids, vec!["cpu:0", "gpu:0", "memory", "thermal"]);
    assert!(
        panels
            .iter()
            .find(|panel| panel.id == "thermal")
            .expect("thermal")
            .scrollable
    );

    // Optional presets appear only when enabled AND listed in ui.layout.
    let mut cfg_fans = cfg.clone();
    cfg_fans.panels.fans.enabled = true;
    cfg_fans.ui.layout.push("fans".to_string());
    let live = snapshot("lhm-live-5900xt.json");
    let live_bindings = bindings::resolve(&live, &cfg_fans.panels);
    let ids: Vec<String> = presets::panel_list(&cfg_fans, &live_bindings, Default::default())
        .into_iter()
        .map(|panel| panel.id)
        .collect();
    assert_eq!(ids, vec!["cpu:0", "gpu:0", "gpu:1", "memory", "fans"]);

    // Heights: tables clamp to ui.max_height; chart panels get chart + chrome.
    let thermal = presets::panel_list(&cfg, &live_bindings, overrides)
        .into_iter()
        .find(|panel| panel.id == "thermal")
        .expect("thermal");
    assert_eq!(
        presets::panel_height(&thermal, &live_bindings, &cfg, true),
        cfg.ui.max_height as u16
    );
    let cpu = presets::panel_list(&cfg, &live_bindings, Default::default())
        .into_iter()
        .find(|panel| panel.id == "cpu:0")
        .expect("cpu");
    assert_eq!(
        presets::panel_height(&cpu, &live_bindings, &cfg, true),
        (cfg.charts.height + 5) as u16
    );
    assert_eq!(presets::panel_height(&cpu, &live_bindings, &cfg, false), 7);
}

#[test]
fn layout_rows_and_auto_height() {
    let cfg = Config::default();
    let live = snapshot("lhm-live-5900xt.json");
    let bindings = bindings::resolve(&live, &cfg.panels);
    let panels = presets::panel_list(&cfg, &bindings, Default::default());
    // Default layout: [cpu:0, gpu:0, gpu:1, memory].
    let rows = presets::layout_rows(&panels, 2);
    assert_eq!(rows, vec![vec![0], vec![1, 2], vec![3]]);
    assert_eq!(
        presets::layout_rows(&panels, 1),
        vec![vec![0], vec![1], vec![2], vec![3]]
    );

    // Auto-height distributes spare rows to chart panels, and side-by-side
    // GPUs share one row (never counted twice).
    let rows = presets::layout_rows(&panels, 2);
    let heights = presets::fit_heights(&panels, &bindings, &cfg, 50, true, &rows);
    let base_total = cfg.charts.height + 5 + (cfg.charts.height + 4) + (cfg.charts.height + 4);
    let row_total: u32 = rows.iter().map(|row| u32::from(heights[row[0]])).sum();
    assert_eq!(row_total, 50, "fills the available height: {heights:?}");
    assert!(row_total > base_total);
    assert_eq!(heights[1], heights[2], "row partners share the height");

    // Overflow squeezes chart rows down to the minimum first.
    let heights = presets::fit_heights(&panels, &bindings, &cfg, 20, true, &rows);
    assert_eq!(heights[0], 5 + 4, "cpu at minimum chart height");
    assert_eq!(heights[1], 4 + 4, "gpu at minimum chart height");
    assert_eq!(heights[3], 4 + 4, "memory at minimum chart height");

    // auto_height off keeps base heights.
    let mut fixed = cfg.clone();
    fixed.ui.auto_height = false;
    let heights = presets::fit_heights(&panels, &bindings, &fixed, 50, true, &rows);
    let row_total: u32 = rows.iter().map(|row| u32::from(heights[row[0]])).sum();
    assert_eq!(row_total, base_total);
}

#[test]
fn helpers_accept_id_and_kind_styles() {
    assert_eq!(
        bindings::parse_hardware_kind("GpuNvidia"),
        Some(HardwareKind::GpuNvidia)
    );
    assert_eq!(
        bindings::parse_hardware_kind("gpu-nvidia"),
        Some(HardwareKind::GpuNvidia)
    );
    assert_eq!(
        bindings::parse_hardware_kind("nvidiagpu"),
        Some(HardwareKind::GpuNvidia)
    );
    assert_eq!(
        bindings::parse_hardware_kind("GpuAmd"),
        Some(HardwareKind::GpuAmd)
    );
    assert_eq!(
        bindings::parse_hardware_kind("vram"),
        Some(HardwareKind::Memory)
    );
    assert_eq!(bindings::parse_hardware_kind("nope"), None);

    assert_eq!(bindings::parse_sensor_kind("Load"), SensorKind::Load);
    assert_eq!(
        bindings::parse_sensor_kind("temperature"),
        SensorKind::Temperature
    );
    assert_eq!(
        bindings::parse_sensor_kind("SmallData"),
        SensorKind::SmallData
    );
}
