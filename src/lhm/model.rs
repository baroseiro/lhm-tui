//! Normalized domain model (SPEC §5.1).
//!
//! Identity is always the LHM `SensorId`/`HardwareId` string (SPEC Q3);
//! node `id` values from `data.json` are request-scoped and never used.

use std::fmt;
use std::time::SystemTime;

/// One parsed `/data.json` response.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub fetched_at: SystemTime,
    pub lhm_version: Option<String>,
    pub hardware: Vec<Hardware>,
    /// Nodes that did not fit the hardware / group / sensor shape.
    pub skipped_nodes: usize,
}

#[derive(Debug, Clone)]
pub struct Hardware {
    /// LHM hardware id, e.g. `/amdcpu/0`.
    pub id: String,
    /// Display name, e.g. `AMD Ryzen 7 7800X3D`.
    pub name: String,
    pub kind: HardwareKind,
    /// Sensor groups in LHM order (`Temperatures`, `Load`, …), flattened view
    /// available via [`Hardware::sensors`].
    pub groups: Vec<SensorGroup>,
    /// Sub-hardware (e.g. an embedded controller under the mainboard).
    pub sub: Vec<Hardware>,
}

#[derive(Debug, Clone)]
pub struct SensorGroup {
    pub name: String,
    pub sensors: Vec<Sensor>,
}

#[derive(Debug, Clone)]
pub struct Sensor {
    /// LHM sensor id, e.g. `/amdcpu/0/temperature/2`.
    pub id: String,
    pub name: String,
    pub kind: SensorKind,
    /// Numeric value: raw when available, else parsed from the formatted text.
    pub value: Option<f64>,
    /// Original formatted string (e.g. `46.9 °C`), for verbatim display.
    pub value_text: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Hardware {
    /// All sensors of this hardware item, in group order.
    pub fn sensors(&self) -> impl Iterator<Item = &Sensor> {
        self.groups.iter().flat_map(|group| group.sensors.iter())
    }

    /// Look up a sensor by exact [`Sensor::id`].
    pub fn sensor(&self, id: &str) -> Option<&Sensor> {
        self.sensors().find(|sensor| sensor.id == id)
    }
}

impl Snapshot {
    /// All hardware including sub-hardware, in tree order.
    pub fn all_hardware(&self) -> Vec<&Hardware> {
        fn walk<'a>(hardware: &'a Hardware, out: &mut Vec<&'a Hardware>) {
            out.push(hardware);
            for sub in &hardware.sub {
                walk(sub, out);
            }
        }

        let mut out = Vec::new();
        for hardware in &self.hardware {
            walk(hardware, &mut out);
        }
        out
    }

    /// Number of hardware items including sub-hardware.
    pub fn hardware_count(&self) -> usize {
        self.all_hardware().len()
    }

    /// Number of sensors across all hardware items.
    pub fn sensor_count(&self) -> usize {
        self.all_hardware()
            .iter()
            .map(|hardware| hardware.sensors().count())
            .sum()
    }

    /// Look up a hardware item by exact id, including sub-hardware.
    pub fn hardware(&self, id: &str) -> Option<&Hardware> {
        self.all_hardware()
            .into_iter()
            .find(|hardware| hardware.id == id)
    }
}

/// Hardware classification, derived from the icon and the id prefix
/// because `data.json` has no hardware-type field (SPEC Q5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HardwareKind {
    Cpu,
    GpuNvidia,
    GpuAmd,
    GpuIntel,
    Memory,
    Storage,
    Network,
    Motherboard,
    SuperIO,
    Cooler,
    EmbeddedController,
    Psu,
    Battery,
    PowerMonitor,
    Other,
}

impl HardwareKind {
    pub fn is_gpu(self) -> bool {
        matches!(
            self,
            HardwareKind::GpuNvidia | HardwareKind::GpuAmd | HardwareKind::GpuIntel
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HardwareKind::Cpu => "cpu",
            HardwareKind::GpuNvidia => "nvidia-gpu",
            HardwareKind::GpuAmd => "amd-gpu",
            HardwareKind::GpuIntel => "intel-gpu",
            HardwareKind::Memory => "memory",
            HardwareKind::Storage => "storage",
            HardwareKind::Network => "network",
            HardwareKind::Motherboard => "motherboard",
            HardwareKind::SuperIO => "superio",
            HardwareKind::Cooler => "cooler",
            HardwareKind::EmbeddedController => "embedded-controller",
            HardwareKind::Psu => "psu",
            HardwareKind::Battery => "battery",
            HardwareKind::PowerMonitor => "power-monitor",
            HardwareKind::Other => "other",
        }
    }
}

impl fmt::Display for HardwareKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sensor category as sent in `Type` (SPEC Appendix B). Unknown values are
/// preserved so future LHM sensors still render (SPEC Q6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensorKind {
    Voltage,
    Current,
    Power,
    Clock,
    Temperature,
    Load,
    Frequency,
    Fan,
    Flow,
    Control,
    Level,
    Factor,
    Data,
    SmallData,
    Throughput,
    TimeSpan,
    Timing,
    Energy,
    Noise,
    Conductivity,
    Humidity,
    Other(String),
}

impl SensorKind {
    pub fn from_wire(name: &str) -> Self {
        match name {
            "Voltage" => SensorKind::Voltage,
            "Current" => SensorKind::Current,
            "Power" => SensorKind::Power,
            "Clock" => SensorKind::Clock,
            "Temperature" => SensorKind::Temperature,
            "Load" => SensorKind::Load,
            "Frequency" => SensorKind::Frequency,
            "Fan" => SensorKind::Fan,
            "Flow" => SensorKind::Flow,
            "Control" => SensorKind::Control,
            "Level" => SensorKind::Level,
            "Factor" => SensorKind::Factor,
            "Data" => SensorKind::Data,
            "SmallData" => SensorKind::SmallData,
            "Throughput" => SensorKind::Throughput,
            "TimeSpan" => SensorKind::TimeSpan,
            "Timing" => SensorKind::Timing,
            "Energy" => SensorKind::Energy,
            "Noise" => SensorKind::Noise,
            "Conductivity" => SensorKind::Conductivity,
            "Humidity" => SensorKind::Humidity,
            other => SensorKind::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            SensorKind::Voltage => "Voltage",
            SensorKind::Current => "Current",
            SensorKind::Power => "Power",
            SensorKind::Clock => "Clock",
            SensorKind::Temperature => "Temperature",
            SensorKind::Load => "Load",
            SensorKind::Frequency => "Frequency",
            SensorKind::Fan => "Fan",
            SensorKind::Flow => "Flow",
            SensorKind::Control => "Control",
            SensorKind::Level => "Level",
            SensorKind::Factor => "Factor",
            SensorKind::Data => "Data",
            SensorKind::SmallData => "SmallData",
            SensorKind::Throughput => "Throughput",
            SensorKind::TimeSpan => "TimeSpan",
            SensorKind::Timing => "Timing",
            SensorKind::Energy => "Energy",
            SensorKind::Noise => "Noise",
            SensorKind::Conductivity => "Conductivity",
            SensorKind::Humidity => "Humidity",
            SensorKind::Other(name) => name,
        }
    }
}

impl fmt::Display for SensorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Compare an LHM version string (`major.minor.build`, SPEC Q7) against a
/// minimum. Unparseable versions count as "at least" so we never warn falsely.
pub fn lhm_version_at_least(version: &str, required: (u32, u32, u32)) -> bool {
    let numeric = |part: &str| -> Option<u32> {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };
    let mut parts = version.split('.').map(numeric);
    let major = parts.next().flatten();
    let minor = parts.next().flatten();
    let build = parts.next().flatten().unwrap_or(0);
    match (major, minor) {
        (Some(major), Some(minor)) => (major, minor, build) >= required,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(lhm_version_at_least("0.9.6", (0, 9, 5)));
        assert!(lhm_version_at_least("0.9.5", (0, 9, 5)));
        assert!(!lhm_version_at_least("0.9.4", (0, 9, 5)));
        assert!(!lhm_version_at_least("0.8.9", (0, 9, 5)));
        assert!(lhm_version_at_least("1.0.0", (0, 9, 5)));
        assert!(!lhm_version_at_least("0.9", (0, 9, 5)), "0.9 == 0.9.0");
        assert!(lhm_version_at_least("0.9", (0, 9, 0)));
        assert!(
            lhm_version_at_least("unknown", (0, 9, 5)),
            "unknown never warns"
        );
    }

    #[test]
    fn sensor_kind_roundtrip() {
        assert_eq!(
            SensorKind::from_wire("Temperature"),
            SensorKind::Temperature
        );
        assert_eq!(SensorKind::from_wire("SmallData"), SensorKind::SmallData);
        assert_eq!(
            SensorKind::from_wire("Whatever"),
            SensorKind::Other("Whatever".to_string())
        );
        assert_eq!(SensorKind::Other("X".into()).as_str(), "X");
    }
}
