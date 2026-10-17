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

### Changed

- Declared MSRV corrected to Rust **1.89** (was 1.83): the resolved dependency tree
  (ratatui 0.30.2 / uuid 1.89) does not build on older toolchains.
