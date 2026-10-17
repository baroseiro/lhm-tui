#![forbid(unsafe_code)]

//! lhm-tui — terminal dashboard for LibreHardwareMonitor.
//!
//! Read-only monitoring TUI that polls LHM's built-in web server
//! (`GET /data.json`). See `SPEC.md` for the design and `PLAN.md` for the
//! implementation plan.
//!
//! This library crate exists so integration tests (`tests/`) can exercise the
//! same code the binary runs; `main.rs` is a thin entrypoint on top of it.

pub mod app;
pub mod bindings;
pub mod cli;
pub mod config;
pub mod lhm;
pub mod theme;
pub mod ui;
pub mod util;
