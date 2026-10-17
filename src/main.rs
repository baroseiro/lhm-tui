#![forbid(unsafe_code)]

//! lhm-tui — thin binary entrypoint (the crate lives in `src/lib.rs`).

use std::process::ExitCode;

use clap::Parser;
use lhm_tui::app;
use lhm_tui::cli::Cli;
use lhm_tui::config::{self, Config};
use lhm_tui::lhm::client::{LhmClient, LhmError};
use lhm_tui::lhm::model;
use lhm_tui::util;

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

    if cli.probe {
        return run_probe(&loaded.config).await;
    }

    if let Some((mode, milestone)) = stub_mode(&cli) {
        eprintln!("lhm-tui: `{mode}` is not implemented yet (planned in {milestone}; see PLAN.md)");
        return ExitCode::FAILURE;
    }

    match app::run(&loaded.config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(&err),
    }
}

fn stub_mode(cli: &Cli) -> Option<(&'static str, &'static str)> {
    if cli.once {
        return Some(("--once", "milestone M5"));
    }
    if cli.dump.is_some() {
        return Some(("--dump", "milestone M5"));
    }
    None
}

/// `--probe`: connectivity/auth check with a report (SPEC §9). Exit codes:
/// 0 ok · 1 other failures · 3 connection failure · 4 auth failure.
async fn run_probe(config: &Config) -> ExitCode {
    let client = match LhmClient::new(&config.server) {
        Ok(client) => client,
        Err(err) => return fail(&err),
    };

    match client.fetch_data_json().await {
        Ok(fetched) => {
            let snapshot = &fetched.snapshot;
            let version = snapshot
                .lhm_version
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            println!("lhm-tui probe");
            println!("  url        {}", client.base_url());
            println!("  lhm        {version}");
            println!(
                "  payload    {} in {} ms",
                util::human_bytes(fetched.bytes),
                fetched.latency.as_millis()
            );
            println!(
                "  totals     {} hardware items · {} sensors",
                snapshot.hardware_count(),
                snapshot.sensor_count()
            );
            for hardware in snapshot.all_hardware() {
                println!(
                    "    - {:<22} {:<20} {} sensors",
                    hardware.id,
                    hardware.kind.to_string(),
                    hardware.sensors().count()
                );
            }
            if snapshot.skipped_nodes > 0 {
                println!("  skipped    {} unrecognized nodes", snapshot.skipped_nodes);
            }
            if config.server.verify_version && !model::lhm_version_at_least(&version, (0, 9, 5)) {
                println!(
                    "  warning    LHM {version} is older than 0.9.5 — some fields may be missing"
                );
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            match err {
                LhmError::Connect { .. } => ExitCode::from(3),
                LhmError::Auth { .. } => ExitCode::from(4),
                LhmError::Http { .. } | LhmError::BadPayload { .. } => ExitCode::FAILURE,
            }
        }
    }
}

fn fail(err: &anyhow::Error) -> ExitCode {
    eprintln!("error: {err:#}");
    ExitCode::FAILURE
}
