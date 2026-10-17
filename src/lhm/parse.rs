//! Normalization: raw `data.json` tree → [`Snapshot`] (SPEC §3.4 Q5–Q6, §5.1).

use std::time::SystemTime;

use crate::lhm::model::{Hardware, HardwareKind, Sensor, SensorGroup, SensorKind, Snapshot};
use crate::lhm::raw::RawNode;

/// Sanitize and parse a raw `/data.json` payload into a snapshot
/// (used by the client, tests, fixtures and future dump modes).
pub fn parse_payload(text: &str) -> Result<Snapshot, serde_json::Error> {
    let root: RawNode = serde_json::from_str(&crate::lhm::raw::sanitize(text))?;
    Ok(parse_tree(&root))
}

/// Parse the root `data.json` node into a normalized snapshot.
/// Unrecognized nodes are skipped and counted, never fatal (SPEC Q6).
pub fn parse_tree(root: &RawNode) -> Snapshot {
    let mut skipped = 0usize;
    let mut hardware = Vec::new();

    collect_hardware(&root.children, &mut hardware, &mut skipped);

    Snapshot {
        fetched_at: SystemTime::now(),
        lhm_version: root.version.clone(),
        hardware,
        skipped_nodes: skipped,
    }
}

/// Collect hardware nodes, descending through container nodes: real LHM
/// builds wrap hardware in a single machine node (e.g. `SILVERWIN` with
/// `computer.png`), while older captures expose hardware directly under the
/// root. Both shapes must work.
fn collect_hardware(nodes: &[RawNode], out: &mut Vec<Hardware>, skipped: &mut usize) {
    for node in nodes {
        if node.hardware_id.is_some() {
            if let Some(item) = parse_hardware(node, skipped) {
                out.push(item);
            }
        } else if node.sensor_id.is_none() && !node.children.is_empty() {
            collect_hardware(&node.children, out, skipped);
        } else {
            *skipped += 1;
        }
    }
}

fn parse_hardware(node: &RawNode, skipped: &mut usize) -> Option<Hardware> {
    let id = node.hardware_id.clone()?;
    let mut groups = Vec::new();
    let mut sub = Vec::new();

    for child in &node.children {
        if child.hardware_id.is_some() {
            if let Some(item) = parse_hardware(child, skipped) {
                sub.push(item);
            }
            continue;
        }

        if child.sensor_id.is_some() {
            // Sensor directly under hardware (no type group): keep it as its
            // own unnamed group rather than dropping it.
            groups.push(SensorGroup {
                name: String::new(),
                sensors: vec![parse_sensor(child)],
            });
            continue;
        }

        // Type node ("Temperatures", "Load", …) containing sensor children.
        let mut sensors = Vec::new();
        for grandchild in &child.children {
            if grandchild.sensor_id.is_some() {
                sensors.push(parse_sensor(grandchild));
            } else {
                *skipped += 1;
            }
        }
        if !sensors.is_empty() {
            groups.push(SensorGroup {
                name: child.text.clone().unwrap_or_default(),
                sensors,
            });
        }
    }

    Some(Hardware {
        id,
        name: node.text.clone().unwrap_or_default(),
        kind: classify(node),
        groups,
        sub,
    })
}

fn parse_sensor(node: &RawNode) -> Sensor {
    Sensor {
        id: node.sensor_id.clone().unwrap_or_default(),
        name: node.text.clone().unwrap_or_default(),
        kind: SensorKind::from_wire(node.sensor_type.as_deref().unwrap_or_default()),
        // Raw numeric preferred, formatted text as fallback (SPEC Q1).
        value: node.raw_value.value.or(node.value.value),
        value_text: node.value.text.clone().unwrap_or_default(),
        min: node.raw_min.value.or(node.min.value),
        max: node.raw_max.value.or(node.max.value),
    }
}

/// Hardware type from the icon file name, falling back to the id prefix
/// (SPEC Q5). The icon mapping mirrors LHM's `GetHardwareImageFile` switch.
fn classify(node: &RawNode) -> HardwareKind {
    let icon = node
        .image_url
        .as_deref()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("");
    let by_icon = match icon {
        "cpu.png" => Some(HardwareKind::Cpu),
        "nvidia.png" => Some(HardwareKind::GpuNvidia),
        "ati.png" => Some(HardwareKind::GpuAmd),
        "intel.png" => Some(HardwareKind::GpuIntel),
        "ram.png" => Some(HardwareKind::Memory),
        "hdd.png" => Some(HardwareKind::Storage),
        "nic.png" => Some(HardwareKind::Network),
        "mainboard.png" | "motherboard.png" => Some(HardwareKind::Motherboard),
        "chip.png" => Some(HardwareKind::SuperIO),
        "cooler.png" => Some(HardwareKind::Cooler),
        "embeddedcontroller.png" => Some(HardwareKind::EmbeddedController),
        "psu.png" => Some(HardwareKind::Psu),
        "battery.png" => Some(HardwareKind::Battery),
        _ => None,
    };
    if let Some(kind) = by_icon {
        return kind;
    }

    let prefix = node
        .hardware_id
        .as_deref()
        .unwrap_or("")
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match prefix.as_str() {
        "amdcpu" | "intelcpu" | "cpu" => HardwareKind::Cpu,
        // Both the classic (`nvidiagpu`) and hyphenated (`gpu-nvidia`)
        // id styles exist across LHM versions.
        "nvidiagpu" | "gpu-nvidia" => HardwareKind::GpuNvidia,
        "atigpu" | "gpu-amd" => HardwareKind::GpuAmd,
        "gpuintel" | "gpu-intel" => HardwareKind::GpuIntel,
        "ram" | "vram" | "memory" => HardwareKind::Memory,
        "nvme" | "hdd" => HardwareKind::Storage,
        "nic" => HardwareKind::Network,
        "motherboard" => HardwareKind::Motherboard,
        "lpc" => HardwareKind::SuperIO,
        "cooler" => HardwareKind::Cooler,
        "embeddedcontroller" => HardwareKind::EmbeddedController,
        "psu" => HardwareKind::Psu,
        "battery" => HardwareKind::Battery,
        "powermonitor" | "powersupply" => HardwareKind::PowerMonitor,
        _ => HardwareKind::Other,
    }
}
