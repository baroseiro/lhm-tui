//! T4.5: the shipped example must satisfy the shipped JSON Schema, and the
//! schema must genuinely reject broken configurations.

use std::path::PathBuf;

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()))
}

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_str(&read("schema/lhm-tui.schema.json")).expect("schema parses");
    jsonschema::validator_for(&schema).expect("schema compiles")
}

fn errors(validator: &jsonschema::Validator, instance: &serde_json::Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect()
}

#[test]
fn example_config_matches_schema() {
    let example: serde_json::Value =
        serde_json::from_str(&read("lhm-tui.example.json")).expect("example parses");
    let validator = validator();
    let failures = errors(&validator, &example);
    assert!(
        failures.is_empty(),
        "example violates the schema:\n{}",
        failures.join("\n")
    );
}

#[test]
fn full_config_with_custom_panels_matches_schema() {
    // The documented shape for every major section, exercised in one instance.
    let instance = serde_json::json!({
        "$schema": "./schema/lhm-tui.schema.json",
        "server": { "url": "http://127.0.0.1:8085", "username": "admin", "password": null,
                    "poll_interval_ms": 1000, "retry": { "factor": 2.0 } },
        "history": { "window_s": 900 },
        "ui": { "layout": ["cpu", "gpu", "memory", "custom:nvme"], "auto_height": true },
        "charts": { "enabled": true, "marker": "halfblock", "y_min": null, "y_max": null },
        "theme": { "name": "nord", "thresholds": { "temperature": { "warn": 80, "crit": 95 } } },
        "panels": {
            "cpu": { "show": ["temp", "load"] },
            "gpu": { "include": ["GpuNvidia"], "per_row": 2, "show": ["load", "memory", "temp"] },
            "thermal": { "enabled": true },
            "sensor_overrides": {
                "cpu.temp": { "sensor": "/amdcpu/0/temperature/2" },
                "gpu.0.load": { "match": { "hardware": "GpuNvidia", "type": "Load", "name": "GPU Core" } }
            },
            "custom": [{
                "id": "nvme",
                "title": "NVMe",
                "widgets": [
                    { "kind": "graph", "sensor": "/nvme/0/temperature/0", "label": "Temp", "unit": "°C" },
                    { "kind": "gauge", "match": { "type": "Load" }, "y_mode": "percent" },
                    { "kind": "table", "filter": { "hardware": "/nvme/0", "types": ["Temperature"] } }
                ]
            }]
        },
        "keys": { "quit": ["q", "ctrl+c"], "toggle_charts": ["c"] }
    });
    let validator = validator();
    let failures = errors(&validator, &instance);
    assert!(
        failures.is_empty(),
        "documented config shape rejected:\n{}",
        failures.join("\n")
    );
}

#[test]
fn schema_rejects_broken_configs() {
    let validator = validator();

    let bad_type = serde_json::json!({ "server": { "poll_interval_ms": "soon" } });
    assert!(
        !errors(&validator, &bad_type).is_empty(),
        "string where int expected"
    );

    let unknown_key = serde_json::json!({ "server": { "pol_interval_ms": 1000 } });
    assert!(
        !errors(&validator, &unknown_key).is_empty(),
        "typo'd key rejected"
    );

    let bad_enum = serde_json::json!({ "charts": { "marker": "brialle" } });
    assert!(
        !errors(&validator, &bad_enum).is_empty(),
        "unknown marker rejected"
    );

    let bad_slot = serde_json::json!({ "panels": { "cpu": { "show": ["temperatures"] } } });
    assert!(
        !errors(&validator, &bad_slot).is_empty(),
        "unknown show slot rejected"
    );

    let widget_without_kind = serde_json::json!({
        "panels": { "custom": [{ "id": "x", "widgets": [{ "sensor": "/a/b/0" }] }] }
    });
    assert!(
        !errors(&validator, &widget_without_kind).is_empty(),
        "widget kind required"
    );

    let good = serde_json::json!({ "server": { "poll_interval_ms": 1000 } });
    assert!(errors(&validator, &good).is_empty(), "valid subset passes");
}
