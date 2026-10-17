//! Fixture-driven parsing tests (PLAN T1.6): the version matrix and quirks
//! from SPEC §3.4 must hold for every fixture.

mod support;

use lhm_tui::lhm::model::HardwareKind;
use lhm_tui::lhm::parse::parse_payload;

#[test]
fn lhm_096_amd_nvidia_reference() {
    let snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    assert_eq!(snapshot.lhm_version.as_deref(), Some("0.9.6"));
    assert_eq!(snapshot.hardware_count(), 4);
    assert_eq!(snapshot.sensor_count(), 22);
    assert_eq!(snapshot.skipped_nodes, 0);

    let kinds: Vec<HardwareKind> = snapshot.hardware.iter().map(|hw| hw.kind).collect();
    assert_eq!(
        kinds,
        vec![
            HardwareKind::Cpu,
            HardwareKind::GpuNvidia,
            HardwareKind::Memory,
            HardwareKind::Storage
        ]
    );

    let cpu = &snapshot.hardware[0];
    assert_eq!(cpu.id, "/amdcpu/0");
    assert_eq!(cpu.name, "AMD Ryzen 7 7800X3D");

    let temp = cpu
        .sensor("/amdcpu/0/temperature/2")
        .expect("temperature sensor");
    assert_eq!(temp.value, Some(46.9));
    assert_eq!(temp.min, Some(43.5));
    assert_eq!(temp.max, Some(77.3));
    assert_eq!(temp.value_text, "46.9 °C");

    // Empty formatted values mean "no value", not zero.
    let core_max = cpu
        .sensor("/amdcpu/0/temperature/0")
        .expect("core max sensor");
    assert_eq!(core_max.value, None);
    assert_eq!(core_max.value_text, "");

    let gpu = &snapshot.hardware[1];
    assert_eq!(
        gpu.sensor("/nvidiagpu/0/load/0").expect("gpu load").value,
        Some(78.0)
    );
    assert_eq!(
        gpu.sensor("/nvidiagpu/0/smalldata/1")
            .expect("vram used")
            .value,
        Some(8619.9)
    );

    let ram = &snapshot.hardware[2];
    assert_eq!(
        ram.sensor("/ram/0/data/0").expect("ram used").value,
        Some(20.1)
    );

    let nvme = &snapshot.hardware[3];
    assert_eq!(
        nvme.sensor("/nvme/0/temperature/0")
            .expect("nvme temp")
            .value,
        Some(38.0)
    );
}

#[test]
fn master_numeric_raw_values() {
    let snapshot = parse_payload(&support::fixture("lhm-master.json")).expect("parses");
    assert_eq!(snapshot.lhm_version.as_deref(), Some("0.9.7"));
    assert_eq!(snapshot.hardware_count(), 2);
    assert_eq!(snapshot.sensor_count(), 8);
    assert_eq!(snapshot.skipped_nodes, 0);

    let cpu = &snapshot.hardware[0];
    let temp = cpu
        .sensor("/amdcpu/0/temperature/2")
        .expect("temperature sensor");
    assert_eq!(temp.value, Some(46.9), "numeric RawValue wins");
    assert_eq!(temp.value_text, "46.9 °C", "formatted text still kept");

    let fan = snapshot.hardware[1]
        .sensor("/nvidiagpu/0/fan/0")
        .expect("fan");
    assert_eq!(fan.value, Some(1205.0));
    assert_eq!(fan.kind, lhm_tui::lhm::model::SensorKind::Fan);
}

#[test]
fn lhm_095_without_version_field() {
    let snapshot = parse_payload(&support::fixture("lhm-0.9.5.json")).expect("parses");
    assert_eq!(snapshot.lhm_version, None, "Version is optional (SPEC Q7)");
    assert_eq!(snapshot.hardware_count(), 1);
    assert_eq!(snapshot.sensor_count(), 2);

    let cpu = &snapshot.hardware[0];
    assert_eq!(cpu.kind, HardwareKind::Cpu);
    assert_eq!(
        cpu.sensor("/amdcpu/0/temperature/0")
            .expect("core max")
            .value,
        None
    );
    assert_eq!(
        cpu.sensor("/amdcpu/0/load/0").expect("load").value,
        Some(8.4)
    );
}

#[test]
fn intel_cpu_and_igpu_classification() {
    let snapshot = parse_payload(&support::fixture("lhm-intel-igpu.json")).expect("parses");
    let kinds: Vec<HardwareKind> = snapshot.hardware.iter().map(|hw| hw.kind).collect();
    assert_eq!(kinds, vec![HardwareKind::Cpu, HardwareKind::GpuIntel]);
    assert_eq!(snapshot.sensor_count(), 6);
    assert_eq!(snapshot.hardware[0].id, "/intelcpu/0");
    assert_eq!(snapshot.hardware[1].id, "/gpuintel/0");
}

#[test]
fn dual_gpu_machine() {
    let snapshot = parse_payload(&support::fixture("lhm-2gpu.json")).expect("parses");
    let kinds: Vec<HardwareKind> = snapshot.hardware.iter().map(|hw| hw.kind).collect();
    assert_eq!(kinds, vec![HardwareKind::GpuNvidia, HardwareKind::GpuAmd]);
    assert_eq!(snapshot.sensor_count(), 5);
    assert!(kinds.iter().all(|kind| kind.is_gpu()));
}

#[test]
fn ram_only_machine() {
    let snapshot = parse_payload(&support::fixture("lhm-ram-only.json")).expect("parses");
    assert_eq!(snapshot.hardware_count(), 1);
    assert_eq!(snapshot.hardware[0].kind, HardwareKind::Memory);
    assert_eq!(snapshot.sensor_count(), 3);
}

#[test]
fn nan_and_infinity_are_tolerated() {
    let snapshot = parse_payload(&support::fixture("lhm-nan.json")).expect("parses");
    assert_eq!(snapshot.hardware_count(), 1);
    assert_eq!(snapshot.skipped_nodes, 0);

    let cpu = &snapshot.hardware[0];
    assert_eq!(cpu.sensors().count(), 3);

    let core_average = cpu.sensor("/amdcpu/0/temperature/4").expect("core average");
    assert_eq!(core_average.value, None);
    assert_eq!(
        core_average.value_text, "NaN °C",
        "formatted string preserved"
    );

    let ccd1 = cpu.sensor("/amdcpu/0/temperature/3").expect("ccd1");
    assert_eq!(ccd1.value, None, "±Infinity / null are no values");

    let nan_named = cpu
        .sensor("/amdcpu/0/temperature/9")
        .expect("nan-named sensor");
    assert_eq!(
        nan_named.name, "NaN test sensor",
        "NaN inside strings survives"
    );
}

#[test]
fn thousand_sensor_synthetic_tree() {
    // Generated rather than stored (PLAN T1.6 note): keeps the fixture corpus
    // small while still proving scale.
    use serde_json::{json, Value};

    let mut hardware: Vec<Value> = Vec::new();
    for hw in 0..100 {
        let sensors: Vec<Value> = (0..10)
            .map(|index| {
                json!({
                    "Text": format!("Sensor {index}"),
                    "Min": "0.0 %", "Value": "50.0 %", "Max": "100.0 %",
                    "SensorId": format!("/amdcpu/{hw}/load/{index}"),
                    "Type": "Load",
                    "RawMin": 0.0, "RawValue": 50.0, "RawMax": 100.0,
                    "ImageURL": "images/transparent.png",
                    "Children": []
                })
            })
            .collect();
        hardware.push(json!({
            "Text": format!("CPU {hw}"),
            "Min": "", "Value": "", "Max": "",
            "HardwareId": format!("/amdcpu/{hw}"),
            "ImageURL": "images_icon/cpu.png",
            "Children": [{
                "Text": "Load",
                "Min": "", "Value": "", "Max": "",
                "ImageURL": "images_icon/load.png",
                "Children": sensors
            }]
        }));
    }
    let document = json!({
        "Version": "0.9.6",
        "Text": "Sensor",
        "Min": "Min", "Value": "Value", "Max": "Max",
        "ImageURL": "",
        "Children": hardware
    });

    let snapshot = parse_payload(&document.to_string()).expect("parses");
    assert_eq!(snapshot.hardware_count(), 100);
    assert_eq!(snapshot.sensor_count(), 1000);
    assert_eq!(snapshot.skipped_nodes, 0);
    assert_eq!(
        snapshot.hardware[99]
            .sensor("/amdcpu/99/load/9")
            .expect("last sensor")
            .value,
        Some(50.0)
    );
}
