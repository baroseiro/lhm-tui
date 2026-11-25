//! LibreHardwareMonitor integration (SPEC §3):
//! raw wire types, tolerant parsing, normalized model, the HTTP client and
//! the Windows DLL bridge.

pub mod client;
pub mod dll;
pub mod model;
pub mod parse;
pub mod raw;
