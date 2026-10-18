//! GPU fan anti-stop guard decisions (T5.8).
//!
//! Pure logic only: gather per-GPU views from a snapshot + bindings and
//! decide whether the software fan floor should engage, release, or hold.
//! All network writes live in `app.rs` (control task) so this module stays
//! trivially testable.

use crate::bindings::Bindings;
use crate::config::FanGuardConfig;
use crate::lhm::model::Snapshot;

/// Everything the guard needs about one GPU.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuFanView {
    pub hardware_id: String,
    pub name: String,
    /// `None` when load is unknown — treated fail-safe (not idle).
    pub load: Option<f64>,
    /// Hottest of core / hot spot / memory-junction temperatures.
    pub max_temp: Option<f64>,
    /// Writable `Control` sensor ids for this GPU's fans.
    pub control_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardAction {
    /// Go to the software floor (fan never stops).
    Engage,
    /// Hand control back to the driver/automatic.
    Release,
    /// Keep the current mode.
    Hold,
}

/// One view per bound GPU (SPEC §5.2 slots: load / temp / hotspot / mem_temp).
pub fn views(snapshot: &Snapshot, bindings: &Bindings) -> Vec<GpuFanView> {
    bindings
        .gpus
        .iter()
        .map(|gpu| {
            let value = |slot: &Option<crate::bindings::Slot>| {
                slot.as_ref()
                    .and_then(|slot| crate::bindings::slot_sensor(snapshot, slot))
                    .and_then(|sensor| sensor.value)
            };
            let temps = [&gpu.temp, &gpu.hotspot, &gpu.mem_temp]
                .into_iter()
                .filter_map(value)
                .fold(None::<f64>, |acc, temp| {
                    Some(acc.map_or(temp, |best| best.max(temp)))
                });
            let control_ids = gpu
                .fans
                .iter()
                .filter_map(|row| row.control.as_ref().map(|slot| slot.sensor_id.clone()))
                .collect();
            GpuFanView {
                hardware_id: gpu.hardware_id.clone(),
                name: gpu.name.clone(),
                load: value(&gpu.load),
                max_temp: temps,
                control_ids,
            }
        })
        .collect()
}

/// The decision table. Unknown load/temperature **releases** an engaged floor
/// (fail-safe: let the driver manage), and never engages a new one.
pub fn decide(view: &GpuFanView, engaged: bool, cfg: &FanGuardConfig) -> GuardAction {
    if view.control_ids.is_empty() {
        return GuardAction::Hold;
    }
    let (Some(load), Some(temp)) = (view.load, view.max_temp) else {
        return if engaged {
            GuardAction::Release
        } else {
            GuardAction::Hold
        };
    };
    if engaged {
        let load_hot = load > f64::from(cfg.idle_load_percent + cfg.release_load_margin_percent);
        let temp_hot = temp > f64::from(cfg.idle_temp_c + cfg.release_temp_margin_c);
        if load_hot || temp_hot {
            GuardAction::Release
        } else {
            GuardAction::Hold
        }
    } else if load <= f64::from(cfg.idle_load_percent) && temp <= f64::from(cfg.idle_temp_c) {
        GuardAction::Engage
    } else {
        GuardAction::Hold
    }
}

/// Guard tracking state for one GPU.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuFanState {
    /// True while the software floor is applied.
    pub engaged: bool,
    /// Control sensor ids currently (or last) targeted.
    pub control_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(load: Option<f64>, temp: Option<f64>) -> GpuFanView {
        GpuFanView {
            hardware_id: "/gpu-nvidia/0".to_string(),
            name: "Dev GPU".to_string(),
            load,
            max_temp: temp,
            control_ids: vec!["/gpu-nvidia/0/control/0".to_string()],
        }
    }

    #[test]
    fn engages_when_idle() {
        let cfg = FanGuardConfig::default(); // 10 %, 65 °C
        assert_eq!(
            decide(&view(Some(0.0), Some(40.0)), false, &cfg),
            GuardAction::Engage
        );
        assert_eq!(
            decide(&view(Some(10.0), Some(65.0)), false, &cfg),
            GuardAction::Engage
        );
        assert_eq!(
            decide(&view(Some(11.0), Some(40.0)), false, &cfg),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(5.0), Some(66.0)), false, &cfg),
            GuardAction::Hold
        );
    }

    #[test]
    fn holds_within_hysteresis_and_releases_when_hot() {
        let cfg = FanGuardConfig::default(); // release above 20 % load / 70 °C
        assert_eq!(
            decide(&view(Some(15.0), Some(65.0)), true, &cfg),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(20.0), Some(70.0)), true, &cfg),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(21.0), Some(65.0)), true, &cfg),
            GuardAction::Release
        );
        assert_eq!(
            decide(&view(Some(5.0), Some(71.0)), true, &cfg),
            GuardAction::Release
        );
    }

    #[test]
    fn unknown_data_is_fail_safe() {
        let cfg = FanGuardConfig::default();
        assert_eq!(
            decide(&view(None, Some(40.0)), false, &cfg),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(0.0), None), false, &cfg),
            GuardAction::Hold
        );
        assert_eq!(decide(&view(None, None), true, &cfg), GuardAction::Release);
    }

    #[test]
    fn no_controllable_fans_is_a_noop() {
        let cfg = FanGuardConfig::default();
        let mut no_controls = view(Some(0.0), Some(30.0));
        no_controls.control_ids.clear();
        assert_eq!(decide(&no_controls, false, &cfg), GuardAction::Hold);
        assert_eq!(decide(&no_controls, true, &cfg), GuardAction::Hold);
    }
}
