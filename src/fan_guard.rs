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
    /// `Some(true)`: every bound control's software range reaches 0 %, so the
    /// card can park its fans (zero-RPM / auto-stop). `Some(false)`: a control
    /// bottoms out above 0 % — the driver curve never stops the fans (most AMD
    /// cards), so the floor must leave such a card alone. `None`: no data.
    pub can_park: Option<bool>,
    /// Any bound fan reads at/near standstill right now (zero-RPM evidence for
    /// cards without a control-range statement).
    pub parked: bool,
}

/// Fan speed at or below which a fan counts as parked (tach noise margin).
pub const PARKED_RPM: f64 = 50.0;
/// Software control minimum at or below which the range counts as reaching 0 %.
const CONTROL_MIN_ZERO: f64 = 0.5;

impl GpuFanView {
    /// Does this card use the auto-stop (zero-RPM) behaviour the guard exists
    /// to defeat? `seen` is the session-sticky evidence that the fans were
    /// observed parked at some point.
    ///
    /// A control range starting above 0 % is authoritative: the driver cannot
    /// stop the fans, so the card is never touched. Otherwise the card
    /// qualifies when its control range reaches 0 % or when stopped fans were
    /// ever observed (a floored fan spins, so evidence must be sticky).
    pub fn zero_rpm_capable(&self, seen: bool) -> bool {
        match self.can_park {
            Some(capable) => capable,
            None => self.parked || seen,
        }
    }
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
            let control_ids: Vec<String> = gpu
                .fans
                .iter()
                .filter_map(|row| row.control.as_ref().map(|slot| slot.sensor_id.clone()))
                .collect();
            // Zero-RPM capability: a control whose software range starts above
            // 0 % proves the driver can never stop the fans (`Some(false)`);
            // otherwise a range reaching 0 % marks the card capable.
            let control_mins: Vec<Option<f64>> = gpu
                .fans
                .iter()
                .filter_map(|row| row.control.as_ref())
                .map(|slot| {
                    crate::bindings::slot_sensor(snapshot, slot).and_then(|sensor| sensor.min)
                })
                .collect();
            let can_park = if control_mins.is_empty() || control_mins.iter().all(Option::is_none) {
                None
            } else if control_mins
                .iter()
                .flatten()
                .any(|min| *min > CONTROL_MIN_ZERO)
            {
                Some(false)
            } else if control_mins.iter().all(Option::is_some) {
                Some(true)
            } else {
                None
            };
            let parked = gpu
                .fans
                .iter()
                .filter_map(|row| crate::bindings::slot_sensor(snapshot, &row.fan))
                .filter_map(|sensor| sensor.value)
                .any(|rpm| rpm <= PARKED_RPM);
            GpuFanView {
                hardware_id: gpu.hardware_id.clone(),
                name: gpu.name.clone(),
                load: value(&gpu.load),
                max_temp: temps,
                control_ids,
                can_park,
                parked,
            }
        })
        .collect()
}

/// The decision table. Unknown load/temperature **releases** an engaged floor
/// (fail-safe: let the driver manage), and never engages a new one. Cards that
/// cannot park their fans (`eligible == false`, `only_auto_stop`) are never
/// touched; an engaged floor is released if the card turns out ineligible.
pub fn decide(
    view: &GpuFanView,
    engaged: bool,
    cfg: &FanGuardConfig,
    eligible: bool,
) -> GuardAction {
    if view.control_ids.is_empty() {
        return GuardAction::Hold;
    }
    if cfg.only_auto_stop && !eligible {
        return if engaged {
            GuardAction::Release
        } else {
            GuardAction::Hold
        };
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
    /// Session-sticky zero-RPM eligibility (see [`GpuFanView::zero_rpm_capable`]).
    pub eligible: bool,
    /// A "left to its driver curve" note was already surfaced.
    pub skip_noticed: bool,
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
            can_park: Some(true),
            parked: false,
        }
    }

    #[test]
    fn engages_when_idle() {
        let cfg = FanGuardConfig::default(); // 10 %, 65 °C
        assert_eq!(
            decide(&view(Some(0.0), Some(40.0)), false, &cfg, true),
            GuardAction::Engage
        );
        assert_eq!(
            decide(&view(Some(10.0), Some(65.0)), false, &cfg, true),
            GuardAction::Engage
        );
        assert_eq!(
            decide(&view(Some(11.0), Some(40.0)), false, &cfg, true),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(5.0), Some(66.0)), false, &cfg, true),
            GuardAction::Hold
        );
    }

    #[test]
    fn holds_within_hysteresis_and_releases_when_hot() {
        let cfg = FanGuardConfig::default(); // release above 20 % load / 70 °C
        assert_eq!(
            decide(&view(Some(15.0), Some(65.0)), true, &cfg, true),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(20.0), Some(70.0)), true, &cfg, true),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(21.0), Some(65.0)), true, &cfg, true),
            GuardAction::Release
        );
        assert_eq!(
            decide(&view(Some(5.0), Some(71.0)), true, &cfg, true),
            GuardAction::Release
        );
    }

    #[test]
    fn unknown_data_is_fail_safe() {
        let cfg = FanGuardConfig::default();
        assert_eq!(
            decide(&view(None, Some(40.0)), false, &cfg, true),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(Some(0.0), None), false, &cfg, true),
            GuardAction::Hold
        );
        assert_eq!(
            decide(&view(None, None), true, &cfg, true),
            GuardAction::Release
        );
    }

    #[test]
    fn no_controllable_fans_is_a_noop() {
        let cfg = FanGuardConfig::default();
        let mut no_controls = view(Some(0.0), Some(30.0));
        no_controls.control_ids.clear();
        assert_eq!(decide(&no_controls, false, &cfg, true), GuardAction::Hold);
        assert_eq!(decide(&no_controls, true, &cfg, true), GuardAction::Hold);
    }

    #[test]
    fn zero_rpm_capability() {
        // Control range reaching 0 % → capable regardless of current RPM.
        let mut capable = view(None, None);
        capable.can_park = Some(true);
        assert!(capable.zero_rpm_capable(false));

        // A range bottoming out above 0 % is authoritative: never touch it,
        // even if a fan reads 0 (stuck tach, dead fan).
        let mut spinners = view(None, None);
        spinners.can_park = Some(false);
        spinners.parked = true;
        assert!(
            !spinners.zero_rpm_capable(true),
            "min 15 % card stays alone"
        );

        // No control-range statement: parked fans are the evidence, and it is
        // sticky (a floored fan spins, so the observation must be remembered).
        let mut unknown = view(None, None);
        unknown.can_park = None;
        unknown.parked = true;
        assert!(unknown.zero_rpm_capable(false));
        unknown.parked = false;
        assert!(unknown.zero_rpm_capable(true), "sticky evidence");
        assert!(
            !unknown.zero_rpm_capable(false),
            "never parked, no evidence"
        );
    }

    #[test]
    fn ineligible_cards_are_left_alone() {
        let cfg = FanGuardConfig::default(); // only_auto_stop: true
        let idle = view(Some(0.0), Some(40.0));
        assert_eq!(decide(&idle, false, &cfg, false), GuardAction::Hold);
        assert_eq!(decide(&idle, true, &cfg, false), GuardAction::Release);

        // `only_auto_stop: false` restores the old floor-every-card behaviour.
        let all = FanGuardConfig {
            only_auto_stop: false,
            ..FanGuardConfig::default()
        };
        assert_eq!(decide(&idle, false, &all, false), GuardAction::Engage);
    }
}
