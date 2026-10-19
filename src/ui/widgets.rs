//! Leaf widgets: gauge, core bars, sensor table, sparkline (SPEC §6.2).
//!
//! Widgets are pure render functions over `Rect` + data, so panel tests can
//! drive them with a `TestBackend` and no network.

use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::theme::{with_dim, Metric, Theme};

const BAR_FULL: char = '█';
const BAR_EMPTY: char = '░';
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn display_width(text: &str) -> u16 {
    unicode_width::UnicodeWidthStr::width(text) as u16
}

/// Truncate to `width` display columns and pad with spaces to exactly that
/// width, so mixed-length labels/bars line up.
pub fn pad_display(text: &str, width: u16) -> String {
    let truncated = truncate(text, width);
    let used = display_width(&truncated);
    let mut out = truncated;
    for _ in used..width {
        out.push(' ');
    }
    out
}

fn truncate(text: &str, width: u16) -> String {
    let mut out = String::new();
    let mut used = 0u16;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16;
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out
}

/// Fixed label column shared by gauges and info rows (SPEC §6.5).
const LABEL_WIDTH: u16 = 8;

/// Label/value column widths of `info_line` pairs (SPEC §6.5).
const INFO_LABEL_WIDTH: u16 = 5;
const INFO_VALUE_WIDTH: u16 = 10;

/// One-line gauge: ` label ███████░░░ display` with a fixed label column and
/// the value right-aligned at the inner edge.
#[allow(clippy::too_many_arguments)]
pub fn gauge(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    display: &str,
    ratio: f64,
    value_style: Style,
    theme: &Theme,
    dim: bool,
) {
    if area.height == 0 || area.width < 14 {
        return;
    }
    let value_width = display_width(display).clamp(3, area.width.saturating_sub(LABEL_WIDTH + 6));
    let bar_width = area.width.saturating_sub(LABEL_WIDTH + value_width + 4);

    let mut spans = vec![Span::styled(
        format!(" {} ", pad_display(label, LABEL_WIDTH)),
        with_dim(Style::new().fg(theme.muted), dim),
    )];
    if bar_width > 0 {
        let filled =
            ((ratio.clamp(0.0, 1.0) * bar_width as f64).round() as u16).clamp(1, bar_width);
        spans.push(Span::styled(
            BAR_FULL.to_string().repeat(filled as usize),
            with_dim(value_style, dim),
        ));
        spans.push(Span::styled(
            BAR_EMPTY.to_string().repeat((bar_width - filled) as usize),
            with_dim(Style::new().fg(theme.muted), dim),
        ));
    }
    spans.push(Span::styled(
        format!(" {display} "),
        with_dim(value_style.bold(), dim),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Label column width for a pair: the shared 5-column pad, widened (never
/// truncated) for longer labels such as `Clock Effective` or `Voltage`.
fn label_width(label: &str) -> u16 {
    display_width(label).max(INFO_LABEL_WIDTH)
}

/// Value column width used by the compact renderer (values are capped).
fn compact_value_width(value: &str) -> u16 {
    display_width(value).min(INFO_VALUE_WIDTH)
}

/// Width of one compact pair, including its label pad and separating space.
fn compact_pair_width(label: &str, value: &str) -> u16 {
    label_width(label) + 1 + compact_value_width(value)
}

/// Fixed-width `label value` pairs for info rows, so columns stay aligned
/// across panels and frames (SPEC §6.5).
pub fn info_line(
    frame: &mut Frame,
    area: Rect,
    pairs: &[(&str, String)],
    theme: &Theme,
    dim: bool,
) {
    if area.height == 0 {
        return;
    }
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (index, (label, value)) in pairs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!("{} ", pad_display(label, label_width(label))),
            with_dim(Style::new().fg(theme.muted), dim),
        ));
        spans.push(Span::styled(
            pad_display(value, INFO_VALUE_WIDTH),
            with_dim(Style::new().fg(theme.text).bold(), dim),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Rendered width of a `compact_pairs` block (no leading/trailing space).
/// Labels keep at least the shared 5-column pad (longer labels keep their full
/// width); values stay natural up to 10 columns.
pub fn compact_pairs_width(pairs: &[(&str, String)]) -> u16 {
    let count = pairs.len() as u16;
    if count == 0 {
        return 0;
    }
    pairs
        .iter()
        .map(|(label, value)| compact_pair_width(label, value))
        .sum::<u16>()
        + (count - 1)
}

fn push_compact_pairs(
    spans: &mut Vec<Span<'static>>,
    pairs: &[(&str, String)],
    theme: &Theme,
    dim: bool,
) {
    for (index, (label, value)) in pairs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!("{} ", pad_display(label, label_width(label))),
            with_dim(Style::new().fg(theme.muted), dim),
        ));
        spans.push(Span::styled(
            truncate(value, INFO_VALUE_WIDTH),
            with_dim(Style::new().fg(theme.text).bold(), dim),
        ));
    }
}

/// Greedy row split for wrapped pair rows: pairs keep display order, and a pair
/// moves to the next row when it no longer fits `width` (a pair wider than the
/// row gets a row of its own and clips).
fn compact_pair_rows(pairs: &[(&str, String)], width: u16) -> Vec<std::ops::Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0usize;
    let mut used = 0u16;
    for (index, (label, value)) in pairs.iter().enumerate() {
        let pair = compact_pair_width(label, value);
        let separator = u16::from(index > start);
        if index > start && used + separator + pair > width {
            rows.push(start..index);
            start = index;
            used = pair;
        } else {
            used += separator + pair;
        }
    }
    if start < pairs.len() {
        rows.push(start..pairs.len());
    }
    rows
}

/// Rows a wrapped pair strip needs at `width` (the height model uses this;
/// the renderer uses the same split so they never disagree).
pub fn info_lines_needed(pairs: &[(&str, String)], width: u16) -> u16 {
    compact_pair_rows(pairs, width).len() as u16
}

/// `label value` pairs wrapped onto consecutive rows of `area` (the CPU
/// metrics strip, SPEC §6.3). Renders at most `area.height` rows.
pub fn info_lines_wrapped(
    frame: &mut Frame,
    area: Rect,
    pairs: &[(&str, String)],
    theme: &Theme,
    dim: bool,
) {
    if area.height == 0 || pairs.is_empty() {
        return;
    }
    let rows = compact_pair_rows(pairs, area.width);
    let mut lines: Vec<Line> = Vec::new();
    for row in rows.into_iter().take(area.height as usize) {
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        push_compact_pairs(&mut spans, &pairs[row], theme, dim);
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// `info_line` with a second block pinned to the right edge of the same row
/// (GPU fans beside power/clock). Both blocks render compactly; callers must
/// size the row with [`compact_pairs_width`] and fall back to
/// [`info_line`] when the two blocks plus a gap column do not fit.
pub fn info_line_split(
    frame: &mut Frame,
    area: Rect,
    left: &[(&str, String)],
    right: &[(&str, String)],
    theme: &Theme,
    dim: bool,
) {
    if area.height == 0 || right.is_empty() {
        return;
    }
    let left_width = u32::from(compact_pairs_width(left));
    let right_width = u32::from(compact_pairs_width(right));
    // One leading and one trailing space, plus at least one gap column.
    let gap = u32::from(area.width).saturating_sub(left_width + right_width + 2);
    let mut spans: Vec<Span> = Vec::new();
    if !left.is_empty() {
        spans.push(Span::raw(" "));
        push_compact_pairs(&mut spans, left, theme, dim);
    }
    spans.push(Span::raw(" ".repeat(gap.max(1) as usize)));
    push_compact_pairs(&mut spans, right, theme, dim);
    spans.push(Span::raw(" "));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Vertical core bars (SPEC §6.2 `bars`), one block glyph per value.
pub fn bars(frame: &mut Frame, area: Rect, values: &[Option<f64>], theme: &Theme, dim: bool) {
    if area.width == 0 || area.height == 0 || values.is_empty() {
        return;
    }
    let per_row = (area.width as usize).saturating_sub(1).max(1);
    let rows = values.len().div_ceil(per_row).min(area.height as usize);
    let mut lines: Vec<Line> = Vec::new();
    for row in 0..rows {
        let slice = &values[row * per_row..((row + 1) * per_row).min(values.len())];
        let mut spans: Vec<Span> = vec![Span::raw(" ")];
        spans.extend(slice.iter().map(|value| match value {
            Some(v) => {
                let level = ((v.clamp(0.0, 100.0) / 100.0) * 8.0).round() as usize;
                Span::styled(
                    BLOCKS[level.min(8)].to_string(),
                    with_dim(theme.value_style(Metric::Load, *v), dim),
                )
            }
            None => Span::styled(
                BLOCKS[0].to_string(),
                with_dim(Style::new().fg(theme.muted), dim),
            ),
        }));
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// Scrollable sensor table (SPEC §6.2 `table`).
#[allow(clippy::too_many_arguments)]
pub fn table(
    frame: &mut Frame,
    area: Rect,
    block: Block<'static>,
    headers: &[&'static str],
    widths: Vec<Constraint>,
    rows: Vec<Row<'static>>,
    scroll: u16,
    theme: &Theme,
    dim: bool,
) {
    let header = Row::new(headers.iter().enumerate().map(|(index, header)| {
        // Numeric columns are right-aligned, so their headers must follow.
        let alignment = if index == 0 {
            Alignment::Left
        } else {
            Alignment::Right
        };
        Cell::from(Text::from(*header).alignment(alignment))
    }))
    .style(with_dim(Style::new().fg(theme.muted), dim));
    let table = Table::new(rows, widths).header(header).block(block);
    let mut state = TableState::default().with_offset(scroll as usize);
    frame.render_stateful_widget(table, area, &mut state);
}

/// Right-aligned cell used by numeric table columns.
pub fn right_cell(text: String, style: Style) -> Cell<'static> {
    Cell::from(Text::from(text).alignment(Alignment::Right)).style(style)
}

/// One-line trend (SPEC §6.2 `sparkline`), manual block glyphs (no history
/// yet — charts arrive in M3).
pub fn sparkline(frame: &mut Frame, area: Rect, values: &[f64], style: Style, dim: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let visible = if values.len() > area.width as usize {
        &values[values.len() - area.width as usize..]
    } else {
        values
    };
    if visible.is_empty() {
        return;
    }
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for value in visible {
        min = min.min(*value);
        max = max.max(*value);
    }
    let span = (max - min).max(f64::EPSILON);
    let text: String = visible
        .iter()
        .map(|value| {
            let level = (((value - min) / span) * 8.0).round() as usize;
            BLOCKS[level.min(8)]
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Span::styled(text, with_dim(style, dim))),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_is_width_aware() {
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("46.9 °C", 20), "46.9 °C");
        assert_eq!(truncate("", 5), "");
    }
}
