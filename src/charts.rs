//! Chart rendering (SPEC §7): min/max envelope decimation, nice-step
//! autoscale with shrink hysteresis, and the braille/halfblock/ascii graph
//! widget. Charts are optional: panels only call this when `charts.enabled`
//! is true (config or `--no-charts`).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::config::{Marker, YMode};
use crate::theme::{with_dim, Theme};

/// One charted series: recent points (oldest → newest, `None` = gap).
#[derive(Debug, Clone)]
pub struct Series {
    pub label: String,
    pub color: Color,
    /// Legend value text (already formatted, e.g. `46.9 °C`).
    pub latest_text: String,
    pub latest_style: Style,
    pub points: Vec<Option<f32>>,
}

/// Everything the graph widget needs to draw one metric chart.
pub struct GraphSpec<'a> {
    pub series: &'a [Series],
    /// Stable key for the autoscale hysteresis state (panel + graph).
    pub scale_key: String,
    pub y_mode: YMode,
    pub y_fixed: Option<(f64, f64)>,
    pub min_span: f64,
    pub marker: Marker,
    pub grid: bool,
    pub legend: bool,
    /// Chart window label for the axis row, e.g. `5m`.
    pub window_label: String,
}

/// Per-panel axis memory with shrink hysteresis (SPEC §7.1 step 4).
#[derive(Debug, Default)]
pub struct ChartsState {
    scales: BTreeMap<String, ScaleState>,
}

#[derive(Debug)]
struct ScaleState {
    min: f64,
    max: f64,
    last_shrink: Instant,
}

impl ChartsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Axis bounds for `key`: grow immediately, shrink only when the
    /// candidate range fits inside ≤ 90 % of the current one and has stayed
    /// that way for ≥ 5 s.
    pub fn axis(&mut self, key: &str, data: (f64, f64), min_span: f64, now: Instant) -> (f64, f64) {
        let (mut data_min, mut data_max) = data;
        if !data_min.is_finite() || !data_max.is_finite() {
            data_min = 0.0;
            data_max = 1.0;
        }
        let pad = ((data_max - data_min) * 0.05).max(min_span * 0.5);
        let (mut lo, mut hi) = (data_min - pad, data_max + pad);
        if hi - lo < min_span {
            let mid = (lo + hi) / 2.0;
            lo = mid - min_span / 2.0;
            hi = mid + min_span / 2.0;
        }
        let (cand_min, cand_max) = nice_bounds(lo, hi);

        match self.scales.get_mut(key) {
            None => {
                self.scales.insert(
                    key.to_string(),
                    ScaleState {
                        min: cand_min,
                        max: cand_max,
                        last_shrink: now,
                    },
                );
                (cand_min, cand_max)
            }
            Some(state) => {
                let grows = cand_min < state.min || cand_max > state.max;
                let fits = cand_min >= state.min && cand_max <= state.max;
                let much_smaller = (cand_max - cand_min) < 0.9 * (state.max - state.min);
                if grows {
                    state.min = cand_min;
                    state.max = cand_max;
                    state.last_shrink = now;
                } else if fits && much_smaller {
                    if now.duration_since(state.last_shrink) >= Duration::from_secs(5) {
                        state.min = cand_min;
                        state.max = cand_max;
                        state.last_shrink = now;
                    }
                } else {
                    state.last_shrink = now;
                }
                (state.min, state.max)
            }
        }
    }
}

/// Min/max envelope decimation (SPEC §7.1 step 3): one `(min, max)` span per
/// pixel column; `None` marks a fully-gapped column so lines break. Series
/// shorter than the pixel width are right-aligned — real-time charts grow
/// from the right instead of stretching one sample across the screen.
pub fn decimate(points: &[Option<f32>], columns: usize) -> Vec<Option<(f64, f64)>> {
    if columns == 0 || points.is_empty() {
        return Vec::new();
    }
    let n = points.len();
    if n < columns {
        let mut out = vec![None; columns - n];
        out.extend(points.iter().map(|point| {
            point.map(|value| {
                let value = f64::from(value);
                (value, value)
            })
        }));
        return out;
    }
    let mut out = Vec::with_capacity(columns);
    for column in 0..columns {
        let start = column * n / columns;
        let end = ((column + 1) * n).div_ceil(columns).clamp(start + 1, n);
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        let mut any = false;
        for value in points[start..end.min(n)].iter().flatten() {
            let value = f64::from(*value);
            lo = lo.min(value);
            hi = hi.max(value);
            any = true;
        }
        out.push(any.then_some((lo, hi)));
    }
    out
}

/// Nice step: 1/2/5 × 10ⁿ.
pub fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 || !raw.is_finite() {
        return 1.0;
    }
    let exponent = raw.log10().floor();
    let base = 10f64.powf(exponent);
    let normalized = raw / base;
    let factor = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    factor * base
}

/// Round axis bounds outward to nice steps (~4 grid lines).
pub fn nice_bounds(min: f64, max: f64) -> (f64, f64) {
    let span = (max - min).max(1e-9);
    let step = nice_step(span / 4.0);
    ((min / step).floor() * step, (max / step).ceil() * step)
}

// Braille dot bit per (column, row) inside a cell (Unicode 0x2800 + bits).
const BRAILLE_DOTS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

fn dots_per_cell(marker: Marker) -> (usize, usize) {
    match marker {
        Marker::Braille => (2, 4),
        Marker::Halfblock | Marker::Ascii => (1, 2),
    }
}

fn cell_char(bits: u8, marker: Marker) -> char {
    match marker {
        Marker::Braille => char::from_u32(0x2800 + u32::from(bits)).unwrap_or(' '),
        Marker::Halfblock => match bits {
            0 => ' ',
            1 => '▄',
            2 => '▀',
            _ => '█',
        },
        Marker::Ascii => match bits {
            0 => ' ',
            1 => '_',
            2 => '-',
            _ => '=',
        },
    }
}

#[derive(Clone, Copy, Default)]
struct GridCell {
    bits: u8,
    color: Option<Color>,
}

/// Draw one metric graph: dots area + optional legend line + axis row.
pub fn draw_graph(
    frame: &mut Frame,
    area: Rect,
    spec: &GraphSpec,
    state: &mut ChartsState,
    theme: &Theme,
    stale: bool,
    now: Instant,
) {
    if area.width < 6 || area.height < 2 || spec.series.is_empty() {
        return;
    }
    let legend_rows = u16::from(spec.legend);
    let axis_rows = u16::from(area.height >= 8);
    let dot_height = area.height.saturating_sub(legend_rows + axis_rows).max(1);
    let dot_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: dot_height,
    };

    // Data bounds across all series.
    let (mut data_min, mut data_max) = (f64::MAX, f64::MIN);
    for series in spec.series {
        for point in series.points.iter().flatten() {
            let value = f64::from(*point);
            data_min = data_min.min(value);
            data_max = data_max.max(value);
        }
    }
    if data_min > data_max {
        data_min = 0.0;
        data_max = 1.0;
    }
    let (min, max) = match spec.y_mode {
        YMode::Percent => (0.0, 100.0),
        YMode::Fixed => spec
            .y_fixed
            .unwrap_or((data_min, (data_max).max(data_min + 1.0))),
        YMode::Auto => state.axis(&spec.scale_key, (data_min, data_max), spec.min_span, now),
    };
    if min >= max {
        return;
    }

    let (dx, dy) = dots_per_cell(spec.marker);
    let px_w = dot_area.width as usize * dx;
    let px_h = dot_area.height as usize * dy;
    let mut cells = vec![GridCell::default(); dot_area.width as usize * dot_area.height as usize];

    let map_y = |value: f64| -> usize {
        let ratio = ((max - value) / (max - min)).clamp(0.0, 1.0);
        (ratio * px_h.saturating_sub(1) as f64).round() as usize
    };

    // Grid lines first (series colors overwrite where they cross).
    if spec.grid && px_h >= 6 {
        let step = nice_step((max - min) / 4.0);
        let mut value = (min / step).ceil() * step;
        while value <= max {
            let py = map_y(value);
            for cx in 0..dot_area.width as usize {
                let px = cx * dx;
                let (cell_x, cell_y) = (px / dx, py / dy);
                if cell_x < dot_area.width as usize && cell_y < dot_area.height as usize {
                    let bit = if matches!(spec.marker, Marker::Braille) {
                        BRAILLE_DOTS[px % 2][py % 4]
                    } else if py.is_multiple_of(2) {
                        0x02
                    } else {
                        0x01
                    };
                    let cell = &mut cells[cell_y * dot_area.width as usize + cell_x];
                    if cell.bits == 0 {
                        cell.bits = bit;
                        cell.color = Some(theme.muted);
                    }
                }
            }
            value += step;
        }
    }

    // Series envelopes.
    for series in spec.series {
        if series.points.is_empty() {
            continue;
        }
        let spans = decimate(&series.points, px_w);
        let mut previous_center: Option<usize> = None;
        for (column, span) in spans.iter().enumerate() {
            let Some((lo, hi)) = span else {
                previous_center = None;
                continue;
            };
            let y_a = map_y(*lo);
            let y_b = map_y(*hi);
            let (top, bottom) = (y_a.min(y_b), y_a.max(y_b));
            if let Some(previous) = previous_center {
                let center = (top + bottom) / 2;
                for row in previous.min(center)..=previous.max(center) {
                    set_pixel(&mut cells, spec.marker, dot_area, column, row, series.color);
                }
            }
            for row in top..=bottom {
                set_pixel(&mut cells, spec.marker, dot_area, column, row, series.color);
            }
            previous_center = Some((top + bottom) / 2);
        }
    }

    // Render dot rows.
    for row in 0..dot_area.height {
        let mut spans: Vec<Span> = Vec::new();
        let mut run = String::new();
        let mut run_color: Option<Color> = None;
        for column in 0..dot_area.width {
            let cell = cells[row as usize * dot_area.width as usize + column as usize];
            let character = cell_char(cell.bits, spec.marker);
            let color = cell.color.unwrap_or(theme.muted);
            if run_color == Some(color) {
                run.push(character);
            } else {
                if !run.is_empty() {
                    let previous = run_color.unwrap_or(color);
                    spans.push(Span::styled(
                        std::mem::take(&mut run),
                        with_dim(Style::new().fg(previous), stale),
                    ));
                }
                run.push(character);
                run_color = Some(color);
            }
        }
        if !run.is_empty() {
            spans.push(Span::styled(
                run,
                with_dim(Style::new().fg(run_color.unwrap_or(theme.text)), stale),
            ));
        }
        let row_area = Rect {
            x: dot_area.x,
            y: dot_area.y + row,
            width: dot_area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(spans)), row_area);
    }

    // Legend line.
    if spec.legend {
        let mut spans: Vec<Span> = Vec::new();
        for (index, series) in spec.series.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                "● ",
                with_dim(Style::new().fg(series.color), stale),
            ));
            spans.push(Span::styled(
                format!("{} ", series.label),
                with_dim(Style::new().fg(theme.muted), stale),
            ));
            spans.push(Span::styled(
                series.latest_text.clone(),
                with_dim(series.latest_style, stale),
            ));
        }
        let legend_area = Rect {
            x: area.x,
            y: dot_area.y + dot_area.height,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(spans)), legend_area);
    }

    // Axis row: window start on the left, "now" on the right.
    if axis_rows == 1 {
        let left = format!("-{}", spec.window_label);
        let right = "now";
        let left_w = unicode_width::UnicodeWidthStr::width(left.as_str());
        let pad = (area.width as usize).saturating_sub(left_w + right.len());
        let text = format!("{left}{}{right}", " ".repeat(pad));
        let axis_area = Rect {
            x: area.x,
            y: dot_area.y + dot_area.height + legend_rows,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                text,
                with_dim(Style::new().fg(theme.muted), stale),
            )),
            axis_area,
        );
    }
}

fn set_pixel(
    cells: &mut [GridCell],
    marker: Marker,
    dot_area: Rect,
    px: usize,
    py: usize,
    color: Color,
) {
    let (dx, dy) = dots_per_cell(marker);
    let (cell_x, cell_y) = (px / dx, py / dy);
    if cell_x >= dot_area.width as usize || cell_y >= dot_area.height as usize {
        return;
    }
    let bit = match marker {
        Marker::Braille => BRAILLE_DOTS[px % 2][py % 4],
        _ => {
            if py.is_multiple_of(2) {
                0x02
            } else {
                0x01
            }
        }
    };
    let cell = &mut cells[cell_y * dot_area.width as usize + cell_x];
    cell.bits |= bit;
    cell.color = Some(color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimate_preserves_spikes() {
        let mut points = vec![Some(2.0); 100];
        points[42] = Some(100.0);
        let spans = decimate(&points, 10);
        assert_eq!(spans.len(), 10);
        assert!(
            spans.iter().flatten().any(|(_, hi)| *hi == 100.0),
            "single-sample spike must survive: {spans:?}"
        );
        assert!(spans
            .iter()
            .flatten()
            .all(|(lo, _)| *lo == 2.0 || *lo == 100.0));
        assert!(spans.iter().flatten().all(|(lo, hi)| lo <= hi));
    }

    #[test]
    fn decimate_keeps_gaps() {
        let points = vec![None, None, Some(1.0), Some(2.0), None, None, None, None];
        let spans = decimate(&points, 4);
        assert_eq!(spans[0], None);
        assert!(spans[1].is_some());
        assert_eq!(spans[3], None);
    }

    #[test]
    fn decimate_right_aligns_short_series() {
        let spans = decimate(&[Some(1.0)], 5);
        assert_eq!(spans.len(), 5);
        assert_eq!(
            spans[..4],
            [None, None, None, None],
            "line grows from the right"
        );
        assert_eq!(spans[4], Some((1.0, 1.0)));
    }

    #[test]
    fn nice_steps_and_bounds() {
        assert_eq!(nice_step(0.9), 1.0);
        assert_eq!(nice_step(1.5), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(12.0), 20.0);
        assert_eq!(nice_step(0.07), 0.1);
        let (lo, hi) = nice_bounds(43.5, 77.3);
        assert_eq!((lo, hi), (40.0, 80.0));
    }

    #[test]
    fn axis_grows_immediately_and_shrinks_with_hysteresis() {
        let mut state = ChartsState::new();
        let t0 = Instant::now();
        // (0..100) padded by 5 % then rounded to the 50-step → (-50, 150).
        let axis = state.axis("k", (0.0, 100.0), 0.0, t0);
        assert_eq!(axis, (-50.0, 150.0));

        // Bigger data → grow right away.
        let axis = state.axis("k", (-100.0, 300.0), 0.0, t0);
        assert!(axis.0 <= -100.0 && axis.1 >= 300.0, "grown: {axis:?}");

        // Much smaller data: no shrink before 5 s.
        let axis = state.axis("k", (0.0, 10.0), 0.0, t0 + Duration::from_secs(1));
        assert!(axis.1 >= 300.0, "still the wide axis: {axis:?}");
        // After 5 s the timer lets it shrink.
        let axis = state.axis("k", (0.0, 10.0), 0.0, t0 + Duration::from_secs(6));
        assert!(axis.1 <= 20.0, "shrunk: {axis:?}");
    }

    #[test]
    fn cell_chars_for_each_marker() {
        assert_eq!(cell_char(0, Marker::Braille), '\u{2800}', "blank braille");
        assert_eq!(cell_char(0x01, Marker::Braille), '⠁');
        assert_eq!(cell_char(0xFF, Marker::Braille), '⣿');
        assert_eq!(cell_char(1, Marker::Halfblock), '▄');
        assert_eq!(cell_char(2, Marker::Halfblock), '▀');
        assert_eq!(cell_char(3, Marker::Halfblock), '█');
        assert_eq!(cell_char(1, Marker::Ascii), '_');
        assert_eq!(cell_char(3, Marker::Ascii), '=');
    }
}
