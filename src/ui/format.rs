//! Value formatting helpers (SPEC §6.5, N9): fixed decimals, unit-aware.
//!
//! Sensors with no numeric value render as `—` (SPEC Q4). Inputs are always
//! finite — the parser rejects `NaN`/`Infinity` — so formatting never leaks
//! `NaN` into the UI.

use crate::lhm::model::{Sensor, SensorKind};

/// `"69.5 °C"`
pub fn temperature(value: f64) -> String {
    format!("{value:.1} °C")
}

/// `"41.2 %"`
pub fn percent(value: f64) -> String {
    format!("{value:.1} %")
}

/// `"5.42 GHz"` / `"800 MHz"`
pub fn clock_mhz(mhz: f64) -> String {
    if mhz >= 1000.0 {
        format!("{:.2} GHz", mhz / 1000.0)
    } else {
        format!("{mhz:.0} MHz")
    }
}

/// `"3600 MHz"` - memory transfer rate; always MHz, never switched to GHz.
pub fn memory_speed_mhz(mhz: f64) -> String {
    format!("{mhz:.0} MHz")
}

/// `"1205 RPM"`
pub fn fan_rpm(rpm: f64) -> String {
    format!("{rpm:.0} RPM")
}

/// `"142 W"`
pub fn power_w(watts: f64) -> String {
    format!("{watts:.0} W")
}

/// Data sensors carry GB (SPEC Appendix B): `"20.1 GB"`.
pub fn data_gb(gb: f64) -> String {
    format!("{gb:.1} GB")
}

/// SmallData sensors carry MB: `"8619 MB"` or `"8.4 GB"` when large.
pub fn small_data_mb(mb: f64) -> String {
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// `"20.1 / 32.0 GB (63%)"` / `"8619 / 24564 MB (35%)"`: used / total in the
/// kind's native unit plus the used percentage (SPEC §6.5).
pub fn data_ratio(kind: &SensorKind, used: f64, total: f64) -> String {
    let percent = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };
    match kind {
        SensorKind::SmallData => format!("{used:.0} / {total:.0} MB ({percent:.0}%)"),
        _ => format!("{used:.1} / {total:.1} GB ({percent:.0}%)"),
    }
}

/// Throughput is bytes per second: `"12.3 MB/s"`, `"980 B/s"`, `"1.2 GB/s"`.
pub fn throughput_bps(bytes_per_sec: f64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes_per_sec;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{value:.0} {}", UNITS[unit]),
        _ => format!("{value:.1} {}", UNITS[unit]),
    }
}

/// `"1.32 V"`
pub fn voltage(volts: f64) -> String {
    format!("{volts:.2} V")
}

/// `"0.98 A"`
pub fn current(amps: f64) -> String {
    format!("{amps:.2} A")
}

/// Format a sensor's value by its kind; `None` → `—`.
pub fn sensor_value(sensor: &Sensor) -> String {
    match sensor.value {
        Some(value) => value_for(&sensor.kind, value),
        None => "—".to_string(),
    }
}

/// Format a bare value for a kind (shared by gauges and tables).
pub fn value_for(kind: &SensorKind, value: f64) -> String {
    match kind {
        SensorKind::Temperature => temperature(value),
        SensorKind::Load | SensorKind::Control | SensorKind::Level => percent(value),
        SensorKind::Clock => clock_mhz(value),
        SensorKind::Fan => fan_rpm(value),
        SensorKind::Power => power_w(value),
        SensorKind::Data => data_gb(value),
        SensorKind::SmallData => small_data_mb(value),
        SensorKind::Throughput => throughput_bps(value),
        SensorKind::Voltage => voltage(value),
        SensorKind::Current => current(value),
        _ => format!("{value:.1}"),
    }
}

/// Compact statistic for min/max table columns (no unit, fixed width).
pub fn stat(kind: &SensorKind, value: Option<f64>) -> String {
    match (kind, value) {
        (_, None) => "—".to_string(),
        (
            SensorKind::Temperature | SensorKind::Load | SensorKind::Control | SensorKind::Level,
            Some(v),
        ) => {
            format!("{v:.1}")
        }
        // Voltages need decimals to be meaningful (`1.16`, not `1`).
        (SensorKind::Voltage, Some(v)) => format!("{v:.2}"),
        (SensorKind::Current, Some(v)) => format!("{v:.1}"),
        (_, Some(v)) => format!("{v:.0}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor(kind: SensorKind, value: Option<f64>) -> Sensor {
        Sensor {
            id: "/x/0".to_string(),
            name: "x".to_string(),
            kind,
            value,
            value_text: String::new(),
            min: None,
            max: None,
        }
    }

    #[test]
    fn formats_match_spec_examples() {
        assert_eq!(temperature(69.5), "69.5 °C");
        assert_eq!(percent(41.2), "41.2 %");
        assert_eq!(clock_mhz(5420.0), "5.42 GHz");
        assert_eq!(clock_mhz(800.0), "800 MHz");
        assert_eq!(memory_speed_mhz(3600.0), "3600 MHz");
        assert_eq!(fan_rpm(1205.0), "1205 RPM");
        assert_eq!(power_w(142.0), "142 W");
        assert_eq!(data_gb(20.1), "20.1 GB");
        assert_eq!(small_data_mb(8619.9), "8.4 GB");
        assert_eq!(small_data_mb(512.0), "512 MB");
        assert_eq!(
            data_ratio(&SensorKind::Data, 20.1, 32.0),
            "20.1 / 32.0 GB (63%)"
        );
        assert_eq!(
            data_ratio(&SensorKind::SmallData, 8619.0, 24564.0),
            "8619 / 24564 MB (35%)"
        );
        assert_eq!(throughput_bps(12_900_000.0), "12.3 MB/s");
        assert_eq!(throughput_bps(980.0), "980 B/s");
        assert_eq!(throughput_bps(1_300_000_000.0), "1.2 GB/s");
        assert_eq!(voltage(1.319), "1.32 V");
        assert_eq!(current(0.98), "0.98 A");
    }

    #[test]
    fn metrics_route_by_sensor_kind() {
        assert_eq!(value_for(&SensorKind::Temperature, 46.9), "46.9 °C");
        assert_eq!(value_for(&SensorKind::Clock, 4275.0), "4.28 GHz");
        assert_eq!(value_for(&SensorKind::Fan, 1205.0), "1205 RPM");
        assert_eq!(value_for(&SensorKind::Other("Weird".into()), 12.34), "12.3");
    }

    #[test]
    fn missing_values_render_as_dash() {
        let s = sensor(SensorKind::Temperature, None);
        assert_eq!(sensor_value(&s), "—");
        let s = sensor(SensorKind::Temperature, Some(46.9));
        assert_eq!(sensor_value(&s), "46.9 °C");
    }

    #[test]
    fn stat_formatting() {
        assert_eq!(stat(&SensorKind::Temperature, Some(77.3)), "77.3");
        assert_eq!(stat(&SensorKind::Temperature, None), "—");
        assert_eq!(stat(&SensorKind::Fan, Some(1205.0)), "1205");
        assert_eq!(stat(&SensorKind::Voltage, Some(1.156)), "1.16");
        assert_eq!(stat(&SensorKind::Current, Some(41.02)), "41.0");
    }
}
