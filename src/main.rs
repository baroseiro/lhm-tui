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
    if let Some(target) = cli.dump.clone() {
        return run_dump(&loaded.config, &target).await;
    }
    if cli.once {
        return run_once(&loaded.config).await;
    }

    match app::run(
        &loaded.config,
        app::SessionOptions {
            cli: cli.clone(),
            warnings: loaded.warnings.clone(),
        },
    )
    .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(&err),
    }
}

/// `--probe`: connectivity/auth check with a report (SPEC §9). Exit codes:
/// 0 ok · 1 other failures · 3 connection failure · 4 auth failure.
async fn run_probe(config: &Config) -> ExitCode {
    let fetched = match fetch_snapshot(config).await {
        Ok(fetched) => fetched,
        Err(code) => return code,
    };
    let snapshot = &fetched.snapshot;
    let version = snapshot
        .lhm_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    println!("lhm-tui probe");
    println!("  url        {}", config.server.url.trim_end_matches('/'));
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
        println!("  warning    LHM {version} is older than 0.9.5 — some fields may be missing");
    }
    ExitCode::SUCCESS
}

/// Shared fetch for one-shot modes with SPEC §9 exit-code mapping.
async fn fetch_snapshot(config: &Config) -> Result<lhm_tui::lhm::client::Fetched, ExitCode> {
    let client = match LhmClient::new(&config.server) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("error: {err:#}");
            return Err(ExitCode::FAILURE);
        }
    };
    client.fetch_data_json().await.map_err(|err| {
        eprintln!("error: {err}");
        match err {
            LhmError::Connect { .. } => ExitCode::from(3),
            LhmError::Auth { .. } => ExitCode::from(4),
            LhmError::Http { .. } | LhmError::BadPayload { .. } => ExitCode::FAILURE,
        }
    })
}

/// `--once`: render one frame to stdout without entering the alternate screen
/// (colours off when piped), then exit (T5.5).
async fn run_once(config: &Config) -> ExitCode {
    use std::io::IsTerminal;

    let fetched = match fetch_snapshot(config).await {
        Ok(fetched) => fetched,
        Err(code) => return code,
    };
    let mut cfg = config.clone();
    if !std::io::stdout().is_terminal() {
        cfg.ui.no_color = true;
    }
    let mut app = app::AppState::new();
    app.apply_poll_event(app::PollEvent::Snapshot(std::sync::Arc::new(fetched)), &cfg);

    let (width, height) = ratatui::crossterm::terminal::size().unwrap_or((120, 40));
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let options = ratatui::TerminalOptions {
        viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, width, height)),
    };
    let mut terminal = match ratatui::Terminal::with_options(backend, options) {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = terminal.draw(|frame| lhm_tui::ui::draw(frame, &mut app, &cfg)) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }
    let _ = terminal.show_cursor();
    println!();
    ExitCode::SUCCESS
}

/// `--dump <PATH|->`: write the normalized snapshot JSON (stable shape), exit
/// (T5.5; SPEC §9).
async fn run_dump(config: &Config, target: &str) -> ExitCode {
    let fetched = match fetch_snapshot(config).await {
        Ok(fetched) => fetched,
        Err(code) => return code,
    };
    let json = match model::dump_json(&fetched.snapshot) {
        Ok(json) => format!("{json}\n"),
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };
    if target == "-" {
        print!("{json}");
        return ExitCode::SUCCESS;
    }
    match std::fs::write(target, json) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: cannot write {target}: {err}");
            ExitCode::FAILURE
        }
    }
}

fn fail(err: &anyhow::Error) -> ExitCode {
    eprintln!("error: {err:#}");
    ExitCode::FAILURE
}
