//! Configuration: schema, defaults, discovery, precedence and validation (SPEC §8).
//!
//! Precedence (later wins): built-in defaults → config file → environment →
//! CLI flags (SPEC §8.1). Validation collects *all* errors and never panics;
//! unknown keys are reported as warnings unless `--strict-config` is set.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

/// Lowest accepted `server.poll_interval_ms` (SPEC §8.3).
pub const MIN_POLL_INTERVAL_MS: u64 = 250;
/// Highest accepted `server.poll_interval_ms`.
pub const MAX_POLL_INTERVAL_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Optional JSON Schema pointer; accepted and ignored by the tool.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub server: ServerConfig,
    pub history: HistoryConfig,
    pub ui: UiConfig,
    pub charts: ChartsConfig,
    pub theme: ThemeConfig,
    pub panels: PanelsConfig,
    pub keys: BTreeMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: None,
            server: ServerConfig::default(),
            history: HistoryConfig::default(),
            ui: UiConfig::default(),
            charts: ChartsConfig::default(),
            theme: ThemeConfig::default(),
            panels: PanelsConfig::default(),
            keys: default_keys(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_env: Option<String>,
    pub password_file: Option<String>,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub gzip: bool,
    pub verify_version: bool,
    pub retry: RetryConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:8085".to_string(),
            username: None,
            password: None,
            password_env: None,
            password_file: None,
            poll_interval_ms: 1000,
            request_timeout_ms: 2000,
            gzip: true,
            verify_version: true,
            retry: RetryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetryConfig {
    pub initial_ms: u64,
    pub max_ms: u64,
    pub factor: f64,
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_ms: 500,
            max_ms: 30_000,
            factor: 2.0,
            jitter: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    pub window_s: u64,
    pub max_points_per_series: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            window_s: 900,
            max_points_per_series: 3600,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub fps: u32,
    pub layout: Vec<String>,
    pub show_header: bool,
    pub show_footer: bool,
    pub max_height: u32,
    pub table_sort: TableSort,
    pub ascii: bool,
    pub no_color: bool,
    pub pause_position: bool,
    /// Stretch panels (charts first, then tables up to `max_height`) to fill
    /// the terminal height instead of leaving blank space at the bottom.
    pub auto_height: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            fps: 30,
            layout: ["cpu", "gpu", "memory"].map(String::from).to_vec(),
            show_header: true,
            show_footer: true,
            max_height: 12,
            table_sort: TableSort::ValueDesc,
            ascii: false,
            no_color: false,
            pause_position: false,
            auto_height: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableSort {
    #[default]
    ValueDesc,
    ValueAsc,
    Name,
    #[serde(rename = "none")]
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartsConfig {
    /// Charts are optional: `false` (config or `--no-charts`) keeps the
    /// gauges/bars-only panel layout.
    pub enabled: bool,
    pub marker: Marker,
    pub height: u32,
    pub window: String,
    pub window_cycle: Vec<String>,
    pub y_mode: YMode,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub min_span: f64,
    pub grid: bool,
    pub legend: bool,
    pub blend_overlap: bool,
}

impl Default for ChartsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            marker: Marker::Braille,
            height: 8,
            window: "5m".to_string(),
            window_cycle: ["1m", "5m", "15m"].map(String::from).to_vec(),
            y_mode: YMode::Auto,
            y_min: None,
            y_max: None,
            min_span: 1.0,
            grid: true,
            legend: true,
            blend_overlap: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Marker {
    #[default]
    Braille,
    Halfblock,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YMode {
    #[default]
    Auto,
    Percent,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub accent: String,
    pub text: String,
    pub muted: String,
    pub warn: String,
    pub crit: String,
    pub cpu: String,
    pub gpu: String,
    pub memory: String,
    pub network: String,
    pub storage: String,
    pub thresholds: Thresholds,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            accent: "cyan".to_string(),
            text: "#c0caf5".to_string(),
            muted: "gray".to_string(),
            warn: "#e0af68".to_string(),
            crit: "#f7768e".to_string(),
            cpu: "#7aa2f7".to_string(),
            gpu: "#bb9af7".to_string(),
            memory: "#9ece6a".to_string(),
            network: "#7dcfff".to_string(),
            storage: "#e0af68".to_string(),
            thresholds: Thresholds::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    pub temperature: Threshold,
    pub load: Threshold,
    pub gpu_memory: Threshold,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            temperature: Threshold {
                warn: 80.0,
                crit: 95.0,
            },
            load: Threshold {
                warn: 90.0,
                crit: 98.0,
            },
            gpu_memory: Threshold {
                warn: 90.0,
                crit: 97.0,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Threshold {
    pub warn: f64,
    pub crit: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelsConfig {
    pub cpu: CpuPanel,
    pub gpu: GpuPanel,
    pub memory: MemoryPanel,
    pub thermal: ThermalPanel,
    pub fans: TogglePanel,
    pub power: TogglePanel,
    pub storage: TogglePanel,
    pub network: TogglePanel,
    pub sensor_overrides: BTreeMap<String, SensorOverride>,
    pub custom: Vec<CustomPanel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuPanel {
    pub enabled: bool,
    pub show: Vec<String>,
}

impl Default for CpuPanel {
    fn default() -> Self {
        Self {
            enabled: true,
            show: ["temp", "load", "core_loads", "clock", "power"]
                .map(String::from)
                .to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuPanel {
    pub enabled: bool,
    pub include: Vec<String>,
    pub per_row: u32,
    pub show: Vec<String>,
}

impl Default for GpuPanel {
    fn default() -> Self {
        Self {
            enabled: true,
            include: ["GpuNvidia", "GpuAmd", "GpuIntel"]
                .map(String::from)
                .to_vec(),
            per_row: 2,
            show: ["load", "memory", "temp", "power", "fan"]
                .map(String::from)
                .to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryPanel {
    pub enabled: bool,
    pub show: Vec<String>,
}

impl Default for MemoryPanel {
    fn default() -> Self {
        Self {
            enabled: true,
            show: ["used", "load", "virtual"].map(String::from).to_vec(),
        }
    }
}

/// Thermal table: enabled by default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThermalPanel {
    pub enabled: bool,
}

impl Default for ThermalPanel {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Optional preset (fans, power, storage, network): disabled by default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TogglePanel {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SensorOverride {
    /// Exact LHM SensorId, e.g. `/amdcpu/0/temperature/2`.
    pub sensor: Option<String>,
    /// Match rule, used when no exact sensor id is pinned.
    #[serde(rename = "match")]
    pub matcher: Option<SensorMatch>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SensorMatch {
    pub hardware: Option<String>,
    #[serde(rename = "type")]
    pub sensor_type: Option<String>,
    pub name: Option<String>,
    pub name_regex: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomPanel {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub widgets: Vec<CustomWidget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomWidget {
    pub kind: WidgetKind,
    pub sensor: Option<String>,
    pub label: Option<String>,
    pub unit: Option<String>,
    pub color: Option<String>,
    pub y_mode: Option<YMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Graph,
    Gauge,
    Bars,
    Table,
    Sparkline,
}

fn default_keys() -> BTreeMap<String, Vec<String>> {
    let entries: [(&str, &[&str]); 16] = [
        ("quit", &["q", "esc", "ctrl+c"]),
        ("help", &["?"]),
        ("pause", &["p", "space"]),
        ("window_next", &["g"]),
        ("window_prev", &["G"]),
        ("focus_next", &["tab"]),
        ("focus_prev", &["shift+tab", "backtab"]),
        ("zoom", &["enter"]),
        ("browser", &["b"]),
        ("sort", &["s"]),
        ("reload", &["r"]),
        ("reset_minmax", &["m"]),
        ("ascii", &["a"]),
        ("debug", &["d"]),
        ("toggle_thermal", &["t"]),
        ("toggle_charts", &["c"]),
    ];
    entries
        .iter()
        .map(|(action, specs)| {
            (
                action.to_string(),
                specs.iter().map(|s| s.to_string()).collect(),
            )
        })
        .collect()
}

/// Where the effective configuration came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Defaults,
    File(PathBuf),
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigSource::Defaults => f.write_str("built-in defaults"),
            ConfigSource::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// Result of [`load`]: the effective config plus warnings and its source.
#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub source: ConfigSource,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    #[error("cannot read config file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("invalid configuration:\n{}", join_bullets(.0))]
    Validation(Vec<String>),
}

fn join_bullets(errors: &[String]) -> String {
    errors
        .iter()
        .map(|error| format!("  - {error}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Default config file location in the platform config directory
/// (`%APPDATA%\lhm-tui\config.json` on Windows, `~/.config/lhm-tui/config.json` elsewhere).
pub fn default_config_path() -> PathBuf {
    ProjectDirs::from("", "", "lhm-tui")
        .map(|dirs| dirs.config_dir().join("config.json"))
        .unwrap_or_else(|| PathBuf::from("lhm-tui.json"))
}

/// Load, merge and validate the effective configuration (SPEC §8.1).
pub fn load(cli: &Cli) -> Result<LoadedConfig, ConfigLoadError> {
    let (mut config, source, ignored) = match resolve_config_path(cli)? {
        Some(path) => {
            let text = std::fs::read_to_string(&path).map_err(|source| ConfigLoadError::Io {
                path: path.clone(),
                source,
            })?;
            let (parsed, ignored) = parse_config_str(&text);
            let parsed = parsed.map_err(|err| ConfigLoadError::Parse {
                path: path.clone(),
                message: err.to_string(),
            })?;
            (parsed, ConfigSource::File(path), ignored)
        }
        None => (Config::default(), ConfigSource::Defaults, Vec::new()),
    };

    apply_env_overrides(&mut config, |name| std::env::var(name).ok());

    let mut errors = Vec::new();
    apply_cli_overrides(&mut config, cli, &mut errors);
    errors.extend(validate(&config));

    let mut warnings = Vec::new();
    for path in ignored {
        let key = path.trim_start_matches("$.").to_string();
        if cli.strict_config {
            errors.push(format!("unknown config key `{key}` (--strict-config)"));
        } else {
            warnings.push(format!(
                "unknown config key `{key}` (ignored — see --strict-config)"
            ));
        }
    }

    if errors.is_empty() {
        Ok(LoadedConfig {
            config,
            source,
            warnings,
        })
    } else {
        Err(ConfigLoadError::Validation(errors))
    }
}

fn resolve_config_path(cli: &Cli) -> Result<Option<PathBuf>, ConfigLoadError> {
    if let Some(path) = cli
        .config
        .clone()
        .or_else(|| std::env::var_os("LHM_TUI_CONFIG").map(PathBuf::from))
    {
        if path.exists() {
            return Ok(Some(path));
        }
        return Err(ConfigLoadError::NotFound(path));
    }

    let cwd = PathBuf::from("lhm-tui.json");
    if cwd.exists() {
        return Ok(Some(cwd));
    }

    let platform = default_config_path();
    if platform.exists() {
        return Ok(Some(platform));
    }

    Ok(None)
}

fn parse_config_str(text: &str) -> (Result<Config, serde_json::Error>, Vec<String>) {
    let mut ignored = Vec::new();
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let result = serde_ignored::deserialize(&mut deserializer, |path| {
        ignored.push(path.to_string());
    });
    (result, ignored)
}

/// Environment overrides (`LHM_TUI_*`), applied after the config file.
fn apply_env_overrides(config: &mut Config, lookup: impl Fn(&str) -> Option<String>) {
    if let Some(url) = lookup("LHM_TUI_URL") {
        config.server.url = url;
    }
    if let Some(username) = lookup("LHM_TUI_USERNAME") {
        config.server.username = Some(username);
    }
    if let Some(password) = lookup("LHM_TUI_PASSWORD") {
        // The env password takes precedence over other password sources.
        config.server.password = Some(password);
        config.server.password_env = None;
        config.server.password_file = None;
    }
    if let Some(theme) = lookup("LHM_TUI_THEME") {
        config.theme.name = theme;
    }
}

/// CLI overrides, applied last (highest precedence).
fn apply_cli_overrides(config: &mut Config, cli: &Cli, errors: &mut Vec<String>) {
    if let Some(url) = &cli.url {
        config.server.url = url.clone();
    }
    if let Some(username) = &cli.username {
        config.server.username = Some(username.clone());
    }
    if let Some(password) = &cli.password {
        config.server.password = Some(password.clone());
        config.server.password_env = None;
        config.server.password_file = None;
    }
    if let Some(poll) = &cli.poll {
        match parse_duration_ms(poll) {
            Ok(ms) => config.server.poll_interval_ms = ms,
            Err(message) => errors.push(format!("--poll: {message}")),
        }
    }
    if let Some(window) = &cli.window {
        match humantime::parse_duration(window) {
            Ok(_) => config.charts.window = window.clone(),
            Err(err) => errors.push(format!("--window: invalid duration `{window}` ({err})")),
        }
    }
    if let Some(theme) = &cli.theme {
        config.theme.name = theme.clone();
    }
    if cli.ascii {
        config.ui.ascii = true;
    }
    if cli.no_color {
        config.ui.no_color = true;
    }
    if let Some(layout) = &cli.layout {
        config.ui.layout = layout
            .split(',')
            .map(|entry| entry.trim().to_ascii_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect();
    }
    if cli.charts {
        config.charts.enabled = true;
    }
    if cli.no_charts {
        config.charts.enabled = false;
    }
}

/// Parse a duration for `--poll`: bare numbers are milliseconds, otherwise humantime.
fn parse_duration_ms(text: &str) -> Result<u64, String> {
    if let Ok(ms) = text.trim().parse::<u64>() {
        return Ok(ms);
    }
    humantime::parse_duration(text)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|err| format!("invalid duration `{text}` ({err})"))
}

/// Validate the merged configuration; returns every error found (SPEC §8.3).
pub fn validate(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();

    let server = &config.server;
    if !(MIN_POLL_INTERVAL_MS..=MAX_POLL_INTERVAL_MS).contains(&server.poll_interval_ms) {
        errors.push(format!(
            "server.poll_interval_ms: must be between {MIN_POLL_INTERVAL_MS} and \
             {MAX_POLL_INTERVAL_MS} ms (got {})",
            server.poll_interval_ms
        ));
    }
    if server.request_timeout_ms < 100 {
        errors.push(format!(
            "server.request_timeout_ms: must be at least 100 ms (got {})",
            server.request_timeout_ms
        ));
    }
    let retry = &server.retry;
    if retry.initial_ms < 50 {
        errors.push(format!(
            "server.retry.initial_ms: must be at least 50 ms (got {})",
            retry.initial_ms
        ));
    }
    if retry.max_ms < 1000 {
        errors.push(format!(
            "server.retry.max_ms: must be at least 1000 ms (got {})",
            retry.max_ms
        ));
    }
    if retry.max_ms < retry.initial_ms {
        errors.push("server.retry: max_ms must be >= initial_ms".to_string());
    }
    if retry.factor < 1.0 {
        errors.push(format!(
            "server.retry.factor: must be >= 1.0 (got {})",
            retry.factor
        ));
    }
    if !(0.0..=0.5).contains(&retry.jitter) {
        errors.push(format!(
            "server.retry.jitter: must be within 0.0..=0.5 (got {})",
            retry.jitter
        ));
    }

    let password_sources = [
        server.password.is_some(),
        server.password_env.is_some(),
        server.password_file.is_some(),
    ]
    .iter()
    .filter(|set| **set)
    .count();
    if password_sources > 1 {
        errors.push(
            "server: at most one of password, password_env, password_file may be set".to_string(),
        );
    }

    if !(60..=86_400).contains(&config.history.window_s) {
        errors.push(format!(
            "history.window_s: must be between 60 and 86400 seconds (got {})",
            config.history.window_s
        ));
    }
    if config.history.max_points_per_series < 60 {
        errors.push(format!(
            "history.max_points_per_series: must be at least 60 (got {})",
            config.history.max_points_per_series
        ));
    }

    if !(1..=60).contains(&config.ui.fps) {
        errors.push(format!(
            "ui.fps: must be between 1 and 60 (got {})",
            config.ui.fps
        ));
    }
    if config.ui.max_height < 4 {
        errors.push(format!(
            "ui.max_height: must be at least 4 (got {})",
            config.ui.max_height
        ));
    }
    if config.ui.layout.is_empty() {
        errors.push("ui.layout: must not be empty".to_string());
    }
    for key in &config.ui.layout {
        if !is_known_panel(key, &config.panels) {
            errors.push(format!(
                "ui.layout: unknown panel `{key}` (expected cpu, gpu, memory, thermal, fans, \
                 power, storage, network, or custom:<id>)"
            ));
        }
    }

    if config.charts.height < 3 {
        errors.push(format!(
            "charts.height: must be at least 3 (got {})",
            config.charts.height
        ));
    }
    if config.charts.min_span < 0.0 {
        errors.push(format!(
            "charts.min_span: must be >= 0.0 (got {})",
            config.charts.min_span
        ));
    }
    if let (Some(min), Some(max)) = (config.charts.y_min, config.charts.y_max) {
        if min >= max {
            errors.push(format!("charts: y_min ({min}) must be < y_max ({max})"));
        }
    }

    let mut windows: Vec<(String, &str)> =
        vec![("charts.window".to_string(), config.charts.window.as_str())];
    for (index, entry) in config.charts.window_cycle.iter().enumerate() {
        windows.push((format!("charts.window_cycle[{index}]"), entry.as_str()));
    }
    for (label, value) in windows {
        if humantime::parse_duration(value).is_err() {
            errors.push(format!(
                "{label}: invalid duration `{value}` (expected e.g. \"30s\", \"5m\", \"1h\")"
            ));
        }
    }

    errors
}

fn is_known_panel(key: &str, panels: &PanelsConfig) -> bool {
    match key {
        "cpu" | "gpu" | "memory" | "thermal" | "fans" | "power" | "storage" | "network" => true,
        _ => key
            .strip_prefix("custom:")
            .is_some_and(|id| panels.custom.iter().any(|panel| panel.id == id)),
    }
}

/// Serialize the effective config with secrets redacted (`--print-config`).
pub fn redacted_json(config: &Config) -> anyhow::Result<String> {
    let mut redacted = config.clone();
    if redacted.server.password.is_some() {
        redacted.server.password = Some("***".to_string());
    }
    Ok(serde_json::to_string_pretty(&redacted)?)
}

/// Write the built-in defaults as a config file (`--init-config`).
pub fn write_default_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        anyhow::bail!(
            "config file already exists: {} (remove it or pass a different path)",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(&Config::default())?;
    std::fs::write(path, format!("{json}\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli_from(args: &[&str]) -> Cli {
        Cli::parse_from(args)
    }

    fn plain(config: &Config) -> Config {
        let mut plain = config.clone();
        plain.schema = None;
        plain
    }

    #[test]
    fn defaults_match_spec() {
        let config = Config::default();
        assert_eq!(config.server.url, "http://127.0.0.1:8085");
        assert_eq!(config.server.poll_interval_ms, 1000);
        assert_eq!(config.server.request_timeout_ms, 2000);
        assert!(config.server.gzip);
        assert!(config.server.verify_version);
        assert_eq!(
            config.server.retry,
            RetryConfig {
                initial_ms: 500,
                max_ms: 30_000,
                factor: 2.0,
                jitter: 0.1
            }
        );
        assert_eq!(config.history.window_s, 900);
        assert_eq!(config.history.max_points_per_series, 3600);
        assert_eq!(config.ui.fps, 30);
        assert_eq!(config.ui.layout, vec!["cpu", "gpu", "memory"]);
        assert!(config.ui.auto_height);
        assert_eq!(config.ui.table_sort, TableSort::ValueDesc);
        assert_eq!(config.charts.marker, Marker::Braille);
        assert!(config.charts.enabled, "charts on by default");
        assert_eq!(config.charts.height, 8);
        assert_eq!(config.charts.window, "5m");
        assert_eq!(config.charts.window_cycle, vec!["1m", "5m", "15m"]);
        assert_eq!(config.charts.y_mode, YMode::Auto);
        assert_eq!(config.charts.min_span, 1.0);
        assert_eq!(config.theme.name, "default");
        assert_eq!(
            config.theme.thresholds.temperature,
            Threshold {
                warn: 80.0,
                crit: 95.0
            }
        );
        assert!(config.panels.cpu.enabled);
        assert!(config.panels.gpu.enabled);
        assert_eq!(config.panels.gpu.per_row, 2);
        assert!(config.panels.thermal.enabled);
        assert!(!config.panels.fans.enabled);
        assert!(!config.panels.storage.enabled);
        assert_eq!(config.keys["quit"], vec!["q", "esc", "ctrl+c"]);
        assert_eq!(config.keys["pause"], vec!["p", "space"]);
        assert_eq!(config.keys["toggle_thermal"], vec!["t"]);
        assert_eq!(config.keys["toggle_charts"], vec!["c"]);
        assert_eq!(config.keys.len(), 16);
    }

    #[test]
    fn default_config_validates() {
        assert_eq!(validate(&Config::default()), Vec::<String>::new());
    }

    #[test]
    fn validation_collects_all_errors() {
        let mut config = Config::default();
        config.server.poll_interval_ms = 100;
        config.ui.fps = 0;
        config.history.window_s = 10;
        let errors = validate(&config);
        assert_eq!(errors.len(), 3, "errors: {errors:?}");
        assert!(errors
            .iter()
            .any(|error| error.contains("poll_interval_ms")));
        assert!(errors.iter().any(|error| error.contains("ui.fps")));
        assert!(errors.iter().any(|error| error.contains("window_s")));
    }

    #[test]
    fn poll_interval_bounds() {
        let mut config = Config::default();
        config.server.poll_interval_ms = 249;
        assert!(!validate(&config).is_empty());
        config.server.poll_interval_ms = 250;
        assert!(validate(&config).is_empty());
        config.server.poll_interval_ms = 60_000;
        assert!(validate(&config).is_empty());
        config.server.poll_interval_ms = 60_001;
        assert!(!validate(&config).is_empty());
    }

    #[test]
    fn password_sources_are_exclusive() {
        let mut config = Config::default();
        config.server.password = Some("a".to_string());
        config.server.password_env = Some("LHM_PW".to_string());
        let errors = validate(&config);
        assert!(errors
            .iter()
            .any(|error| error.contains("at most one of password")));
    }

    #[test]
    fn layout_keys_are_validated() {
        let mut config = Config::default();
        config.ui.layout = vec!["cpu".to_string(), "gpu".to_string()];
        assert!(validate(&config).is_empty());

        config.ui.layout = vec!["cpu".to_string(), "nonsense".to_string()];
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("nonsense")));

        config.ui.layout = vec!["custom:nvme".to_string()];
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("custom:nvme")));

        config.panels.custom.push(CustomPanel {
            id: "nvme".to_string(),
            title: None,
            widgets: Vec::new(),
        });
        assert!(validate(&config).is_empty());
    }

    #[test]
    fn durations_are_validated() {
        let mut config = Config::default();
        config.charts.window = "5x".to_string();
        config.charts.window_cycle = vec!["1m".to_string(), "bogus".to_string()];
        let errors = validate(&config);
        assert!(errors.iter().any(|error| error.contains("charts.window")));
        assert!(errors.iter().any(|error| error.contains("window_cycle[1]")));
    }

    #[test]
    fn poll_duration_parsing() {
        assert_eq!(parse_duration_ms("1000").expect("bare ms"), 1000);
        assert_eq!(parse_duration_ms("1s").expect("seconds"), 1000);
        assert_eq!(parse_duration_ms("500ms").expect("millis"), 500);
        assert!(parse_duration_ms("5x").is_err());
    }

    #[test]
    fn precedence_defaults_env_cli() {
        let mut config = Config::default();
        assert_eq!(config.server.url, "http://127.0.0.1:8085");

        // As if loaded from a file.
        config.server.url = "http://file:8085".to_string();
        // Environment beats the file.
        apply_env_overrides(&mut config, |name| {
            (name == "LHM_TUI_URL").then(|| "http://env:8085".to_string())
        });
        assert_eq!(config.server.url, "http://env:8085");
        // CLI beats the environment.
        let cli = cli_from(&["lhm-tui", "--url", "http://cli:8085"]);
        let mut errors = Vec::new();
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(config.server.url, "http://cli:8085");
    }

    #[test]
    fn env_password_overrides_file_sources() {
        let mut config = Config::default();
        config.server.password_env = Some("LHM_PW".to_string());
        apply_env_overrides(&mut config, |name| {
            (name == "LHM_TUI_PASSWORD").then(|| "s3cret".to_string())
        });
        assert_eq!(config.server.password.as_deref(), Some("s3cret"));
        assert!(config.server.password_env.is_none());
        assert!(validate(&config).is_empty());
    }

    #[test]
    fn cli_overrides_config() {
        let mut config = Config::default();
        let cli = cli_from(&[
            "lhm-tui",
            "--poll",
            "2s",
            "--window",
            "30s",
            "--layout",
            "cpu, fans",
            "--ascii",
            "--no-color",
            "--theme",
            "nord",
        ]);
        let mut errors = Vec::new();
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(errors.is_empty(), "errors: {errors:?}");
        assert_eq!(config.server.poll_interval_ms, 2000);
        assert_eq!(config.charts.window, "30s");
        assert_eq!(config.ui.layout, vec!["cpu", "fans"]);
        assert!(config.ui.ascii);
        assert!(config.ui.no_color);
        assert_eq!(config.theme.name, "nord");
    }

    #[test]
    fn invalid_cli_durations_are_collected() {
        let mut config = Config::default();
        let cli = cli_from(&["lhm-tui", "--poll", "5x", "--window", "9q"]);
        let mut errors = Vec::new();
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert_eq!(errors.len(), 2, "errors: {errors:?}");
    }

    #[test]
    fn charts_can_be_toggled_via_cli() {
        let mut config = Config::default();
        let mut errors = Vec::new();
        let cli = cli_from(&["lhm-tui", "--no-charts"]);
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(!config.charts.enabled, "--no-charts disables charts");
        let cli = cli_from(&["lhm-tui", "--charts"]);
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(config.charts.enabled, "--charts re-enables them");
        assert!(errors.is_empty());
    }

    #[test]
    fn redaction_hides_password() {
        let mut config = Config::default();
        config.server.password = Some("hunter2".to_string());
        let json = redacted_json(&config).expect("serialize");
        assert!(!json.contains("hunter2"));
        assert!(json.contains("\"***\""));
    }

    #[test]
    fn unknown_keys_are_collected() {
        let text = r#"{ "server": { "url": "http://x:1", "pol_interval_ms": 5 } }"#;
        let (parsed, ignored) = parse_config_str(text);
        let parsed = parsed.expect("parses");
        assert_eq!(parsed.server.url, "http://x:1");
        assert!(
            ignored.iter().any(|path| path.contains("pol_interval_ms")),
            "ignored: {ignored:?}"
        );
    }

    #[test]
    fn invalid_enum_value_is_an_error() {
        let text = r#"{ "charts": { "marker": "brialle" } }"#;
        let (parsed, _) = parse_config_str(text);
        let message = parsed.expect_err("must fail").to_string();
        assert!(message.contains("brialle"), "message: {message}");
    }

    #[test]
    fn roundtrip_defaults() {
        let json = serde_json::to_string_pretty(&Config::default()).expect("serialize");
        let (parsed, ignored) = parse_config_str(&json);
        assert!(ignored.is_empty(), "ignored: {ignored:?}");
        assert_eq!(parsed.expect("parse"), Config::default());
    }

    #[test]
    fn example_config_is_valid_and_roundtrips() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("lhm-tui.example.json");
        let text = std::fs::read_to_string(&path).expect("lhm-tui.example.json exists");
        let (parsed, ignored) = parse_config_str(&text);
        assert!(ignored.is_empty(), "unknown keys in example: {ignored:?}");
        let config = parsed.expect("example parses");
        assert_eq!(plain(&config), Config::default());
        assert_eq!(validate(&config), Vec::<String>::new());

        let printed = redacted_json(&config).expect("prints");
        let (reparsed, _) = parse_config_str(&printed);
        assert_eq!(reparsed.expect("reparses"), config);
    }
}
