//! T5.5: `--dump` output is a stable, golden-tested machine surface.

mod support;

use std::time::{Duration, SystemTime};

use lhm_tui::lhm::model::dump_json;
use lhm_tui::lhm::parse::parse_payload;

#[test]
fn dump_json_is_stable() {
    let mut snapshot = parse_payload(&support::fixture("lhm-0.9.6.json")).expect("parses");
    // Pin the timestamp so the goldens are deterministic.
    snapshot.fetched_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let json = dump_json(&snapshot).expect("dump");
    insta::assert_snapshot!(json);
}

#[test]
fn dump_of_nan_fixture_has_no_nan_values() {
    let mut snapshot = parse_payload(&support::fixture("lhm-nan.json")).expect("parses");
    snapshot.fetched_at = SystemTime::UNIX_EPOCH;
    let json = dump_json(&snapshot).expect("dump");
    // The fixture contains a sensor *named* "NaN test sensor" (fine); what
    // must never appear is a bare NaN/Infinity *value*.
    assert!(!json.contains(": NaN"), "no NaN values: {json}");
    assert!(!json.contains(": Infinity") && !json.contains(": -Infinity"));
    // Strongest check: the dump is strict JSON and round-trips.
    serde_json::from_str::<serde_json::Value>(&json).expect("dump is strict JSON");
}
