//! Command-line interface (SPEC §9).

use std::path::PathBuf;

use clap::{ArgAction, Parser};

/// Terminal dashboard for LibreHardwareMonitor (read-only).
#[derive(Debug, Clone, Parser)]
#[command(
    name = "lhm-tui",
    version,
    about = "Terminal dashboard for LibreHardwareMonitor (read-only).",
    long_about = None
)]
pub struct Cli {
    /// Config file (JSON). [env: LHM_TUI_CONFIG]
    #[arg(short = 'c', long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// LibreHardwareMonitor base URL, e.g. http://127.0.0.1:8085. [env: LHM_TUI_URL]
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// HTTP Basic auth username. [env: LHM_TUI_USERNAME]
    #[arg(long, value_name = "USER")]
    pub username: Option<String>,

    /// HTTP Basic auth password. [env: LHM_TUI_PASSWORD]
    #[arg(long, value_name = "PASS")]
    pub password: Option<String>,

    /// Poll interval, e.g. 1s or 500ms.
    #[arg(long, value_name = "DURATION")]
    pub poll: Option<String>,

    /// Initial chart window, e.g. 5m or 30s.
    #[arg(long, value_name = "DURATION")]
    pub window: Option<String>,

    /// Built-in theme name or theme file. [env: LHM_TUI_THEME]
    #[arg(long, value_name = "NAME|PATH")]
    pub theme: Option<String>,

    /// ASCII-only rendering.
    #[arg(long)]
    pub ascii: bool,

    /// Disable colors (also honors the NO_COLOR environment variable).
    #[arg(long)]
    pub no_color: bool,

    /// Override ui.layout, e.g. "cpu,gpu,memory,fans".
    #[arg(long, value_name = "KEYS")]
    pub layout: Option<String>,

    /// Debug log file (default: platform temp/state dir).
    #[arg(long, value_name = "PATH")]
    pub log: Option<PathBuf>,

    /// Increase log verbosity (-v debug, -vv trace).
    #[arg(short = 'v', long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Check connectivity/auth, print a report, then exit.
    #[arg(long)]
    pub probe: bool,

    /// Render one snapshot to stdout, then exit.
    #[arg(long)]
    pub once: bool,

    /// Write the normalized snapshot JSON to PATH ("-" for stdout), then exit.
    #[arg(long, value_name = "PATH|-")]
    pub dump: Option<String>,

    /// Print the effective merged config (secrets redacted), then exit.
    #[arg(long)]
    pub print_config: bool,

    /// Write a default config file to PATH (default: platform config dir), then exit.
    #[arg(long, value_name = "PATH", num_args = 0..=1)]
    pub init_config: Option<Option<PathBuf>>,

    /// Treat unknown config keys as errors.
    #[arg(long)]
    pub strict_config: bool,
}
