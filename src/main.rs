#![forbid(unsafe_code)]

//! lhm-tui — terminal dashboard for LibreHardwareMonitor.
//!
//! Read-only monitoring TUI that polls LHM's built-in web server
//! (`GET /data.json`). See `SPEC.md` for the design and `PLAN.md` for the
//! implementation plan (this build covers phase P0).

mod app;
mod cli;
mod config;
mod ui;
mod util;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let log_path = util::resolve_log_path(cli.log.as_deref());
    match util::init_logging(&log_path, cli.verbose) {
        Ok(()) => tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            log = %log_path.display(),
            "lhm-tui starting"
        ),
        Err(err) => eprintln!(
            "warning: file logging disabled ({}): {err}",
            log_path.display()
        ),
    }

    if let Some(target) = cli.init_config.clone() {
        let path = target.unwrap_or_else(config::default_config_path);
        return match config::write_default_config(&path) {
            Ok(()) => {
                tracing::info!(path = %path.display(), "wrote default config");
                println!("wrote default config to {}", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err),
        };
    }

    let loaded = match config::load(&cli) {
        Ok(loaded) => loaded,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
        tracing::warn!("{warning}");
    }
    tracing::info!(source = %loaded.source, "configuration loaded");

    if cli.print_config {
        return match config::redacted_json(&loaded.config) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err),
        };
    }

    if let Some((mode, milestone)) = unimplemented_mode(&cli) {
        eprintln!("lhm-tui: `{mode}` is not implemented yet (planned in {milestone}; see PLAN.md)");
        return ExitCode::FAILURE;
    }

    match app::run(&loaded.config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(&err),
    }
}

fn unimplemented_mode(cli: &Cli) -> Option<(&'static str, &'static str)> {
    if cli.probe {
        return Some(("--probe", "milestone M1"));
    }
    if cli.once {
        return Some(("--once", "milestone M5"));
    }
    if cli.dump.is_some() {
        return Some(("--dump", "milestone M5"));
    }
    None
}

fn fail(err: &anyhow::Error) -> ExitCode {
    eprintln!("error: {err:#}");
    ExitCode::FAILURE
}
