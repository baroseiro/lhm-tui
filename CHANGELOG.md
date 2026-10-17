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

### Changed

- Declared MSRV corrected to Rust **1.89** (was 1.83): the resolved dependency tree
  (ratatui 0.30.2 / uuid 1.89) does not build on older toolchains.
- Spec/plan corrections aligned with the implementation: poll interval floor fixed at 250 ms
  everywhere (was 200 in §8.2 and Appendix C), `serde_ignored` added to the dependency table,
  input events documented as a crossterm input thread, T0.2 stub exit code corrected to 1.
