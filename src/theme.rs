//! Theme: color parsing, threshold styling and dimming (SPEC §6.5).
//!
//! The full theme system (built-ins, downgrades, theme files) lands in M4
//! (T4.3); this is the subset panels need today: parse `theme.*` colors from
//! config and style values by threshold.

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
        Self {
            accent: parse_color(&config.accent).unwrap_or(Color::Cyan),
            text: parse_color(&config.text).unwrap_or(Color::Gray),
            muted: parse_color(&config.muted).unwrap_or(Color::DarkGray),
            warn: parse_color(&config.warn).unwrap_or(Color::Yellow),
            crit: parse_color(&config.crit).unwrap_or(Color::Red),
            cpu: parse_color(&config.cpu).unwrap_or(Color::Cyan),
            gpu: parse_color(&config.gpu).unwrap_or(Color::Magenta),
            memory: parse_color(&config.memory).unwrap_or(Color::Green),
            network: parse_color(&config.network).unwrap_or(Color::LightBlue),
            storage: parse_color(&config.storage).unwrap_or(Color::Yellow),
            thresholds: config.thresholds.clone(),
        }
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
        // temperature defaults: warn 80, crit 95
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
        // load defaults: warn 90, crit 98
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
}
