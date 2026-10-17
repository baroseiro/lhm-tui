//! Per-sensor rolling history (SPEC §7.2).
//!
//! One ring buffer per `SensorId`, appended once per applied snapshot;
//! sensors missing from a poll append a gap so lines break instead of
//! drawing straight through (gap-preserving, per SPEC §7.1 step 1).

use std::collections::{BTreeMap, HashSet, VecDeque};

use crate::lhm::model::Snapshot;

#[derive(Debug)]
pub struct History {
    capacity: usize,
    series: BTreeMap<String, VecDeque<Option<f32>>>,
}

impl History {
    /// Capacity per series: `window_s × 1000 / poll_interval_ms`, guarded by
    /// `max_points` (SPEC §7.2). `f32` halves the memory footprint.
    pub fn new(window_s: u64, poll_interval_ms: u64, max_points: usize) -> Self {
        let points = window_s.saturating_mul(1000) / poll_interval_ms.max(1);
        let capacity = (points as usize).clamp(2, max_points.max(2));
        Self {
            capacity,
            series: BTreeMap::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of tracked series.
    pub fn series_count(&self) -> usize {
        self.series.len()
    }

    /// Append one poll: every present sensor pushes its value (or `None` when
    /// it has none), every missing sensor pushes a gap.
    pub fn append_snapshot(&mut self, snapshot: &Snapshot) {
        let mut seen: HashSet<&str> = HashSet::new();
        for hardware in snapshot.all_hardware() {
            for sensor in hardware.sensors() {
                let entry = self.series.entry(sensor.id.clone()).or_default();
                entry.push_back(sensor.value.map(|value| value as f32));
                if entry.len() > self.capacity {
                    entry.pop_front();
                }
                seen.insert(sensor.id.as_str());
            }
        }
        for (id, entry) in self.series.iter_mut() {
            if !seen.contains(id.as_str()) {
                entry.push_back(None);
                if entry.len() > self.capacity {
                    entry.pop_front();
                }
            }
        }
    }

    /// Last `points` values (oldest → newest) for a sensor; empty when the
    /// sensor was never seen.
    pub fn window(&self, sensor_id: &str, points: usize) -> Vec<Option<f32>> {
        match self.series.get(sensor_id) {
            None => Vec::new(),
            Some(deque) => {
                let take = points.min(deque.len());
                deque.iter().skip(deque.len() - take).copied().collect()
            }
        }
    }

    /// `(min, max, avg)` over the last `points` values, ignoring gaps
    /// (SPEC §7.3).
    pub fn stats(&self, sensor_id: &str, points: usize) -> Option<(f64, f64, f64)> {
        let window = self.window(sensor_id, points);
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        let mut sum = 0.0;
        let mut count = 0u32;
        for value in window.iter().flatten() {
            let value = f64::from(*value);
            min = min.min(value);
            max = max.max(value);
            sum += value;
            count += 1;
        }
        (count > 0).then(|| (min, max, sum / f64::from(count)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lhm::model::{Hardware, HardwareKind, Sensor, SensorGroup, SensorKind, Snapshot};
    use std::time::SystemTime;

    fn snapshot_with(values: &[(&str, Option<f64>)]) -> Snapshot {
        let sensors: Vec<Sensor> = values
            .iter()
            .map(|(id, value)| Sensor {
                id: (*id).to_string(),
                name: (*id).to_string(),
                kind: SensorKind::Load,
                value: *value,
                value_text: String::new(),
                min: None,
                max: None,
            })
            .collect();
        Snapshot {
            fetched_at: SystemTime::now(),
            lhm_version: None,
            hardware: vec![Hardware {
                id: "/amdcpu/0".to_string(),
                name: "dev".to_string(),
                kind: HardwareKind::Cpu,
                groups: vec![SensorGroup {
                    name: "Load".to_string(),
                    sensors,
                }],
                sub: Vec::new(),
            }],
            skipped_nodes: 0,
        }
    }

    #[test]
    fn capacity_math_matches_spec() {
        let history = History::new(900, 1000, 3600);
        assert_eq!(history.capacity(), 900);
        let history = History::new(900, 250, 3600);
        assert_eq!(history.capacity(), 3600);
        let history = History::new(86_400, 250, 3600);
        assert_eq!(history.capacity(), 3600, "max_points guards");
        let history = History::new(60, 60_000, 3600);
        assert_eq!(history.capacity(), 2, "clamped minimum");
    }

    #[test]
    fn appends_and_wraps() {
        let mut history = History::new(2, 1000, 10); // capacity 2
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(10.0))]));
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(20.0))]));
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(30.0))]));
        assert_eq!(
            history.window("/amdcpu/0/load/0", 10),
            vec![Some(20.0), Some(30.0)]
        );
    }

    #[test]
    fn missing_sensors_become_gaps() {
        let mut history = History::new(10, 1000, 10);
        history.append_snapshot(&snapshot_with(&[
            ("/amdcpu/0/load/0", Some(1.0)),
            ("/amdcpu/0/load/1", Some(2.0)),
        ]));
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(3.0))]));
        assert_eq!(
            history.window("/amdcpu/0/load/0", 10),
            vec![Some(1.0), Some(3.0)]
        );
        assert_eq!(
            history.window("/amdcpu/0/load/1", 10),
            vec![Some(2.0), None],
            "sensor disappeared → gap"
        );
    }

    #[test]
    fn window_slices_most_recent_points() {
        let mut history = History::new(10, 1000, 10);
        for value in 0..6 {
            history.append_snapshot(&snapshot_with(&[(
                "/amdcpu/0/load/0",
                Some(f64::from(value)),
            )]));
        }
        assert_eq!(
            history.window("/amdcpu/0/load/0", 3),
            vec![Some(3.0), Some(4.0), Some(5.0)]
        );
        assert_eq!(history.window("/unknown", 3), Vec::<Option<f32>>::new());
    }

    #[test]
    fn stats_ignore_gaps() {
        let mut history = History::new(10, 1000, 10);
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(10.0))]));
        history.append_snapshot(&snapshot_with(&[]));
        history.append_snapshot(&snapshot_with(&[("/amdcpu/0/load/0", Some(30.0))]));
        let (min, max, avg) = history.stats("/amdcpu/0/load/0", 10).expect("stats");
        assert_eq!(min, 10.0);
        assert_eq!(max, 30.0);
        assert_eq!(avg, 20.0);
    }
}
