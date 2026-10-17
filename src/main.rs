#![forbid(unsafe_code)]

//! lhm-tui — terminal dashboard for LibreHardwareMonitor (PLAN.md, phase P0).
//!
//! T0.1 bootstrap: prints the version banner. The CLI, config core, TUI shell
//! and logging land in the following commits of phase P0.

fn main() {
    println!("lhm-tui {} (bootstrap)", env!("CARGO_PKG_VERSION"));
}
