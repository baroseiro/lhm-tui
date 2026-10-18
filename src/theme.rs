//! Theme: built-in palettes, color parsing, realcolor downgrades and
//! threshold styling (SPEC §6.5, §8.2; T4.3).

use ratatui::style::{Color, Modifier, Style};

use crate::config::{ThemeConfig, Thresholds};
use crate::lhm::model::SensorKind;

/// Value metric used for threshold buckets (SPEC §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Temperature,
    Load,
    GpuMemory,
}

/// Map a sensor kind onto the threshold metric that applies to it.
pub fn metric_for(kind: &SensorKind) -> Option<Metric> {
    match kind {
        SensorKind::Temperature => Some(Metric::Temperature),
        SensorKind::Load | SensorKind::Control | SensorKind::Level => Some(Metric::Load),
        SensorKind::Data | SensorKind::SmallData => Some(Metric::GpuMemory),
        _ => None,
    }
}

/// Terminal colour support, detected once per frame from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
}

/// Read the environment for colour capability (COLORTERM / TERM / WT_SESSION).
pub fn detect_color_depth() -> ColorDepth {
    color_depth_for(
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("WT_SESSION").is_some(),
        cfg!(windows),
    )
}

/// Pure colour-depth decision (injectable for tests).
pub fn color_depth_for(
    colorterm: Option<&str>,
    term: Option<&str>,
    wt_session: bool,
    windows: bool,
) -> ColorDepth {
    let colorterm = colorterm.unwrap_or("").to_ascii_lowercase();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return ColorDepth::TrueColor;
    }
    let term = term.unwrap_or("").to_ascii_lowercase();
    if term.contains("truecolor") || term.contains("24bit") || wt_session {
        return ColorDepth::TrueColor;
    }
    if term.contains("256color") {
        return ColorDepth::Ansi256;
    }
    if windows {
        // Modern conhost and terminals on Windows all do 256 colours.
        ColorDepth::Ansi256
    } else {
        ColorDepth::Ansi16
    }
}

/// Downgrade an RGB colour for the given terminal depth; named and indexed
/// colours pass through.
pub fn downgrade_color(color: Color, depth: ColorDepth) -> Color {
    match (color, depth) {
        (_, ColorDepth::TrueColor) => color,
        (Color::Rgb(red, green, blue), ColorDepth::Ansi256) => {
            Color::Indexed(rgb_to_ansi256(red, green, blue))
        }
        (Color::Rgb(red, green, blue), ColorDepth::Ansi16) => {
            ansi16_named(rgb_to_ansi16(red, green, blue))
        }
        (other, _) => other,
    }
}

/// Standard xterm-256 conversion (extended cube or grayscale, whichever is
/// nearer).
pub fn rgb_to_ansi256(red: u8, green: u8, blue: u8) -> u8 {
    let level = |value: u8| -> u8 {
        match value {
            0..=47 => 0,
            48..=114 => 1,
            _ => ((u16::from(value) - 35) / 40).min(5) as u8,
        }
    };
    let (r, g, b) = (level(red), level(green), level(blue));
    let cube_index = 16 + 36 * r + 6 * g + b;
    let cube_rgb = |component: u8| -> u8 {
        if component == 0 {
            0
        } else {
            55 + 40 * component
        }
    };
    let cube = (cube_rgb(r), cube_rgb(g), cube_rgb(b));

    let gray_level = if red == green && green == blue {
        red
    } else {
        ((u16::from(red) + u16::from(green) + u16::from(blue)) / 3) as u8
    };
    let gray_index = if gray_level < 8 {
        16u8
    } else if gray_level > 238 {
        231u8
    } else {
        232 + (gray_level - 8) / 10
    };
    let gray_value = match gray_index {
        16 => 0,
        231 => 255,
        index => 8 + 10 * (index - 232),
    };

    let distance = |a: (u8, u8, u8), b: (u8, u8, u8)| -> u32 {
        let d = |x: u8, y: u8| {
            let diff = i32::from(x) - i32::from(y);
            (diff * diff) as u32
        };
        d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
    };
    let target = (red, green, blue);
    if distance(cube, target) <= distance((gray_value, gray_value, gray_value), target) {
        cube_index
    } else {
        gray_index
    }
}

/// xterm 16-colour palette in RGB, for nearest-match downgrades.
const ANSI16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// Nearest of the 16 ANSI colours (index 0–15).
pub fn rgb_to_ansi16(red: u8, green: u8, blue: u8) -> u8 {
    let mut best = 0u8;
    let mut best_distance = u32::MAX;
    for (index, (r, g, b)) in ANSI16_RGB.iter().enumerate() {
        let diff = |x: u8, y: u8| {
            let value = i32::from(x) - i32::from(y);
            (value * value) as u32
        };
        let distance = diff(red, *r) + diff(green, *g) + diff(blue, *b);
        if distance < best_distance {
            best_distance = distance;
            best = index as u8;
        }
    }
    best
}

/// RGB for an xterm-256 index (used when degrading 256 → 16).
pub fn ansi256_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => ANSI16_RGB[index as usize],
        16..=231 => {
            let offset = index - 16;
            let component = |value: u8| -> u8 {
                if value == 0 {
                    0
                } else {
                    55 + 40 * value
                }
            };
            (
                component(offset / 36),
                component((offset % 36) / 6),
                component(offset % 6),
            )
        }
        _ => {
            let level = 8 + 10 * (index - 232);
            (level, level, level)
        }
    }
}

/// Named ratatui colour for an ANSI 16 index.
pub fn ansi16_named(index: u8) -> Color {
    match index {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        _ => Color::White,
    }
}

struct Palette {
    accent: Color,
    text: Color,
    muted: Color,
    warn: Color,
    crit: Color,
    cpu: Color,
    gpu: Color,
    memory: Color,
    network: Color,
    storage: Color,
}

fn builtin_palette(name: &str) -> Palette {
    match name {
        "gruvbox" => Palette {
            accent: Color::Rgb(0x83, 0xa5, 0x98),
            text: Color::Rgb(0xeb, 0xdb, 0xb2),
            muted: Color::Rgb(0x92, 0x83, 0x74),
            warn: Color::Rgb(0xfa, 0xbd, 0x2f),
            crit: Color::Rgb(0xfb, 0x49, 0x34),
            cpu: Color::Rgb(0x83, 0xa5, 0x98),
            gpu: Color::Rgb(0xd3, 0x86, 0x9b),
            memory: Color::Rgb(0xb8, 0xbb, 0x26),
            network: Color::Rgb(0x8e, 0xc0, 0x7c),
            storage: Color::Rgb(0xfa, 0xbd, 0x2f),
        },
        "nord" => Palette {
            accent: Color::Rgb(0x88, 0xc0, 0xd0),
            text: Color::Rgb(0xd8, 0xde, 0xe9),
            muted: Color::Rgb(0x4c, 0x56, 0x6a),
            warn: Color::Rgb(0xeb, 0xcb, 0x8b),
            crit: Color::Rgb(0xbf, 0x61, 0x6a),
            cpu: Color::Rgb(0x88, 0xc0, 0xd0),
            gpu: Color::Rgb(0xb4, 0x8e, 0xad),
            memory: Color::Rgb(0xa3, 0xbe, 0x8c),
            network: Color::Rgb(0x8f, 0xbc, 0xbb),
            storage: Color::Rgb(0xeb, 0xcb, 0x8b),
        },
        "mono" => Palette {
            accent: Color::Gray,
            text: Color::White,
            muted: Color::DarkGray,
            warn: Color::Gray,
            crit: Color::White,
            cpu: Color::Gray,
            gpu: Color::Gray,
            memory: Color::Gray,
            network: Color::Gray,
            storage: Color::Gray,
        },
        _ => {
            // "default" and file-loaded themes: the config colours themselves
            // are the palette (see `from_config`).
            let defaults = ThemeConfig::default();
            let parse = |value: &str, fallback: Color| parse_color(value).unwrap_or(fallback);
            Palette {
                accent: parse(&defaults.accent, Color::Cyan),
                text: parse(&defaults.text, Color::Gray),
                muted: parse(&defaults.muted, Color::DarkGray),
                warn: parse(&defaults.warn, Color::Yellow),
                crit: parse(&defaults.crit, Color::Red),
                cpu: parse(&defaults.cpu, Color::Cyan),
                gpu: parse(&defaults.gpu, Color::Magenta),
                memory: parse(&defaults.memory, Color::Green),
                network: parse(&defaults.network, Color::LightBlue),
                storage: parse(&defaults.storage, Color::Yellow),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub text: Color,
    pub muted: Color,
    pub warn: Color,
    pub crit: Color,
    pub cpu: Color,
    pub gpu: Color,
    pub memory: Color,
    pub network: Color,
    pub storage: Color,
    pub thresholds: Thresholds,
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        let defaults = ThemeConfig::default();
        let palette = builtin_palette(&config.name);
        // A colour field differing from the built-in default is an explicit
        // override; otherwise the selected palette wins.
        let pick = |value: &str, default_value: &str, preset: Color| -> Color {
            if value != default_value {
                parse_color(value).unwrap_or(preset)
            } else {
                preset
            }
        };
        Self {
            accent: pick(&config.accent, &defaults.accent, palette.accent),
            text: pick(&config.text, &defaults.text, palette.text),
            muted: pick(&config.muted, &defaults.muted, palette.muted),
            warn: pick(&config.warn, &defaults.warn, palette.warn),
            crit: pick(&config.crit, &defaults.crit, palette.crit),
            cpu: pick(&config.cpu, &defaults.cpu, palette.cpu),
            gpu: pick(&config.gpu, &defaults.gpu, palette.gpu),
            memory: pick(&config.memory, &defaults.memory, palette.memory),
            network: pick(&config.network, &defaults.network, palette.network),
            storage: pick(&config.storage, &defaults.storage, palette.storage),
            thresholds: config.thresholds.clone(),
        }
    }

    /// Distinguishability-free theme for `NO_COLOR` / `--no-color`.
    pub fn plain() -> Self {
        let reset = Color::Reset;
        Self {
            accent: reset,
            text: reset,
            muted: reset,
            warn: reset,
            crit: reset,
            cpu: reset,
            gpu: reset,
            memory: reset,
            network: reset,
            storage: reset,
            thresholds: Thresholds::default(),
        }
    }

    /// Downgrade RGB colours for terminals without truecolor support.
    pub fn downgraded(mut self, depth: ColorDepth) -> Self {
        self.accent = downgrade_color(self.accent, depth);
        self.text = downgrade_color(self.text, depth);
        self.muted = downgrade_color(self.muted, depth);
        self.warn = downgrade_color(self.warn, depth);
        self.crit = downgrade_color(self.crit, depth);
        self.cpu = downgrade_color(self.cpu, depth);
        self.gpu = downgrade_color(self.gpu, depth);
        self.memory = downgrade_color(self.memory, depth);
        self.network = downgrade_color(self.network, depth);
        self.storage = downgrade_color(self.storage, depth);
        self
    }

    /// Style for a live value: normal / warn / crit (SPEC §6.5).
    pub fn value_style(&self, metric: Metric, value: f64) -> Style {
        let threshold = match metric {
            Metric::Temperature => &self.thresholds.temperature,
            Metric::Load => &self.thresholds.load,
            Metric::GpuMemory => &self.thresholds.gpu_memory,
        };
        if value >= threshold.crit {
            Style::new().fg(self.crit)
        } else if value >= threshold.warn {
            Style::new().fg(self.warn)
        } else {
            Style::new().fg(self.text)
        }
    }

    /// Threshold style for a sensor kind, when the kind has a metric.
    pub fn sensor_style(&self, kind: &SensorKind, value: Option<f64>) -> Style {
        match (metric_for(kind), value) {
            (Some(metric), Some(value)) => self.value_style(metric, value),
            _ => Style::new().fg(self.text),
        }
    }

    /// Resolve a custom-panel colour: palette key, named colour or hex.
    pub fn widget_color(&self, name: Option<&str>) -> Color {
        let Some(name) = name else {
            return self.accent;
        };
        match name {
            "accent" => self.accent,
            "text" => self.text,
            "muted" => self.muted,
            "warn" => self.warn,
            "crit" => self.crit,
            "cpu" => self.cpu,
            "gpu" => self.gpu,
            "memory" => self.memory,
            "network" => self.network,
            "storage" => self.storage,
            other => parse_color(other).unwrap_or(self.accent),
        }
    }
}

/// Add the dim modifier when the UI shows stale data (SPEC §10.1).
pub fn with_dim(style: Style, dim: bool) -> Style {
    if dim {
        style.add_modifier(Modifier::DIM)
    } else {
        style
    }
}

/// Parse a color name or `#RRGGBB` value (SPEC §8.2: names or hex).
pub fn parse_color(text: &str) -> Option<Color> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() == 6 {
            let parse = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
            let red = parse(0..2)?;
            let green = parse(2..4)?;
            let blue = parse(4..6)?;
            return Some(Color::Rgb(red, green, blue));
        }
        return None;
    }
    Some(match text.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "white" => Color::White,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "reset" => Color::Reset,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_hex_colors() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color(" DarkGray "), Some(Color::DarkGray));
        assert_eq!(parse_color("#7aa2f7"), Some(Color::Rgb(0x7a, 0xa2, 0xf7)));
        assert_eq!(parse_color("#c0caf5"), Some(Color::Rgb(0xc0, 0xca, 0xf5)));
        assert_eq!(parse_color("#xyzxyz"), None);
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("neon"), None);
    }

    #[test]
    fn value_styles_follow_thresholds() {
        let theme = Theme::from_config(&ThemeConfig::default());
        assert_eq!(
            theme.value_style(Metric::Temperature, 46.9).fg,
            Some(theme.text)
        );
        assert_eq!(
            theme.value_style(Metric::Temperature, 80.0).fg,
            Some(theme.warn)
        );
        assert_eq!(
            theme.value_style(Metric::Temperature, 95.0).fg,
            Some(theme.crit)
        );
        assert_eq!(theme.value_style(Metric::Load, 41.2).fg, Some(theme.text));
        assert_eq!(theme.value_style(Metric::Load, 99.0).fg, Some(theme.crit));
    }

    #[test]
    fn sensor_kind_maps_to_metric() {
        assert_eq!(
            metric_for(&SensorKind::Temperature),
            Some(Metric::Temperature)
        );
        assert_eq!(metric_for(&SensorKind::Load), Some(Metric::Load));
        assert_eq!(metric_for(&SensorKind::Data), Some(Metric::GpuMemory));
        assert_eq!(metric_for(&SensorKind::Fan), None);
    }

    #[test]
    fn dim_adds_modifier() {
        let style = with_dim(Style::new().fg(Color::Red), true);
        assert!(style.add_modifier.contains(Modifier::DIM));
        let style = with_dim(Style::new().fg(Color::Red), false);
        assert!(!style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn builtin_themes_apply_with_field_overrides() {
        let mut config = ThemeConfig {
            name: "nord".to_string(),
            ..ThemeConfig::default()
        };
        let theme = Theme::from_config(&config);
        assert_eq!(theme.accent, Color::Rgb(0x88, 0xc0, 0xd0), "nord accent");

        // An explicitly changed field beats the palette.
        config.cpu = "#ff0000".to_string();
        let theme = Theme::from_config(&config);
        assert_eq!(theme.cpu, Color::Rgb(0xff, 0x00, 0x00));
        assert_eq!(
            theme.gpu,
            Color::Rgb(0xb4, 0x8e, 0xad),
            "other fields stay nord"
        );

        // Mono is grayscale-only.
        config.name = "mono".to_string();
        config.cpu = ThemeConfig::default().cpu;
        let mono = Theme::from_config(&config);
        assert!(matches!(
            mono.text,
            Color::White | Color::Gray | Color::DarkGray | Color::Black
        ));
        assert!(matches!(
            mono.accent,
            Color::White | Color::Gray | Color::DarkGray | Color::Black
        ));
    }

    #[test]
    fn plain_theme_resets_all_colors() {
        let theme = Theme::plain();
        for color in [
            theme.accent,
            theme.text,
            theme.warn,
            theme.crit,
            theme.cpu,
            theme.gpu,
        ] {
            assert_eq!(color, Color::Reset);
        }
    }

    #[test]
    fn depth_detection() {
        assert_eq!(
            color_depth_for(Some("truecolor"), Some("xterm"), false, false),
            ColorDepth::TrueColor
        );
        assert_eq!(
            color_depth_for(Some("24bit"), None, false, false),
            ColorDepth::TrueColor
        );
        assert_eq!(
            color_depth_for(None, Some("xterm"), true, false),
            ColorDepth::TrueColor
        );
        assert_eq!(
            color_depth_for(None, Some("xterm-256color"), false, false),
            ColorDepth::Ansi256
        );
        assert_eq!(
            color_depth_for(None, Some("xterm"), false, true),
            ColorDepth::Ansi256
        );
        assert_eq!(
            color_depth_for(None, Some("xterm"), false, false),
            ColorDepth::Ansi16
        );
    }

    #[test]
    fn rgb_to_ansi256_conversion() {
        assert_eq!(rgb_to_ansi256(0, 0, 0), 16);
        assert_eq!(rgb_to_ansi256(255, 0, 0), 196);
        assert_eq!(rgb_to_ansi256(0, 255, 0), 46);
        assert_eq!(rgb_to_ansi256(255, 255, 255), 231);
        assert_eq!(rgb_to_ansi256(128, 128, 128), 244);
    }

    #[test]
    fn rgb_to_ansi16_conversion() {
        assert_eq!(rgb_to_ansi16(255, 0, 0), 9);
        assert_eq!(rgb_to_ansi16(0, 0, 0), 0);
        assert_eq!(rgb_to_ansi16(255, 255, 255), 15);
        assert_eq!(ansi16_named(9), Color::LightRed);
    }

    #[test]
    fn downgrade_pipeline() {
        let rgb = Color::Rgb(0xff, 0x00, 0x00);
        assert_eq!(downgrade_color(rgb, ColorDepth::TrueColor), rgb);
        assert_eq!(
            downgrade_color(rgb, ColorDepth::Ansi256),
            Color::Indexed(196)
        );
        assert_eq!(downgrade_color(rgb, ColorDepth::Ansi16), Color::LightRed);
        // Named colours pass through untouched.
        assert_eq!(
            downgrade_color(Color::Cyan, ColorDepth::Ansi16),
            Color::Cyan
        );
    }

    #[test]
    fn widget_color_resolution() {
        let theme = Theme::from_config(&ThemeConfig::default());
        assert_eq!(theme.widget_color(Some("storage")), theme.storage);
        assert_eq!(
            theme.widget_color(Some("#112233")),
            Color::Rgb(0x11, 0x22, 0x33)
        );
        assert_eq!(theme.widget_color(Some("cyan")), Color::Cyan);
        assert_eq!(theme.widget_color(Some("nonsense")), theme.accent);
        assert_eq!(theme.widget_color(None), theme.accent);
    }
}
