#![forbid(unsafe_code)]

//! lhm-tui — terminal dashboard for LibreHardwareMonitor.
//!
//! Read-only monitoring TUI that reads LibreHardwareMonitor through its web
//! server (`GET /data.json`) or, on Windows, directly through
//! `LibreHardwareMonitorLib.dll` (SPEC §3.6). See `SPEC.md` for the design and
//! `PLAN.md` for the implementation plan.
//!
//! This library crate exists so integration tests (`tests/`) can exercise the
//! same code the binary runs; `main.rs` is a thin entrypoint on top of it.

pub mod app;
pub mod bindings;
pub mod charts;
pub mod cli;
pub mod config;
pub mod fan_guard;
pub mod history;
pub mod lhm;
pub mod theme;
pub mod ui;
pub mod util;
