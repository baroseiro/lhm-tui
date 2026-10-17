# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `SPEC.md` — technical specification v1.0 for the read-only LibreHardwareMonitor TUI:
  LHM web API contract, architecture, config schema, chart engine, milestones.
- `PLAN.md` — phased implementation plan P0–P6 (tasks, exit checklists, risk burn-down)
  derived from `SPEC.md`.
- `README.md` — public project overview: quick start, configuration, keybindings, troubleshooting.
- `AGENTS.md` — repository instructions for AI agents, including mandatory `CHANGELOG.md` practice.
- `CHANGELOG.md` — change tracking for all notable changes (this file).
- Design reference recorded for the multi-GPU realtime charts: nvtop's per-GPU stacked metric
  graphs (SPEC §7.4), with a license-hygiene rule — nvtop is GPL-3.0-or-later, inspiration only.
- Crate scaffold: `Cargo.toml` (dependencies per SPEC §4.3), MIT `LICENSE`, `.gitignore` and
  GitHub Actions CI (fmt + clippy + test + release build on Windows/Linux/macOS) (`T0.1`).
- Full CLI surface per SPEC §9: config/URL/auth/poll/window/theme/ascii/layout/log flags and
  verbosity; `--probe`/`--once`/`--dump` exist as stubs that exit 1 with a milestone hint (`T0.2`).
- Configuration core: full JSON-schema structs with built-in defaults, discovery
  (`--config` → `LHM_TUI_CONFIG` → `./lhm-tui.json` → platform config dir), precedence
  (defaults < file < env < CLI), validation collecting all errors, unknown-key warnings with
  `--strict-config` escalation, `--print-config` (secrets redacted) and `--init-config` (`T0.3`).
- TUI shell: panic-safe terminal lifecycle, redraw-on-change event loop fed by a crossterm input
  thread, remap-aware quit keys, placeholder screen (`T0.4`).
- File logging: 10 MB × 3 size rotation (never stdout), `--log`, `-v`/`-vv` and `RUST_LOG`
  override (`T0.5`).
- `lhm-tui.example.json` generated from the built-in defaults and covered by tests (round-trip,
  no unknown keys, valid values).
- LHM client (P1): `/data.json` polling via `reqwest` (gzip, Basic auth, `User-Agent`, timeout)
  with the tolerant wire format — unit-suffixed strings, and bare `NaN`/`Infinity` sanitizing
  (`T1.1`/`T1.2`).
- Snapshot normalization: hardware classification (icon + id-prefix fallback incl. hyphenated
  `gpu-nvidia`/`gpu-amd` styles), machine-container descent, sensor grouping, stable `SensorId`
  identity, skipped-node counting (`T1.3`).
- `--probe` implemented: LHM version, payload size/latency, per-hardware sensor counts, exit 3
  (connection) / 4 (auth) per SPEC §9; verified against a live instance (9 hardware items,
  276 sensors, NVIDIA + AMD GPUs) (`T1.4`).
- Poller task with `watch` channel, exponential backoff + jitter reset on success, and
  `LinkStatus` transitions (Connecting/Live/Stale/Reconnecting) in the UI (`T1.5`).
- Test suite growth: 7-fixture version matrix (incl. the machine-node shape and NaN quirks), a
  scripted mock LHM server, poller recovery/backoff integration tests — 56 tests total (`T1.6`).
- `src/lib.rs` + thin `main.rs` split so integration tests exercise the same code the binary runs.
- Preset panels with live values (P2): CPU (temp/load gauges, per-core bars, clock, power), one
  panel per GPU (temp, hotspot, load, VRAM ratio, power/fan/clock), memory (used/total + virtual),
  plus sortable, scrollable tables for temperatures, fans (+control %), power (CPU+GPU total),
  storage and network (`T2.1`, `T2.5`, `T2.6`).
- Binding engine: SPEC §5.2 candidate heuristics over `SensorId`s, `panels.sensor_overrides`
  (exact pins, kind/name/name_regex matchers), fingerprint-based rebinding only when the hardware
  set changes (`T2.2`).
- Value formatting helpers (units, ratios, rates) with threshold-aware styling and a real theme
  implementation (color names/hex, warn/crit buckets, stale dimming) (`T2.3`).
- Widgets: gauge, core bars, sortable scrollable table, sparkline — locked in by `insta`
  snapshots (`T2.4`).
- Interaction: pause (`p`, freezes polling), focus cycling (`tab`/`shift+tab`), sort cycling
  (`s`), table scrolling (`j/k`/arrows), once-a-second stale/paused age ticker, and the
  terminal-too-small screen (SPEC N8).
- Real-world fixture `lhm-live-5900xt.json` (scrubbed capture of a 32-core dual-GPU machine) and
  ignored live tests: `cargo test --test live -- --ignored --nocapture` renders the full dashboard
  from a locally running LHM.

### Changed

- Declared MSRV corrected to Rust **1.89** (was 1.83): the resolved dependency tree
  (ratatui 0.30.2 / uuid 1.89) does not build on older toolchains.
- Spec/plan corrections aligned with the implementation: poll interval floor fixed at 250 ms
  everywhere (was 200 in §8.2 and Appendix C), `serde_ignored` added to the dependency table,
  input events documented as a crossterm input thread, T0.2 stub exit code corrected to 1.
- SPEC §3.3/§3.4 corrected from live-instance verification: hardware nodes are wrapped in a
  machine container node; `Version` is absent on 0.9.6-era servers; id prefixes include the
  hyphenated `gpu-nvidia`/`gpu-amd`/`gpu-intel` and `vram`/`memory` styles.
- Planned layout updated: the binding engine lives in `src/bindings.rs` (SPEC §4.1, PLAN T2.2);
  default `keys.focus_prev` also accepts `backtab` (legacy terminals report Shift+Tab that way)
  and `lhm-tui.example.json` was regenerated.
