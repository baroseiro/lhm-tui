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
    /// GPU fan anti-stop floor (the only hardware write; off by default).
    pub fan_guard: FanGuardConfig,
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
            fan_guard: FanGuardConfig::default(),
            keys: default_keys(),
        }
    }
}

/// GPU fan anti-stop guard (T5.8): keeps fans spinning at `floor_percent` when
/// the GPU is idle, handing control back to the driver as soon as load or
/// temperature rises. This is the **only** hardware write the tool performs;
/// it is off by default and writable only on `Control` sensors (SPEC §3.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FanGuardConfig {
    /// Off by default — enabling this writes to LHM control sensors.
    pub enabled: bool,
    /// Duty cycle (%) held while idle (5–100; 0 is rejected so fans never stop).
    pub floor_percent: f32,
    /// Idle thresholds: floor engages when load ≤ `idle_load_percent` **and**
    /// the hottest GPU temperature ≤ `idle_temp_c` (65 °C by default — typical
    /// idle hot spots sit at 55–60 °C, so the default engages without tuning).
    pub idle_load_percent: f32,
    pub idle_temp_c: f32,
    /// Hysteresis before returning to automatic control.
    pub release_load_margin_percent: f32,
    pub release_temp_margin_c: f32,
}

impl Default for FanGuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            floor_percent: 20.0,
            idle_load_percent: 10.0,
            idle_temp_c: 65.0,
            release_load_margin_percent: 10.0,
            release_temp_margin_c: 5.0,
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
            show: ["load", "memory", "temp", "power", "fan", "clock"]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CustomWidget {
    pub kind: WidgetKind,
    /// Exact LHM SensorId (single-sensor widgets).
    pub sensor: Option<String>,
    /// Match rule, used when no exact sensor id is fixed.
    #[serde(rename = "match")]
    pub matcher: Option<SensorMatch>,
    pub label: Option<String>,
    pub unit: Option<String>,
    pub color: Option<String>,
    pub y_mode: Option<YMode>,
    /// Sensor-set filter (table and bars widgets).
    pub filter: Option<WidgetFilter>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetFilter {
    /// Hardware kind name (`Cpu`, `GpuNvidia`, …) or exact HardwareId.
    pub hardware: Option<String>,
    /// Sensor type names (`Temperature`, `Load`, …); empty = any.
    #[serde(default)]
    pub types: Vec<String>,
    /// Regex over the sensor name.
    pub name_regex: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    #[default]
    Graph,
    Gauge,
    Bars,
    Table,
    Sparkline,
}

/// Remappable key actions: `(action, description)` — used by validation,
/// defaults (in `default_keys`) and the in-TUI help overlay (T4.4).
pub const KNOWN_ACTIONS: &[(&str, &str)] = &[
    ("quit", "Quit"),
    ("help", "Help overlay"),
    ("pause", "Pause / resume polling"),
    ("window_next", "Chart window forward"),
    ("window_prev", "Chart window backward"),
    ("focus_next", "Focus next panel"),
    ("focus_prev", "Focus previous panel"),
    ("zoom", "Zoom focused panel"),
    ("browser", "Sensor browser"),
    ("sort", "Cycle table sort"),
    ("reload", "Reload config"),
    ("reset_minmax", "Reset LHM min/max"),
    ("ascii", "Toggle ASCII rendering"),
    ("debug", "Debug overlay"),
    ("toggle_thermal", "Toggle temperatures table"),
    ("toggle_charts", "Toggle charts"),
    ("toggle_fan_guard", "GPU fan anti-stop floor"),
];

/// Built-in theme names (T4.3); `custom` is used for `--theme <file>`.
pub const BUILTIN_THEMES: &[&str] = &["default", "gruvbox", "nord", "mono", "custom"];

fn default_keys() -> BTreeMap<String, Vec<String>> {
    let entries: [(&str, &[&str]); 17] = [
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
        ("toggle_fan_guard", &["f"]),
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
        let path = std::path::PathBuf::from(theme);
        if path.is_file() {
            match std::fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<ThemeConfig>(&text) {
                    Ok(mut parsed) => {
                        parsed.name = "custom".to_string();
                        config.theme = parsed;
                    }
                    Err(err) => errors.push(format!(
                        "--theme {}: invalid theme file: {err}",
                        path.display()
                    )),
                },
                Err(err) => errors.push(format!(
                    "--theme {}: cannot read theme file: {err}",
                    path.display()
                )),
            }
        } else {
            config.theme.name = theme.clone();
        }
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

    // Panel slot lists and GPU include kinds (T4.1).
    const CPU_SLOTS: &[&str] = &["temp", "load", "core_loads", "clock", "power"];
    const GPU_SLOTS: &[&str] = &["load", "memory", "temp", "power", "fan", "clock"];
    const MEMORY_SLOTS: &[&str] = &["used", "load", "virtual"];
    for (list, valid, label) in [
        (&config.panels.cpu.show, CPU_SLOTS, "panels.cpu.show"),
        (&config.panels.gpu.show, GPU_SLOTS, "panels.gpu.show"),
        (
            &config.panels.memory.show,
            MEMORY_SLOTS,
            "panels.memory.show",
        ),
    ] {
        for entry in list {
            if !valid.contains(&entry.as_str()) {
                errors.push(format!(
                    "{label}: unknown slot `{entry}` (expected one of {})",
                    valid.join(", ")
                ));
            }
        }
    }
    for entry in &config.panels.gpu.include {
        if crate::bindings::parse_hardware_kind(entry).is_none() {
            errors.push(format!(
                "panels.gpu.include: unknown hardware kind `{entry}` (e.g. GpuNvidia, GpuAmd, GpuIntel)"
            ));
        }
    }

    // Sensor overrides: known slot keys + compilable regexes (T4.1).
    for (key, rule) in &config.panels.sensor_overrides {
        if !is_known_slot_key(key) {
            errors.push(format!(
                "panels.sensor_overrides: unknown slot `{key}` (expected cpu.<{}>, gpu.<n>.<{}>, memory.<{}>)",
                CPU_OVERRIDE_SLOTS.join("|"),
                GPU_OVERRIDE_SLOTS.join("|"),
                MEMORY_OVERRIDE_SLOTS.join("|"),
            ));
        }
        if let Some(pattern) = rule
            .matcher
            .as_ref()
            .and_then(|matcher| matcher.name_regex.as_ref())
        {
            if regex::Regex::new(pattern).is_err() {
                errors.push(format!(
                    "panels.sensor_overrides.{key}: invalid name_regex `{pattern}`"
                ));
            }
        }
    }

    // Custom panels (T4.2): unique ids, widget sensor sources, regexes.
    let mut seen_ids: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for panel in &config.panels.custom {
        if panel.id.trim().is_empty() {
            errors.push("panels.custom: id must not be empty".to_string());
        } else if !seen_ids.insert(panel.id.as_str()) {
            errors.push(format!("panels.custom: duplicate panel id `{}`", panel.id));
        }
        for (index, widget) in panel.widgets.iter().enumerate() {
            let base = format!("panels.custom[{}, widget {index}]", panel.id);
            match widget.kind {
                WidgetKind::Table => {
                    if widget.filter.is_none() {
                        errors.push(format!("{base}: table widgets need a `filter`"));
                    }
                }
                _ => {
                    if widget.sensor.is_none()
                        && widget.matcher.is_none()
                        && widget.filter.is_none()
                    {
                        errors.push(format!("{base}: needs `sensor`, `match` or `filter`"));
                    }
                }
            }
            let regexes = [
                widget
                    .filter
                    .as_ref()
                    .and_then(|filter| filter.name_regex.as_ref()),
                widget
                    .matcher
                    .as_ref()
                    .and_then(|matcher| matcher.name_regex.as_ref()),
            ];
            for pattern in regexes.into_iter().flatten() {
                if regex::Regex::new(pattern).is_err() {
                    errors.push(format!("{base}: invalid name_regex `{pattern}`"));
                }
            }
            if let Some(filter) = &widget.filter {
                if let Some(hardware) = &filter.hardware {
                    if crate::bindings::parse_hardware_kind(hardware).is_none()
                        && !hardware.starts_with('/')
                    {
                        errors.push(format!(
                            "{base}.filter.hardware: `{hardware}` is neither a hardware kind nor a `/id`"
                        ));
                    }
                }
                for sensor_type in &filter.types {
                    if let crate::lhm::model::SensorKind::Other(name) =
                        crate::bindings::parse_sensor_kind(sensor_type)
                    {
                        if name == *sensor_type {
                            errors.push(format!(
                                "{base}.filter.types: unknown sensor type `{sensor_type}` (e.g. Temperature, Load, Fan)"
                            ));
                        }
                    }
                }
            }
        }
    }

    // Key remapping (T4.4): known actions, parseable specs, no conflicts.
    let action_names: Vec<&str> = KNOWN_ACTIONS.iter().map(|(action, _)| *action).collect();
    let mut seen_bindings: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for (action, specs) in &config.keys {
        if !action_names.contains(&action.as_str()) {
            errors.push(format!(
                "keys.{action}: unknown action (valid: {})",
                action_names.join(", ")
            ));
        }
        for spec in specs {
            match crate::app::parse_key_spec(spec) {
                None => errors.push(format!(
                    "keys.{action}: invalid key spec `{spec}` (e.g. q, ctrl+c, shift+tab, f5)"
                )),
                Some(key) => {
                    if let Some(canonical) = canonical_key_spec(&key) {
                        if let Some(other) = seen_bindings.insert(canonical, action.clone()) {
                            if other != *action {
                                errors.push(format!(
                                    "keys: `{spec}` is bound to both `{other}` and `{action}`"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Theme colours and built-in name (T4.3).
    if !BUILTIN_THEMES.contains(&config.theme.name.as_str()) {
        errors.push(format!(
            "theme.name: unknown theme `{}` (built-ins: {}; use --theme <file> for custom themes)",
            config.theme.name,
            BUILTIN_THEMES.join(", ")
        ));
    }
    for (field, value) in [
        ("accent", &config.theme.accent),
        ("text", &config.theme.text),
        ("muted", &config.theme.muted),
        ("warn", &config.theme.warn),
        ("crit", &config.theme.crit),
        ("cpu", &config.theme.cpu),
        ("gpu", &config.theme.gpu),
        ("memory", &config.theme.memory),
        ("network", &config.theme.network),
        ("storage", &config.theme.storage),
    ] {
        if crate::theme::parse_color(value).is_none() {
            errors.push(format!(
                "theme.{field}: invalid color `{value}` (name or #RRGGBB)"
            ));
        }
    }

    // Fan guard safety rails (T5.8): a floor of 0 % would stop the fans the
    // guard exists to keep spinning, so it is rejected outright.
    let guard = &config.fan_guard;
    if !(5.0..=100.0).contains(&guard.floor_percent) {
        errors.push(format!(
            "fan_guard.floor_percent: must be within 5..=100 (got {}; 0 would stop the fans)",
            guard.floor_percent
        ));
    }
    if !(0.0..=100.0).contains(&guard.idle_load_percent) {
        errors.push(format!(
            "fan_guard.idle_load_percent: must be within 0..=100 (got {})",
            guard.idle_load_percent
        ));
    }
    if !(-20.0..=110.0).contains(&guard.idle_temp_c) {
        errors.push(format!(
            "fan_guard.idle_temp_c: must be within -20..=110 (got {})",
            guard.idle_temp_c
        ));
    }
    if guard.release_load_margin_percent < 0.0 {
        errors.push("fan_guard.release_load_margin_percent: must be >= 0".to_string());
    }
    if guard.release_temp_margin_c < 0.0 {
        errors.push("fan_guard.release_temp_margin_c: must be >= 0".to_string());
    }

    errors
}

const CPU_OVERRIDE_SLOTS: &[&str] = &["load", "temp", "clock", "power", "core_loads"];
const GPU_OVERRIDE_SLOTS: &[&str] = &[
    "load",
    "mem_load",
    "mem_used",
    "mem_total",
    "temp",
    "hotspot",
    "mem_temp",
    "power",
    "clock",
    "fan",
];
const MEMORY_OVERRIDE_SLOTS: &[&str] = &[
    "used",
    "available",
    "load",
    "virtual_used",
    "virtual_available",
];

fn is_known_slot_key(key: &str) -> bool {
    let Some((prefix, rest)) = key.split_once('.') else {
        return false;
    };
    match prefix {
        "cpu" => CPU_OVERRIDE_SLOTS.contains(&rest),
        "memory" => MEMORY_OVERRIDE_SLOTS.contains(&rest),
        "gpu" => {
            let Some((index, slot)) = rest.split_once('.') else {
                return false;
            };
            index.parse::<u32>().is_ok() && GPU_OVERRIDE_SLOTS.contains(&slot)
        }
        _ => false,
    }
}

/// Canonical spelling of a parsed key for conflict detection (Shift+Tab and
/// BackTab are the same physical key).
fn canonical_key_spec(key: &ratatui::crossterm::event::KeyEvent) -> Option<String> {
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    let (mut code, mut modifiers) = (key.code, key.modifiers);
    if code == KeyCode::Tab && modifiers.contains(KeyModifiers::SHIFT) {
        code = KeyCode::BackTab;
    }
    if code == KeyCode::BackTab {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    Some(format!("{code:?}+{modifiers:?}"))
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
        assert_eq!(config.keys["toggle_fan_guard"], vec!["f"]);
        assert_eq!(config.keys.len(), 17);
        assert!(!config.fan_guard.enabled, "fan writes off by default");
        assert_eq!(config.fan_guard.floor_percent, 20.0);
        assert_eq!(config.fan_guard.idle_load_percent, 10.0);
        assert_eq!(config.fan_guard.idle_temp_c, 65.0);
        assert_eq!(config.fan_guard.release_load_margin_percent, 10.0);
        assert_eq!(config.fan_guard.release_temp_margin_c, 5.0);
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
    fn fan_guard_bounds_are_validated() {
        let mut config = Config::default();
        config.fan_guard.floor_percent = 0.0;
        config.fan_guard.idle_load_percent = 150.0;
        config.fan_guard.idle_temp_c = 130.0;
        config.fan_guard.release_load_margin_percent = -1.0;
        config.fan_guard.release_temp_margin_c = -2.0;
        let errors = validate(&config);
        assert_eq!(errors.len(), 5, "errors: {errors:?}");
        assert!(errors
            .iter()
            .any(|error| error.contains("fan_guard.floor_percent")));
        assert!(errors
            .iter()
            .any(|error| error.contains("fan_guard.idle_load_percent")));
        assert!(errors
            .iter()
            .any(|error| error.contains("fan_guard.idle_temp_c")));
        assert!(errors
            .iter()
            .any(|error| error.contains("fan_guard.release_load_margin_percent")));
        assert!(errors
            .iter()
            .any(|error| error.contains("fan_guard.release_temp_margin_c")));

        // The 5 % floor is accepted; 0 % (fans stopped) never is.
        config.fan_guard = FanGuardConfig::default();
        assert!(validate(&config).is_empty());
        config.fan_guard.floor_percent = 5.0;
        assert!(validate(&config).is_empty());
        config.fan_guard.floor_percent = 4.9;
        assert!(!validate(&config).is_empty());
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
    fn override_slot_keys_are_validated() {
        let mut config = Config::default();
        config
            .panels
            .sensor_overrides
            .insert("cpu.laod".to_string(), SensorOverride::default());
        let errors = validate(&config);
        assert!(
            errors.iter().any(|error| error.contains("cpu.laod")),
            "{errors:?}"
        );

        config.panels.sensor_overrides.clear();
        config
            .panels
            .sensor_overrides
            .insert("gpu.0.load".to_string(), SensorOverride::default());
        config
            .panels
            .sensor_overrides
            .insert("memory.virtual_used".to_string(), SensorOverride::default());
        assert_eq!(validate(&config), Vec::<String>::new());

        config
            .panels
            .sensor_overrides
            .insert("memory.swap".to_string(), SensorOverride::default());
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("memory.swap")));
    }

    #[test]
    fn show_lists_are_validated() {
        let mut config = Config::default();
        config.panels.cpu.show.push("temperatures".to_string());
        config.panels.gpu.show.push("watts".to_string());
        config.panels.memory.show.push("swap".to_string());
        let errors = validate(&config);
        assert!(errors
            .iter()
            .any(|error| error.contains("panels.cpu.show") && error.contains("temperatures")));
        assert!(errors
            .iter()
            .any(|error| error.contains("panels.gpu.show") && error.contains("watts")));
        assert!(errors
            .iter()
            .any(|error| error.contains("panels.memory.show") && error.contains("swap")));

        config.panels.gpu.include = vec!["Radeon".to_string()];
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("panels.gpu.include")));
    }

    #[test]
    fn key_map_is_validated() {
        let mut config = Config::default();
        config
            .keys
            .insert("fire_lasers".to_string(), vec!["x".to_string()]);
        let errors = validate(&config);
        assert!(errors
            .iter()
            .any(|error| error.contains("fire_lasers") && error.contains("valid: quit")));

        let mut config = Config::default();
        config
            .keys
            .get_mut("quit")
            .expect("quit")
            .push("ctrl+".to_string());
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("invalid key spec")));

        let mut config = Config::default();
        config
            .keys
            .insert("sort".to_string(), vec!["p".to_string()]);
        let errors = validate(&config);
        assert!(
            errors.iter().any(|error| error.contains("bound to both")),
            "{errors:?}"
        );
    }

    #[test]
    fn custom_panels_are_validated() {
        let mut config = Config::default();
        config.panels.custom.push(CustomPanel {
            id: "dup".to_string(),
            title: None,
            widgets: Vec::new(),
        });
        config.panels.custom.push(CustomPanel {
            id: "dup".to_string(),
            title: None,
            widgets: vec![
                CustomWidget::default(), // graph without sensor/match/filter
                CustomWidget {
                    kind: WidgetKind::Table,
                    ..Default::default()
                }, // table without filter
                CustomWidget {
                    kind: WidgetKind::Gauge,
                    sensor: Some("/x".to_string()),
                    ..Default::default()
                },
                CustomWidget {
                    kind: WidgetKind::Gauge,
                    matcher: Some(SensorMatch {
                        name_regex: Some("[".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
        });
        let errors = validate(&config);
        assert!(errors
            .iter()
            .any(|error| error.contains("duplicate panel id")));
        assert!(errors
            .iter()
            .any(|error| error.contains("needs `sensor`, `match` or `filter`")));
        assert!(errors
            .iter()
            .any(|error| error.contains("table widgets need a `filter`")));
        assert!(errors
            .iter()
            .any(|error| error.contains("invalid name_regex")));

        let mut config = Config::default();
        config.panels.custom.push(CustomPanel {
            id: "nvme".to_string(),
            title: Some("NVMe".to_string()),
            widgets: vec![
                CustomWidget {
                    kind: WidgetKind::Graph,
                    sensor: Some("/nvme/0/temperature/0".to_string()),
                    ..Default::default()
                },
                CustomWidget {
                    kind: WidgetKind::Table,
                    filter: Some(WidgetFilter {
                        hardware: Some("/nvme/0".to_string()),
                        types: vec!["Temperature".to_string()],
                        name_regex: None,
                    }),
                    ..Default::default()
                },
            ],
        });
        config.ui.layout.push("custom:nvme".to_string());
        assert_eq!(validate(&config), Vec::<String>::new());
    }

    #[test]
    fn theme_is_validated() {
        let mut config = Config::default();
        config.theme.name = "neon".to_string();
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("unknown theme")));

        let mut config = Config::default();
        config.theme.accent = "#12".to_string();
        assert!(validate(&config)
            .iter()
            .any(|error| error.contains("theme.accent")));

        let mut config = Config::default();
        config.theme.name = "gruvbox".to_string();
        assert_eq!(validate(&config), Vec::<String>::new());
    }

    #[test]
    fn cli_theme_accepts_files_and_names() {
        let mut config = Config::default();
        let mut errors = Vec::new();
        let cli = cli_from(&["lhm-tui", "--theme", "nord"]);
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(errors.is_empty());
        assert_eq!(config.theme.name, "nord");
        assert_eq!(validate(&config), Vec::<String>::new());

        let path = std::env::temp_dir().join(format!("lhm-tui-theme-{}.json", std::process::id()));
        std::fs::write(&path, r##"{"accent": "#112233", "cpu": "red"}"##).expect("write theme");
        let mut config = Config::default();
        let mut errors = Vec::new();
        let cli = cli_from(&["lhm-tui", "--theme", path.to_str().expect("utf8 path")]);
        apply_cli_overrides(&mut config, &cli, &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(config.theme.name, "custom");
        assert_eq!(config.theme.accent, "#112233");
        assert_eq!(config.theme.cpu, "red");
        assert_eq!(
            validate(&config),
            Vec::<String>::new(),
            "file themes validate (incl. name)"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn validation_errors_snapshot() {
        let mut config = Config::default();
        config.server.poll_interval_ms = 100;
        config.panels.cpu.show.push("temperatures".to_string());
        config
            .keys
            .insert("fire_lasers".to_string(), vec!["x".to_string()]);
        config.theme.name = "neon".to_string();
        insta::assert_debug_snapshot!(validate(&config));
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
