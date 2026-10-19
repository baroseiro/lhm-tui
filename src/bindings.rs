//! Sensor binding: panel slots → SensorId resolution (SPEC §5.2–5.3).
//!
//! Heuristics follow the candidate tables in SPEC §5.2 (exact-name pass,
//! then fallbacks); `panels.sensor_overrides` can pin or match any slot by
//! key (e.g. `cpu.temp`, `gpu.0.load`). Bindings are recomputed only when the
//! hardware/sensor set changes (fingerprint, SPEC §5.3).

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::config::{PanelsConfig, SensorMatch, SensorOverride};
use crate::lhm::model::{Hardware, HardwareKind, Sensor, SensorKind, Snapshot};

/// A resolved slot: which sensor feeds a panel element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub hardware_id: String,
    pub sensor_id: String,
    /// Display label (sensor name at resolve time).
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuBindings {
    pub hardware_id: String,
    pub name: String,
    pub load_total: Option<Slot>,
    pub core_loads: Vec<Slot>,
    pub temp: Option<Slot>,
    /// Average core clock (`Cores (Average)` on AMD, `Core Average` on Intel).
    pub clock: Option<Slot>,
    /// Average *effective* clock (deep-sleep states counted) — SPEC §5.2.
    pub clock_effective: Option<Slot>,
    pub bus_speed: Option<Slot>,
    pub clock_fabric: Option<Slot>,
    pub clock_uncore: Option<Slot>,
    pub clock_memory: Option<Slot>,
    pub power: Option<Slot>,
    /// Package/core voltage (`Core (SVI2 TFN)`, `CPU Core`, …) — never a VID.
    pub voltage: Option<Slot>,
    /// Package current (`TDC`, `Core TDC`, …).
    pub current: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuBindings {
    pub hardware_id: String,
    pub name: String,
    pub load: Option<Slot>,
    pub mem_load: Option<Slot>,
    pub mem_used: Option<Slot>,
    pub mem_total: Option<Slot>,
    /// Windows "shared GPU memory" (D3D): used / total; total is absent on
    /// NVIDIA cards, in which case the gauge shows the used value only.
    pub shared_used: Option<Slot>,
    pub shared_total: Option<Slot>,
    pub temp: Option<Slot>,
    pub hotspot: Option<Slot>,
    /// GPU memory temperature: NVIDIA "GPU Memory Junction", AMD "GPU Memory".
    pub mem_temp: Option<Slot>,
    pub power: Option<Slot>,
    /// GPU core voltage (`GPU Core Voltage`, `GPU Core`).
    pub voltage: Option<Slot>,
    pub clock: Option<Slot>,
    /// Every fan on the card (multi-fan GPUs), paired with its control by
    /// name. A `gpu.<n>.fan` override pins a single fan.
    pub fans: Vec<FanRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryBindings {
    pub hardware_id: String,
    pub name: String,
    pub used: Option<Slot>,
    pub available: Option<Slot>,
    pub load: Option<Slot>,
    pub virtual_used: Option<Slot>,
    pub virtual_available: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanRow {
    pub hardware_id: String,
    pub fan: Slot,
    pub control: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBindings {
    pub hardware_id: String,
    pub name: String,
    pub temperature: Option<Slot>,
    pub used_space: Option<Slot>,
    pub activity: Option<Slot>,
    pub read_rate: Option<Slot>,
    pub write_rate: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkBindings {
    pub hardware_id: String,
    pub name: String,
    pub upload: Option<Slot>,
    pub download: Option<Slot>,
    pub uploaded: Option<Slot>,
    pub downloaded: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bindings {
    pub cpu: Vec<CpuBindings>,
    pub gpus: Vec<GpuBindings>,
    pub memory: Option<MemoryBindings>,
    pub thermal: Vec<Slot>,
    /// Every voltage sensor in the tree (the `v` table, SPEC §5.2).
    pub voltage: Vec<Slot>,
    pub fans: Vec<FanRow>,
    pub power: Vec<Slot>,
    pub storage: Vec<StorageBindings>,
    pub network: Vec<NetworkBindings>,
}

impl Bindings {
    /// True when at least one preset has something to draw.
    pub fn has_any(&self) -> bool {
        !self.cpu.is_empty()
            || !self.gpus.is_empty()
            || self.memory.is_some()
            || !self.thermal.is_empty()
            || !self.fans.is_empty()
            || !self.power.is_empty()
            || !self.storage.is_empty()
            || !self.network.is_empty()
    }
}

/// Look up the sensor behind a slot in the current snapshot.
pub fn slot_sensor<'a>(snapshot: &'a Snapshot, slot: &Slot) -> Option<&'a Sensor> {
    snapshot
        .hardware(&slot.hardware_id)
        .and_then(|hardware| hardware.sensor(&slot.sensor_id))
}

/// Change-detection fingerprint (SPEC §5.3): identity + names + kinds of all
/// hardware and sensors. Bindings are recomputed only when this changes.
pub fn fingerprint(snapshot: &Snapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    for hardware in snapshot.all_hardware() {
        hardware.id.hash(&mut hasher);
        hardware.name.hash(&mut hasher);
        hardware.kind.hash(&mut hasher);
        for sensor in hardware.sensors() {
            sensor.id.hash(&mut hasher);
            sensor.name.hash(&mut hasher);
            sensor.kind.as_str().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Resolve every panel slot from a snapshot (SPEC §5.2 heuristics + §8 overrides).
pub fn resolve(snapshot: &Snapshot, panels: &PanelsConfig) -> Bindings {
    let overrides = &panels.sensor_overrides;

    let cpu = snapshot
        .hardware
        .iter()
        .filter(|hardware| hardware.kind == HardwareKind::Cpu)
        .map(|hardware| cpu_bindings(snapshot, hardware, overrides))
        .collect();

    let include: Vec<HardwareKind> = panels
        .gpu
        .include
        .iter()
        .filter_map(|name| parse_hardware_kind(name))
        .collect();
    let gpus = snapshot
        .hardware
        .iter()
        .filter(|hardware| hardware.kind.is_gpu() && include.contains(&hardware.kind))
        .enumerate()
        .map(|(index, hardware)| gpu_bindings(snapshot, hardware, index, overrides))
        .collect();

    let memory = memory_bindings(snapshot, overrides);

    let mut thermal: Vec<Slot> = Vec::new();
    let mut voltage: Vec<Slot> = Vec::new();
    let mut fans: Vec<FanRow> = Vec::new();
    let mut power: Vec<Slot> = Vec::new();
    let mut storage: Vec<StorageBindings> = Vec::new();
    let mut network: Vec<NetworkBindings> = Vec::new();

    for hardware in snapshot.all_hardware() {
        for sensor in hardware.sensors() {
            match sensor.kind {
                SensorKind::Temperature => thermal.push(slot_of(hardware, sensor)),
                SensorKind::Voltage => voltage.push(slot_of(hardware, sensor)),
                SensorKind::Fan => {
                    let control = hardware
                        .sensors()
                        .find(|candidate| {
                            candidate.kind == SensorKind::Control && candidate.name == sensor.name
                        })
                        .map(|control| slot_of(hardware, control));
                    fans.push(FanRow {
                        hardware_id: hardware.id.clone(),
                        fan: slot_of(hardware, sensor),
                        control,
                    });
                }
                SensorKind::Power => power.push(slot_of(hardware, sensor)),
                _ => {}
            }
        }
        match hardware.kind {
            HardwareKind::Storage => storage.push(storage_bindings(hardware)),
            HardwareKind::Network => network.push(network_bindings(hardware)),
            _ => {}
        }
    }

    Bindings {
        cpu,
        gpus,
        memory,
        thermal,
        voltage,
        fans,
        power,
        storage,
        network,
    }
}

fn slot_of(hardware: &Hardware, sensor: &Sensor) -> Slot {
    Slot {
        hardware_id: hardware.id.clone(),
        sensor_id: sensor.id.clone(),
        label: sensor.name.clone(),
    }
}

// ---------------------------------------------------------------- CPU

fn cpu_bindings(
    snapshot: &Snapshot,
    hardware: &Hardware,
    overrides: &BTreeMap<String, SensorOverride>,
) -> CpuBindings {
    let load_total = find_slot(snapshot, hardware, overrides, "cpu.load", || {
        find_named(hardware, &SensorKind::Load, &["CPU Total", "Total"]).or_else(|| {
            hardware
                .sensors()
                .find(|sensor| sensor.kind == SensorKind::Load && sensor.id.ends_with("/load/0"))
        })
    });
    let temp = find_slot(snapshot, hardware, overrides, "cpu.temp", || {
        find_named(
            hardware,
            &SensorKind::Temperature,
            &[
                "Core (Tctl/Tdie)",
                "CPU Package",
                "Core Average",
                "CPU Cores",
                "Core Max",
            ],
        )
    });
    let clock = find_slot(snapshot, hardware, overrides, "cpu.clock", || {
        find_named(
            hardware,
            &SensorKind::Clock,
            &[
                "Cores (Average)",
                "Core Average",
                "CPU Core Average",
                "CPU Core #1",
                "P-Core #0",
                "Bus Speed",
            ],
        )
    });
    let clock_effective = find_slot(snapshot, hardware, overrides, "cpu.clock_effective", || {
        find_exact(
            hardware,
            &SensorKind::Clock,
            &[
                "Cores (Average Effective)",
                "Core Average Effective",
                "CPU Core Average Effective",
            ],
        )
    });
    let bus_speed = find_slot(snapshot, hardware, overrides, "cpu.bus_speed", || {
        find_exact(hardware, &SensorKind::Clock, &["Bus Speed"])
    });
    let clock_fabric = find_slot(snapshot, hardware, overrides, "cpu.clock_fabric", || {
        find_exact(hardware, &SensorKind::Clock, &["Fabric", "Infinity Fabric"])
    });
    let clock_uncore = find_slot(snapshot, hardware, overrides, "cpu.clock_uncore", || {
        find_exact(hardware, &SensorKind::Clock, &["Uncore"])
    });
    let clock_memory = find_slot(snapshot, hardware, overrides, "cpu.clock_memory", || {
        find_exact(hardware, &SensorKind::Clock, &["Memory"])
    });
    let power = find_slot(snapshot, hardware, overrides, "cpu.power", || {
        find_named(hardware, &SensorKind::Power, &["Package", "CPU Package"])
    });
    let voltage = find_slot(snapshot, hardware, overrides, "cpu.voltage", || {
        find_named(
            hardware,
            &SensorKind::Voltage,
            &["Core (SVI2 TFN)", "CPU Core", "Core Voltage"],
        )
        .or_else(|| {
            // Fallback: any voltage that is not a per-core VID.
            hardware
                .sensors()
                .find(|sensor| sensor.kind == SensorKind::Voltage && !sensor.name.contains("VID"))
        })
    });
    let current = find_slot(snapshot, hardware, overrides, "cpu.current", || {
        find_named(hardware, &SensorKind::Current, &["TDC", "Core TDC"]).or_else(|| {
            hardware
                .sensors()
                .find(|sensor| sensor.kind == SensorKind::Current)
        })
    });

    let core_loads = if let Some(rule) = overrides.get("cpu.core_loads") {
        resolve_override(snapshot, rule)
            .map(|(hw, sensor)| slot_of(hw, sensor))
            .into_iter()
            .collect()
    } else {
        let regex = regex::Regex::new(r"^CPU Core #(\d+)$").expect("static regex");
        let mut ranked: Vec<(u32, Slot)> = hardware
            .sensors()
            .filter(|sensor| sensor.kind == SensorKind::Load)
            .filter_map(|sensor| {
                regex.captures(&sensor.name).and_then(|captures| {
                    captures[1]
                        .parse::<u32>()
                        .ok()
                        .map(|index| (index, slot_of(hardware, sensor)))
                })
            })
            .collect();
        ranked.sort_by_key(|(index, _)| *index);
        ranked.into_iter().take(64).map(|(_, slot)| slot).collect()
    };

    CpuBindings {
        hardware_id: hardware.id.clone(),
        name: hardware.name.clone(),
        load_total,
        core_loads,
        temp,
        clock,
        clock_effective,
        bus_speed,
        clock_fabric,
        clock_uncore,
        clock_memory,
        power,
        voltage,
        current,
    }
}

// ---------------------------------------------------------------- GPU

fn gpu_bindings(
    snapshot: &Snapshot,
    hardware: &Hardware,
    index: usize,
    overrides: &BTreeMap<String, SensorOverride>,
) -> GpuBindings {
    let key = |slot: &str| format!("gpu.{index}.{slot}");

    let load = find_slot(snapshot, hardware, overrides, &key("load"), || {
        find_named(hardware, &SensorKind::Load, &["GPU Core", "D3D 3D"])
    });
    let mem_load = find_slot(snapshot, hardware, overrides, &key("mem_load"), || {
        find_named(
            hardware,
            &SensorKind::Load,
            &["GPU Memory", "D3D Dedicated Memory Used"],
        )
        .or_else(|| {
            hardware
                .sensors()
                .find(|sensor| sensor.kind == SensorKind::Load && sensor.name.contains("Memory"))
        })
    });
    let mem_used = find_slot(snapshot, hardware, overrides, &key("mem_used"), || {
        find_named_multi(
            hardware,
            &[SensorKind::Data, SensorKind::SmallData],
            &["GPU Memory Used", "D3D Dedicated Memory Used"],
        )
        .or_else(|| find_contains(hardware, &[SensorKind::Data, SensorKind::SmallData], "Used"))
    });
    let mem_total = find_slot(snapshot, hardware, overrides, &key("mem_total"), || {
        find_named_multi(
            hardware,
            &[SensorKind::Data, SensorKind::SmallData],
            &["GPU Memory Total", "D3D Dedicated Memory Total"],
        )
        .or_else(|| {
            find_contains(
                hardware,
                &[SensorKind::Data, SensorKind::SmallData],
                "Total",
            )
            .or_else(|| find_contains(hardware, &[SensorKind::Data, SensorKind::SmallData], "Free"))
        })
    });
    let shared_used = find_slot(snapshot, hardware, overrides, &key("shared_used"), || {
        find_named_multi(
            hardware,
            &[SensorKind::Data, SensorKind::SmallData],
            &["D3D Shared Memory Used", "Shared Memory Used"],
        )
        .or_else(|| {
            hardware.sensors().find(|sensor| {
                matches!(sensor.kind, SensorKind::Data | SensorKind::SmallData)
                    && sensor.name.contains("Shared")
                    && sensor.name.contains("Used")
            })
        })
    });
    let shared_total = find_slot(snapshot, hardware, overrides, &key("shared_total"), || {
        find_named_multi(
            hardware,
            &[SensorKind::Data, SensorKind::SmallData],
            &["D3D Shared Memory Total", "Shared Memory Total"],
        )
        .or_else(|| {
            hardware.sensors().find(|sensor| {
                matches!(sensor.kind, SensorKind::Data | SensorKind::SmallData)
                    && sensor.name.contains("Shared")
                    && sensor.name.contains("Total")
            })
        })
    });
    let temp = find_slot(snapshot, hardware, overrides, &key("temp"), || {
        find_named(
            hardware,
            &SensorKind::Temperature,
            &["GPU Core", "GPU Hot Spot"],
        )
    });
    let hotspot = find_slot(snapshot, hardware, overrides, &key("hotspot"), || {
        find_exact(
            hardware,
            &SensorKind::Temperature,
            &["GPU Hot Spot", "Hot Spot"],
        )
    });
    let mem_temp = find_slot(snapshot, hardware, overrides, &key("mem_temp"), || {
        find_exact(
            hardware,
            &SensorKind::Temperature,
            &["GPU Memory Junction", "GPU Memory", "Memory Junction"],
        )
    });
    let power = find_slot(snapshot, hardware, overrides, &key("power"), || {
        find_named(hardware, &SensorKind::Power, &["GPU Package", "GPU Power"])
    });
    let voltage = find_slot(snapshot, hardware, overrides, &key("voltage"), || {
        find_named(
            hardware,
            &SensorKind::Voltage,
            &["GPU Core Voltage", "GPU Core"],
        )
        .or_else(|| {
            hardware
                .sensors()
                .find(|sensor| sensor.kind == SensorKind::Voltage)
        })
    });
    let clock = find_slot(snapshot, hardware, overrides, &key("clock"), || {
        find_named(hardware, &SensorKind::Clock, &["GPU Core", "GFX Core"])
    });
    // All fans on the card; `gpu.<n>.fan` pins a single one (T2.2 extension).
    let fans = if let Some(rule) = overrides.get(&key("fan")) {
        resolve_override(snapshot, rule)
            .map(|(hardware, sensor)| {
                vec![FanRow {
                    hardware_id: hardware.id.clone(),
                    fan: slot_of(hardware, sensor),
                    control: None,
                }]
            })
            .unwrap_or_default()
    } else {
        hardware
            .sensors()
            .filter(|sensor| sensor.kind == SensorKind::Fan)
            .map(|sensor| {
                let control = hardware
                    .sensors()
                    .find(|candidate| {
                        candidate.kind == SensorKind::Control && candidate.name == sensor.name
                    })
                    .map(|control| slot_of(hardware, control));
                FanRow {
                    hardware_id: hardware.id.clone(),
                    fan: slot_of(hardware, sensor),
                    control,
                }
            })
            .collect()
    };

    GpuBindings {
        hardware_id: hardware.id.clone(),
        name: hardware.name.clone(),
        load,
        mem_load,
        mem_used,
        mem_total,
        shared_used,
        shared_total,
        temp,
        hotspot,
        mem_temp,
        power,
        voltage,
        clock,
        fans,
    }
}

// ---------------------------------------------------------------- memory

fn memory_bindings(
    snapshot: &Snapshot,
    overrides: &BTreeMap<String, SensorOverride>,
) -> Option<MemoryBindings> {
    let candidates: Vec<&Hardware> = snapshot
        .hardware
        .iter()
        .filter(|hardware| hardware.kind == HardwareKind::Memory)
        .collect();
    if candidates.is_empty() {
        return None;
    }

    let is_virtual = |hardware: &Hardware| hardware.name.to_ascii_lowercase().contains("virtual");
    let has_usage = |hardware: &Hardware| {
        hardware
            .sensors()
            .any(|sensor| sensor.kind == SensorKind::Data && sensor.name.contains("Memory Used"))
    };

    let primary = candidates
        .iter()
        .copied()
        .find(|hardware| !is_virtual(hardware) && has_usage(hardware))
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .find(|hardware| !is_virtual(hardware))
        })
        .or_else(|| candidates.first().copied())?;
    let virtual_hw = candidates
        .iter()
        .copied()
        .find(|hardware| is_virtual(hardware));

    let named_data = |hardware: &Hardware, key: &str, names: &[&str]| -> Option<Slot> {
        if let Some(rule) = overrides.get(key) {
            return resolve_override(snapshot, rule).map(|(hw, sensor)| slot_of(hw, sensor));
        }
        find_named(hardware, &SensorKind::Data, names).map(|sensor| slot_of(hardware, sensor))
    };

    let used = named_data(primary, "memory.used", &["Memory Used", "Used Memory"]);
    let available = named_data(
        primary,
        "memory.available",
        &["Memory Available", "Available Memory"],
    );
    let load = if let Some(rule) = overrides.get("memory.load") {
        resolve_override(snapshot, rule).map(|(hw, sensor)| slot_of(hw, sensor))
    } else {
        find_named(primary, &SensorKind::Load, &["Memory"]).map(|sensor| slot_of(primary, sensor))
    };

    let (virtual_used, virtual_available) = match virtual_hw {
        Some(hardware) => (
            named_data(
                hardware,
                "memory.virtual_used",
                &["Memory Used", "Used Memory"],
            ),
            named_data(
                hardware,
                "memory.virtual_available",
                &["Memory Available", "Available Memory"],
            ),
        ),
        None => (None, None),
    };

    Some(MemoryBindings {
        hardware_id: primary.id.clone(),
        name: primary.name.clone(),
        used,
        available,
        load,
        virtual_used,
        virtual_available,
    })
}

// ---------------------------------------------------------------- storage / network

fn storage_bindings(hardware: &Hardware) -> StorageBindings {
    StorageBindings {
        hardware_id: hardware.id.clone(),
        name: hardware.name.clone(),
        temperature: find_named(hardware, &SensorKind::Temperature, &["Temperature"])
            .map(|sensor| slot_of(hardware, sensor)),
        used_space: find_named(hardware, &SensorKind::Load, &["Used Space"])
            .or_else(|| find_contains(hardware, &[SensorKind::Load], "Used"))
            .map(|sensor| slot_of(hardware, sensor)),
        activity: find_named(hardware, &SensorKind::Load, &["Total Activity"])
            .or_else(|| find_contains(hardware, &[SensorKind::Load], "Activity"))
            .map(|sensor| slot_of(hardware, sensor)),
        read_rate: find_contains(hardware, &[SensorKind::Throughput], "Read")
            .map(|sensor| slot_of(hardware, sensor)),
        write_rate: find_contains(hardware, &[SensorKind::Throughput], "Write")
            .map(|sensor| slot_of(hardware, sensor)),
    }
}

fn network_bindings(hardware: &Hardware) -> NetworkBindings {
    NetworkBindings {
        hardware_id: hardware.id.clone(),
        name: hardware.name.clone(),
        upload: find_contains(hardware, &[SensorKind::Throughput], "Upload Speed")
            .or_else(|| find_contains(hardware, &[SensorKind::Throughput], "Upload"))
            .map(|sensor| slot_of(hardware, sensor)),
        download: find_contains(hardware, &[SensorKind::Throughput], "Download Speed")
            .or_else(|| find_contains(hardware, &[SensorKind::Throughput], "Download"))
            .map(|sensor| slot_of(hardware, sensor)),
        uploaded: find_contains(hardware, &[SensorKind::Data], "Data Uploaded")
            .or_else(|| find_contains(hardware, &[SensorKind::Data], "Uploaded"))
            .map(|sensor| slot_of(hardware, sensor)),
        downloaded: find_contains(hardware, &[SensorKind::Data], "Data Downloaded")
            .or_else(|| find_contains(hardware, &[SensorKind::Data], "Downloaded"))
            .map(|sensor| slot_of(hardware, sensor)),
    }
}

// ---------------------------------------------------------------- helpers

fn find_named<'a>(hardware: &'a Hardware, kind: &SensorKind, names: &[&str]) -> Option<&'a Sensor> {
    find_exact(hardware, kind, names)
        .or_else(|| hardware.sensors().find(|sensor| &sensor.kind == kind))
}

/// Exact-name-only search (no "first of kind" fallback): for secondary slots
/// like hotspot / memory temperature, where a fallback would silently show
/// the wrong sensor twice.
fn find_exact<'a>(hardware: &'a Hardware, kind: &SensorKind, names: &[&str]) -> Option<&'a Sensor> {
    for name in names {
        if let Some(sensor) = hardware
            .sensors()
            .find(|sensor| &sensor.kind == kind && sensor.name == *name)
        {
            return Some(sensor);
        }
    }
    None
}

fn find_named_multi<'a>(
    hardware: &'a Hardware,
    kinds: &[SensorKind],
    names: &[&str],
) -> Option<&'a Sensor> {
    for name in names {
        if let Some(sensor) = hardware
            .sensors()
            .find(|sensor| kinds.contains(&sensor.kind) && sensor.name == *name)
        {
            return Some(sensor);
        }
    }
    None
}

fn find_contains<'a>(
    hardware: &'a Hardware,
    kinds: &[SensorKind],
    needle: &str,
) -> Option<&'a Sensor> {
    hardware
        .sensors()
        .find(|sensor| kinds.contains(&sensor.kind) && sensor.name.contains(needle))
}

/// Resolve a slot through its config key: exact sensor id or match rule.
fn find_slot<'a, F>(
    snapshot: &'a Snapshot,
    hardware: &'a Hardware,
    overrides: &BTreeMap<String, SensorOverride>,
    key: &str,
    default: F,
) -> Option<Slot>
where
    F: FnOnce() -> Option<&'a Sensor>,
{
    if let Some(rule) = overrides.get(key) {
        return resolve_override(snapshot, rule).map(|(hw, sensor)| slot_of(hw, sensor));
    }
    default().map(|sensor| slot_of(hardware, sensor))
}

fn resolve_override<'a>(
    snapshot: &'a Snapshot,
    rule: &SensorOverride,
) -> Option<(&'a Hardware, &'a Sensor)> {
    resolve_sensor_ref(snapshot, rule.sensor.as_deref(), rule.matcher.as_ref())
}

/// Resolve a sensor reference (exact id or match rule) — used by
/// `panels.sensor_overrides` and custom-panel widgets.
pub fn resolve_sensor_ref<'a>(
    snapshot: &'a Snapshot,
    sensor_id: Option<&str>,
    matcher: Option<&SensorMatch>,
) -> Option<(&'a Hardware, &'a Sensor)> {
    if let Some(id) = sensor_id {
        return snapshot
            .all_hardware()
            .into_iter()
            .find_map(|hardware| hardware.sensor(id).map(|sensor| (hardware, sensor)));
    }
    resolve_matcher(snapshot, matcher?)
}

fn resolve_matcher<'a>(
    snapshot: &'a Snapshot,
    matcher: &SensorMatch,
) -> Option<(&'a Hardware, &'a Sensor)> {
    let hardware_kind = matcher.hardware.as_deref().and_then(parse_hardware_kind);
    let hardware_id = matcher
        .hardware
        .as_deref()
        .filter(|value| value.starts_with('/'));
    let sensor_kind = matcher.sensor_type.as_deref().map(parse_sensor_kind);
    let name_regex = matcher
        .name_regex
        .as_ref()
        .and_then(|pattern| regex::Regex::new(pattern).ok());

    snapshot.all_hardware().into_iter().find_map(|hardware| {
        if hardware_kind.is_some_and(|kind| hardware.kind != kind) {
            return None;
        }
        if hardware_id.is_some_and(|id| hardware.id != id) {
            return None;
        }
        hardware
            .sensors()
            .find(|sensor| {
                sensor_kind.as_ref().is_none_or(|kind| &sensor.kind == kind)
                    && matcher
                        .name
                        .as_ref()
                        .is_none_or(|name| &sensor.name == name)
                    && name_regex
                        .as_ref()
                        .is_none_or(|regex| regex.is_match(&sensor.name))
            })
            .map(|sensor| (hardware, sensor))
    })
}

/// Build a [`Slot`] from a resolved hardware/sensor pair.
pub fn slot_for(hardware: &Hardware, sensor: &Sensor) -> Slot {
    slot_of(hardware, sensor)
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Accepts `GpuNvidia`, `gpu-nvidia`, `nvidiagpu`, … (SPEC Q5 styles).
pub fn parse_hardware_kind(text: &str) -> Option<HardwareKind> {
    Some(match normalize(text).as_str() {
        "cpu" => HardwareKind::Cpu,
        "gpunvidia" | "nvidiagpu" | "nvidia" => HardwareKind::GpuNvidia,
        "gpuamd" | "atigpu" | "amd" | "amdgpu" => HardwareKind::GpuAmd,
        "gpuintel" | "intelgpu" | "intel" => HardwareKind::GpuIntel,
        "memory" | "ram" | "vram" => HardwareKind::Memory,
        "storage" | "nvme" | "hdd" => HardwareKind::Storage,
        "network" | "nic" => HardwareKind::Network,
        "motherboard" => HardwareKind::Motherboard,
        "superio" | "lpc" => HardwareKind::SuperIO,
        "cooler" => HardwareKind::Cooler,
        "embeddedcontroller" => HardwareKind::EmbeddedController,
        "psu" => HardwareKind::Psu,
        "battery" => HardwareKind::Battery,
        "powermonitor" => HardwareKind::PowerMonitor,
        _ => return None,
    })
}

/// Accepts the wire name (`Load`) or any casing/separator variant.
pub fn parse_sensor_kind(text: &str) -> SensorKind {
    let wire = SensorKind::from_wire(text);
    if wire != SensorKind::Other(text.to_string()) {
        return wire;
    }
    match normalize(text).as_str() {
        "voltage" => SensorKind::Voltage,
        "current" => SensorKind::Current,
        "power" => SensorKind::Power,
        "clock" => SensorKind::Clock,
        "temperature" | "temp" => SensorKind::Temperature,
        "load" => SensorKind::Load,
        "frequency" => SensorKind::Frequency,
        "fan" => SensorKind::Fan,
        "flow" => SensorKind::Flow,
        "control" => SensorKind::Control,
        "level" => SensorKind::Level,
        "factor" => SensorKind::Factor,
        "data" => SensorKind::Data,
        "smalldata" => SensorKind::SmallData,
        "throughput" => SensorKind::Throughput,
        "timespan" => SensorKind::TimeSpan,
        "timing" => SensorKind::Timing,
        "energy" => SensorKind::Energy,
        "noise" => SensorKind::Noise,
        "conductivity" => SensorKind::Conductivity,
        "humidity" => SensorKind::Humidity,
        _ => wire,
    }
}
