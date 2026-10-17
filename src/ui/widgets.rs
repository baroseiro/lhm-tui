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

/// One-line gauge: `label  ███████░░░  display`.
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
    if area.height == 0 || area.width < 10 {
        return;
    }
    let label_width = 14u16.min(area.width / 4);
    let value_width = display_width(display).clamp(3, area.width - 6);
    let bar_width = area.width.saturating_sub(label_width + value_width + 2);

    let mut spans = vec![Span::styled(
        format!(" {} ", truncate(label, label_width.saturating_sub(2))),
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
        format!(" {display}"),
        with_dim(value_style.bold(), dim),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Vertical core bars (SPEC §6.2 `bars`), one block glyph per value.
pub fn bars(frame: &mut Frame, area: Rect, values: &[Option<f64>], theme: &Theme, dim: bool) {
    if area.width == 0 || area.height == 0 || values.is_empty() {
        return;
    }
    let per_row = area.width as usize;
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
    let header = Row::new(headers.iter().map(|header| Cell::from(*header)))
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
