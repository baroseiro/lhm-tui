//! Preset panels (SPEC §6.3): CPU, per-GPU, memory, thermal, fans, power,
//! storage and network.
//!
//! Charts are optional: when `charts.enabled` is true (config or CLI) and
//! history exists, CPU/GPU/memory panels draw stacked metric graphs from the
//! history buffers (nvtop-style, SPEC §7.4); otherwise they fall back to the
//! gauges/bars layout.

use std::time::{Duration, Instant};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row};
use ratatui::Frame;

use crate::bindings::{self, Bindings, CpuBindings, GpuBindings, MemoryBindings, Slot};
use crate::charts::{self, ChartsState, GraphSpec, Series};
use crate::config::{Config, Marker, TableSort, YMode};
use crate::history::History;
use crate::lhm::model::{Sensor, Snapshot};
use crate::theme::{with_dim, Metric, Theme};
use crate::ui::format;
use crate::ui::widgets;

/// Which preset a layout entry expands to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelKind {
    Cpu(usize),
    Gpu(usize),
    Memory,
    Thermal,
    Fans,
    Power,
    Storage,
    Network,
    Custom(String),
}

/// One drawable panel, produced from `ui.layout` + bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelSpec {
    pub id: String,
    pub kind: PanelKind,
    pub scrollable: bool,
}

/// Everything a panel needs at draw time (plus mutable chart scale state).
pub struct PanelCtx<'a> {
    pub snapshot: &'a Snapshot,
    pub bindings: &'a Bindings,
    pub cfg: &'a Config,
    pub theme: &'a Theme,
    pub history: Option<&'a History>,
    pub charts: &'a mut ChartsState,
    pub stale: bool,
    pub ascii: bool,
    pub focused: bool,
    pub sort: TableSort,
    pub scroll: u16,
    pub window: Duration,
    pub window_label: String,
    pub now: Instant,
}

impl PanelCtx<'_> {
    fn charts_on(&self) -> bool {
        self.cfg.charts.enabled && self.history.is_some()
    }

    fn marker(&self) -> Marker {
        if self.ascii {
            Marker::Ascii
        } else {
            self.cfg.charts.marker
        }
    }

    fn window_points(&self) -> usize {
        let poll_ms = self.cfg.server.poll_interval_ms.max(1);
        (self.window.as_millis() / u128::from(poll_ms)).max(1) as usize
    }
}

/// Expand `ui.layout` into the concrete panel list for the current bindings.
pub fn panel_list(cfg: &Config, bindings: &Bindings) -> Vec<PanelSpec> {
    let mut panels = Vec::new();
    for key in &cfg.ui.layout {
        match key.as_str() {
            "cpu" if cfg.panels.cpu.enabled => {
                for index in 0..bindings.cpu.len() {
                    panels.push(PanelSpec {
                        id: format!("cpu:{index}"),
                        kind: PanelKind::Cpu(index),
                        scrollable: false,
                    });
                }
            }
            "gpu" if cfg.panels.gpu.enabled => {
                for index in 0..bindings.gpus.len() {
                    panels.push(PanelSpec {
                        id: format!("gpu:{index}"),
                        kind: PanelKind::Gpu(index),
                        scrollable: false,
                    });
                }
            }
            "memory" if cfg.panels.memory.enabled && bindings.memory.is_some() => {
                panels.push(PanelSpec {
                    id: "memory".to_string(),
                    kind: PanelKind::Memory,
                    scrollable: false,
                });
            }
            "thermal" if cfg.panels.thermal.enabled && !bindings.thermal.is_empty() => {
                panels.push(PanelSpec {
                    id: "thermal".to_string(),
                    kind: PanelKind::Thermal,
                    scrollable: true,
                });
            }
            "fans" if cfg.panels.fans.enabled && !bindings.fans.is_empty() => {
                panels.push(PanelSpec {
                    id: "fans".to_string(),
                    kind: PanelKind::Fans,
                    scrollable: true,
                });
            }
            "power" if cfg.panels.power.enabled && !bindings.power.is_empty() => {
                panels.push(PanelSpec {
                    id: "power".to_string(),
                    kind: PanelKind::Power,
                    scrollable: true,
                });
            }
            "storage" if cfg.panels.storage.enabled && !bindings.storage.is_empty() => {
                panels.push(PanelSpec {
                    id: "storage".to_string(),
                    kind: PanelKind::Storage,
                    scrollable: true,
                });
            }
            "network" if cfg.panels.network.enabled && !bindings.network.is_empty() => {
                panels.push(PanelSpec {
                    id: "network".to_string(),
                    kind: PanelKind::Network,
                    scrollable: true,
                });
            }
            other => {
                if let Some(id) = other.strip_prefix("custom:") {
                    panels.push(PanelSpec {
                        id: other.to_string(),
                        kind: PanelKind::Custom(id.to_string()),
                        scrollable: false,
                    });
                }
            }
        }
    }
    panels
}

/// Height budget for a panel (SPEC §6.1, `ui.max_height` caps tables).
pub fn panel_height(spec: &PanelSpec, bindings: &Bindings, cfg: &Config) -> u16 {
    let max = cfg.ui.max_height.clamp(4, 40) as usize;
    let chart = if cfg.charts.enabled {
        cfg.charts.height as u16
    } else {
        0
    };
    match &spec.kind {
        PanelKind::Cpu(_) => {
            if cfg.charts.enabled {
                chart + 5 // chart + core bars (2) + info (1) + borders
            } else {
                7
            }
        }
        PanelKind::Gpu(_) => {
            if cfg.charts.enabled {
                chart + 4 // chart + VRAM gauge + info + borders
            } else {
                8 // temp, hotspot, mem temp, load, VRAM, info
            }
        }
        PanelKind::Memory => {
            if cfg.charts.enabled {
                chart + 4 // chart + used gauge + virtual line + borders
            } else {
                6
            }
        }
        PanelKind::Thermal => (bindings.thermal.len() + 3).clamp(4, max) as u16,
        PanelKind::Fans => (bindings.fans.len() + 3).clamp(4, max) as u16,
        PanelKind::Power => (bindings.power.len() + 4).clamp(4, max) as u16,
        PanelKind::Storage => (bindings.storage.len() + 3).clamp(4, max) as u16,
        PanelKind::Network => (bindings.network.len() + 3).clamp(4, max) as u16,
        PanelKind::Custom(_) => 5,
    }
}

/// Shrink chart heights first when the terminal is too short, down to a
/// minimum of 3 chart rows; remaining overflow clips naturally.
pub fn fit_heights(
    panels: &[PanelSpec],
    bindings: &Bindings,
    cfg: &Config,
    available: u16,
) -> Vec<u16> {
    let mut heights: Vec<u16> = panels
        .iter()
        .map(|panel| panel_height(panel, bindings, cfg))
        .collect();
    let total: u32 = heights.iter().map(|height| u32::from(*height)).sum();
    if total <= u32::from(available) || !cfg.charts.enabled {
        return heights;
    }
    let mut overflow = total - u32::from(available);
    for (index, spec) in panels.iter().enumerate() {
        if overflow == 0 {
            break;
        }
        if !matches!(
            spec.kind,
            PanelKind::Cpu(_) | PanelKind::Gpu(_) | PanelKind::Memory
        ) {
            continue;
        }
        let fixed = heights[index].saturating_sub(cfg.charts.height as u16);
        let minimum = fixed + MIN_CHART_HEIGHT;
        let reducible = u32::from(heights[index].saturating_sub(minimum));
        let cut = reducible.min(overflow);
        heights[index] -= cut as u16;
        overflow -= cut;
    }
    heights
}

/// Minimum chart rows a squeezed chart panel keeps (SPEC §6.1 responsive rule).
const MIN_CHART_HEIGHT: u16 = 4;

pub fn draw_panel(frame: &mut Frame, area: Rect, spec: &PanelSpec, ctx: &mut PanelCtx) {
    if area.width < 8 || area.height < 3 {
        return;
    }
    match &spec.kind {
        PanelKind::Cpu(index) => {
            if let Some(cpu) = ctx.bindings.cpu.get(*index).cloned() {
                draw_cpu(frame, area, &spec.id, &cpu, ctx);
            }
        }
        PanelKind::Gpu(index) => {
            if let Some(gpu) = ctx.bindings.gpus.get(*index).cloned() {
                draw_gpu(frame, area, &spec.id, *index, &gpu, ctx);
            }
        }
        PanelKind::Memory => {
            if let Some(memory) = ctx.bindings.memory.clone() {
                draw_memory(frame, area, &memory, ctx);
            }
        }
        PanelKind::Thermal => draw_thermal(frame, area, ctx),
        PanelKind::Fans => draw_fans(frame, area, ctx),
        PanelKind::Power => draw_power(frame, area, ctx),
        PanelKind::Storage => draw_storage(frame, area, ctx),
        PanelKind::Network => draw_network(frame, area, ctx),
        PanelKind::Custom(id) => {
            let block = panel_block(format!("custom:{id}"), ctx.theme, ctx.stale, ctx.focused);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "custom panels land in M4 (T4.2)",
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ))),
                inner,
            );
        }
    }
}

/// Focused panels get the accent border; stale data is dimmed (SPEC §6.5).
fn panel_block(title: String, theme: &Theme, stale: bool, focused: bool) -> Block<'static> {
    let border = if focused { theme.accent } else { theme.muted };
    let title_style = if focused {
        Style::new().fg(theme.accent).bold()
    } else {
        Style::new().fg(theme.muted)
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "))
        .border_style(with_dim(Style::new().fg(border), stale))
        .title_style(with_dim(title_style, stale))
}

fn slot_value<'a>(snapshot: &'a Snapshot, slot: &Option<Slot>) -> Option<(&'a Sensor, f64)> {
    let slot = slot.as_ref()?;
    let sensor = bindings::slot_sensor(snapshot, slot)?;
    sensor.value.map(|value| (sensor, value))
}

/// Display string for a slot: `n/a` when unbound, `—` when valueless.
fn slot_display(snapshot: &Snapshot, slot: &Option<Slot>) -> String {
    match slot {
        None => "n/a".to_string(),
        Some(slot) => match bindings::slot_sensor(snapshot, slot) {
            Some(sensor) => format::sensor_value(sensor),
            None => "n/a".to_string(),
        },
    }
}

fn distinct_label(label: &str, hardware_name: &str, duplicated: bool) -> String {
    if duplicated {
        format!("{label} ({hardware_name})")
    } else {
        label.to_string()
    }
}

/// Gauge for a slot; unbound/valueless slots render `n/a` so rows keep their
/// fixed positions (no shifting layout when a sensor disappears).
fn gauge_for_slot(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    ctx: &PanelCtx,
    slot: &Option<Slot>,
    max: f64,
    metric: Metric,
) {
    match slot_value(ctx.snapshot, slot) {
        Some((sensor, value)) => widgets::gauge(
            frame,
            area,
            label,
            &format::sensor_value(sensor),
            value / max,
            ctx.theme.value_style(metric, value),
            ctx.theme,
            ctx.stale,
        ),
        None => widgets::gauge(
            frame,
            area,
            label,
            "n/a",
            0.0,
            with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
            ctx.theme,
            ctx.stale,
        ),
    }
}

/// Build a chart series from a bound slot (history + live legend value).
fn series_for(ctx: &PanelCtx, slot: &Option<Slot>, color: Color) -> Option<Series> {
    let slot = slot.as_ref()?;
    let sensor = bindings::slot_sensor(ctx.snapshot, slot)?;
    let points = ctx
        .history
        .map(|history| history.window(&slot.sensor_id, ctx.window_points()))
        .unwrap_or_default();
    Some(Series {
        label: slot.label.clone(),
        color,
        latest_text: format::sensor_value(sensor),
        latest_style: ctx.theme.sensor_style(&sensor.kind, sensor.value),
        points,
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_metric_graph(
    frame: &mut Frame,
    area: Rect,
    ctx: &mut PanelCtx,
    key: String,
    series: Vec<Series>,
    y_mode: YMode,
) {
    if area.height == 0 || area.width < 8 || series.is_empty() {
        return;
    }
    let y_fixed = match (ctx.cfg.charts.y_min, ctx.cfg.charts.y_max) {
        (Some(min), Some(max)) => Some((min, max)),
        _ => None,
    };
    let spec = GraphSpec {
        series: &series,
        scale_key: key,
        y_mode,
        y_fixed,
        min_span: ctx.cfg.charts.min_span,
        marker: ctx.marker(),
        grid: ctx.cfg.charts.grid,
        legend: ctx.cfg.charts.legend,
        window_label: ctx.window_label.clone(),
    };
    charts::draw_graph(
        frame, area, &spec, ctx.charts, ctx.theme, ctx.stale, ctx.now,
    );
}

// ------------------------------------------------------------------ CPU

fn draw_cpu(frame: &mut Frame, area: Rect, panel_key: &str, cpu: &CpuBindings, ctx: &mut PanelCtx) {
    let block = panel_block(
        format!("CPU · {}", cpu.name),
        ctx.theme,
        ctx.stale,
        ctx.focused,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if ctx.charts_on() && inner.height >= MIN_CHART_HEIGHT + 3 {
        // Chart block (load graph over temp graph), core bars, info line.
        let chart_height = inner.height.saturating_sub(3).max(MIN_CHART_HEIGHT);
        let sections = Layout::vertical([
            Constraint::Length(chart_height),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(inner);
        let graphs =
            Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(sections[0]);

        let load_series: Vec<Series> = series_for(ctx, &cpu.load_total, ctx.theme.cpu)
            .into_iter()
            .collect();
        draw_metric_graph(
            frame,
            graphs[0],
            ctx,
            format!("{panel_key}:load"),
            load_series,
            YMode::Percent,
        );
        let temp_series: Vec<Series> = series_for(ctx, &cpu.temp, ctx.theme.warn)
            .into_iter()
            .collect();
        draw_metric_graph(
            frame,
            graphs[1],
            ctx,
            format!("{panel_key}:temp"),
            temp_series,
            YMode::Auto,
        );

        let core_values: Vec<Option<f64>> = cpu
            .core_loads
            .iter()
            .map(|slot| bindings::slot_sensor(ctx.snapshot, slot).and_then(|sensor| sensor.value))
            .collect();
        widgets::bars(frame, sections[1], &core_values, ctx.theme, ctx.stale);

        let pairs: Vec<(&str, String)> = [("Clock", &cpu.clock), ("Power", &cpu.power)]
            .into_iter()
            .filter(|(_, slot)| slot.is_some())
            .map(|(label, slot)| (label, slot_display(ctx.snapshot, slot)))
            .collect();
        widgets::info_line(frame, sections[2], &pairs, ctx.theme, ctx.stale);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas::<4>(inner);

    gauge_for_slot(
        frame,
        rows[0],
        "Temp",
        ctx,
        &cpu.temp,
        100.0,
        Metric::Temperature,
    );
    gauge_for_slot(
        frame,
        rows[1],
        "Load",
        ctx,
        &cpu.load_total,
        100.0,
        Metric::Load,
    );

    let core_values: Vec<Option<f64>> = cpu
        .core_loads
        .iter()
        .map(|slot| bindings::slot_sensor(ctx.snapshot, slot).and_then(|sensor| sensor.value))
        .collect();
    widgets::bars(frame, rows[2], &core_values, ctx.theme, ctx.stale);

    let pairs: Vec<(&str, String)> = [("Clock", &cpu.clock), ("Power", &cpu.power)]
        .into_iter()
        .filter(|(_, slot)| slot.is_some())
        .map(|(label, slot)| (label, slot_display(ctx.snapshot, slot)))
        .collect();
    widgets::info_line(frame, rows[3], &pairs, ctx.theme, ctx.stale);
}

// ------------------------------------------------------------------ GPU

fn draw_gpu(
    frame: &mut Frame,
    area: Rect,
    panel_key: &str,
    index: usize,
    gpu: &GpuBindings,
    ctx: &mut PanelCtx,
) {
    let block = panel_block(
        format!("GPU {index} · {}", gpu.name),
        ctx.theme,
        ctx.stale,
        ctx.focused,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if ctx.charts_on() && inner.height >= MIN_CHART_HEIGHT + 2 {
        // Load graph (load + memory %) over temperature graph (core/hotspot/
        // memory junction), then VRAM gauge and info line.
        let chart_height = inner.height.saturating_sub(2).max(MIN_CHART_HEIGHT);
        let sections = Layout::vertical([
            Constraint::Length(chart_height),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);
        let graphs =
            Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(sections[0]);

        let load_series: Vec<Series> = [
            series_for(ctx, &gpu.load, ctx.theme.gpu),
            series_for(ctx, &gpu.mem_load, ctx.theme.memory),
        ]
        .into_iter()
        .flatten()
        .collect();
        draw_metric_graph(
            frame,
            graphs[0],
            ctx,
            format!("{panel_key}:load"),
            load_series,
            YMode::Percent,
        );
        let temp_series: Vec<Series> = [
            series_for(ctx, &gpu.temp, ctx.theme.warn),
            series_for(ctx, &gpu.hotspot, ctx.theme.crit),
            series_for(ctx, &gpu.mem_temp, ctx.theme.network),
        ]
        .into_iter()
        .flatten()
        .collect();
        draw_metric_graph(
            frame,
            graphs[1],
            ctx,
            format!("{panel_key}:temp"),
            temp_series,
            YMode::Auto,
        );

        draw_vram_gauge(frame, sections[1], ctx, gpu);
        let pairs: Vec<(&str, String)> = [
            ("Power", &gpu.power),
            ("Fan", &gpu.fan),
            ("Clock", &gpu.clock),
        ]
        .into_iter()
        .filter(|(_, slot)| slot.is_some())
        .map(|(label, slot)| (label, slot_display(ctx.snapshot, slot)))
        .collect();
        widgets::info_line(frame, sections[2], &pairs, ctx.theme, ctx.stale);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<6>(inner);

    gauge_for_slot(
        frame,
        rows[0],
        "Temp",
        ctx,
        &gpu.temp,
        100.0,
        Metric::Temperature,
    );
    gauge_for_slot(
        frame,
        rows[1],
        "Hot Spot",
        ctx,
        &gpu.hotspot,
        100.0,
        Metric::Temperature,
    );
    gauge_for_slot(
        frame,
        rows[2],
        "Mem Temp",
        ctx,
        &gpu.mem_temp,
        100.0,
        Metric::Temperature,
    );
    gauge_for_slot(frame, rows[3], "Load", ctx, &gpu.load, 100.0, Metric::Load);
    draw_vram_gauge(frame, rows[4], ctx, gpu);

    let pairs: Vec<(&str, String)> = [
        ("Power", &gpu.power),
        ("Fan", &gpu.fan),
        ("Clock", &gpu.clock),
    ]
    .into_iter()
    .filter(|(_, slot)| slot.is_some())
    .map(|(label, slot)| (label, slot_display(ctx.snapshot, slot)))
    .collect();
    widgets::info_line(frame, rows[5], &pairs, ctx.theme, ctx.stale);
}

fn draw_vram_gauge(frame: &mut Frame, area: Rect, ctx: &PanelCtx, gpu: &GpuBindings) {
    let used = slot_value(ctx.snapshot, &gpu.mem_used);
    let total = slot_value(ctx.snapshot, &gpu.mem_total);
    match used {
        Some((used_sensor, used_value)) => {
            let (display, ratio) = match total {
                Some((_, total_value)) if total_value > 0.0 => (
                    format::data_ratio(&used_sensor.kind, used_value, total_value),
                    used_value / total_value,
                ),
                _ => (format::sensor_value(used_sensor), f64::NAN),
            };
            let fill_ratio = if ratio.is_nan() {
                0.0
            } else {
                ratio.clamp(0.0, 1.0)
            };
            let style = if ratio.is_nan() {
                ctx.theme.value_style(Metric::GpuMemory, 0.0)
            } else {
                ctx.theme.value_style(Metric::GpuMemory, fill_ratio * 100.0)
            };
            widgets::gauge(
                frame, area, "VRAM", &display, fill_ratio, style, ctx.theme, ctx.stale,
            );
        }
        None => {
            widgets::gauge(
                frame,
                area,
                "VRAM",
                "n/a",
                0.0,
                with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ctx.theme,
                ctx.stale,
            );
        }
    }
}

// ------------------------------------------------------------------ memory

fn draw_memory(frame: &mut Frame, area: Rect, memory: &MemoryBindings, ctx: &mut PanelCtx) {
    let block = panel_block(
        format!("Memory · {}", memory.name),
        ctx.theme,
        ctx.stale,
        ctx.focused,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if ctx.charts_on() && inner.height >= MIN_CHART_HEIGHT + 2 {
        let chart_height = inner.height.saturating_sub(2).max(MIN_CHART_HEIGHT);
        let sections = Layout::vertical([
            Constraint::Length(chart_height),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

        let usage = memory_usage_series(ctx, memory);
        draw_metric_graph(
            frame,
            sections[0],
            ctx,
            "memory:usage".to_string(),
            usage.into_iter().collect(),
            YMode::Percent,
        );

        draw_memory_used_gauge(frame, sections[1], ctx, memory);
        draw_virtual_line(frame, sections[2], ctx, memory);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<4>(inner);

    draw_memory_used_gauge(frame, rows[0], ctx, memory);
    gauge_for_slot(
        frame,
        rows[1],
        "Load",
        ctx,
        &memory.load,
        100.0,
        Metric::Load,
    );
    draw_virtual_line(frame, rows[2], ctx, memory);
}

/// Memory usage percentage series: the `Memory` load sensor when present,
/// else synthesized from used/(used+available) history.
fn memory_usage_series(ctx: &PanelCtx, memory: &MemoryBindings) -> Option<Series> {
    if let Some(series) = series_for(ctx, &memory.load, ctx.theme.memory) {
        return Some(series);
    }
    let used_slot = memory.used.as_ref()?;
    let available_slot = memory.available.as_ref()?;
    let history = ctx.history?;
    let points = ctx.window_points();
    let used_history = history.window(&used_slot.sensor_id, points);
    let available_history = history.window(&available_slot.sensor_id, points);
    let len = used_history.len().min(available_history.len());
    if len == 0 {
        return None;
    }
    let series_points: Vec<Option<f32>> = used_history[used_history.len() - len..]
        .iter()
        .zip(&available_history[available_history.len() - len..])
        .map(|(used, available)| match (used, available) {
            (Some(used), Some(available)) if used + available > 0.0 => {
                Some(used / (used + available) * 100.0)
            }
            _ => None,
        })
        .collect();

    let used_sensor = bindings::slot_sensor(ctx.snapshot, used_slot)?;
    let (used_value, available_value) = (
        used_sensor.value?,
        slot_value(ctx.snapshot, &memory.available)?.1,
    );
    let percent = used_value / (used_value + available_value) * 100.0;
    Some(Series {
        label: "Usage".to_string(),
        color: ctx.theme.memory,
        latest_text: format::percent(percent),
        latest_style: ctx.theme.value_style(Metric::Load, percent),
        points: series_points,
    })
}

fn draw_memory_used_gauge(frame: &mut Frame, area: Rect, ctx: &PanelCtx, memory: &MemoryBindings) {
    let used = slot_value(ctx.snapshot, &memory.used);
    let available = slot_value(ctx.snapshot, &memory.available);
    match used {
        Some((used_sensor, used_value)) => {
            let (display, ratio) = match available {
                Some((_, available_value)) if used_value + available_value > 0.0 => (
                    format::data_ratio(&used_sensor.kind, used_value, used_value + available_value),
                    used_value / (used_value + available_value),
                ),
                _ => (format::sensor_value(used_sensor), f64::NAN),
            };
            let fill_ratio = if ratio.is_nan() {
                0.0
            } else {
                ratio.clamp(0.0, 1.0)
            };
            widgets::gauge(
                frame,
                area,
                "Used",
                &display,
                fill_ratio,
                ctx.theme.value_style(Metric::GpuMemory, fill_ratio * 100.0),
                ctx.theme,
                ctx.stale,
            );
        }
        None => {
            widgets::gauge(
                frame,
                area,
                "Used",
                "n/a",
                0.0,
                with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ctx.theme,
                ctx.stale,
            );
        }
    }
}

fn draw_virtual_line(frame: &mut Frame, area: Rect, ctx: &PanelCtx, memory: &MemoryBindings) {
    let virtual_used = slot_value(ctx.snapshot, &memory.virtual_used);
    let virtual_available = slot_value(ctx.snapshot, &memory.virtual_available);
    let Some((used_sensor, used_value)) = virtual_used else {
        return;
    };
    let display = match virtual_available {
        Some((_, available_value)) => {
            format::data_ratio(&used_sensor.kind, used_value, used_value + available_value)
        }
        None => format::sensor_value(used_sensor),
    };
    let spans = vec![
        Span::raw(" "),
        Span::styled(
            format!("{} ", widgets::pad_display("Virtual", 8)),
            with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
        ),
        Span::styled(
            display,
            with_dim(Style::new().fg(ctx.theme.text).bold(), ctx.stale),
        ),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ------------------------------------------------------------------ tables

fn duplicate_labels(labels: &[String]) -> Vec<bool> {
    let mut duplicated = vec![false; labels.len()];
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            if labels[i] == labels[j] {
                duplicated[i] = true;
                duplicated[j] = true;
            }
        }
    }
    duplicated
}

fn sort_table_rows(sort: TableSort, rows: &mut Vec<(Option<f64>, String, Row<'static>)>) {
    let by_value = |a: Option<f64>, b: Option<f64>, descending: bool| match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    match sort {
        TableSort::ValueDesc => rows.sort_by(|a, b| by_value(a.0, b.0, true)),
        TableSort::ValueAsc => rows.sort_by(|a, b| by_value(a.0, b.0, false)),
        TableSort::Name => rows.sort_by_key(|row| row.1.to_ascii_lowercase()),
        TableSort::Unspecified => {}
    }
}

fn draw_thermal(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block(
        "Temperatures".to_string(),
        ctx.theme,
        ctx.stale,
        ctx.focused,
    );
    let labels: Vec<String> = ctx
        .bindings
        .thermal
        .iter()
        .map(|slot| slot.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = ctx
        .bindings
        .thermal
        .iter()
        .zip(duplicated)
        .filter_map(|(slot, duplicated)| {
            let sensor = bindings::slot_sensor(ctx.snapshot, slot)?;
            let hardware_name = ctx
                .snapshot
                .hardware(&slot.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&slot.label, &hardware_name, duplicated);
            let style = with_dim(
                ctx.theme.sensor_style(&sensor.kind, sensor.value),
                ctx.stale,
            );
            let row = Row::new(vec![
                Cell::from(name.clone()),
                widgets::right_cell(format::sensor_value(sensor), style),
                widgets::right_cell(
                    format::stat(&sensor.kind, sensor.min),
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ),
                widgets::right_cell(
                    format::stat(&sensor.kind, sensor.max),
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ),
            ])
            .style(style);
            Some((sensor.value, name, row))
        })
        .collect();

    sort_table_rows(ctx.sort, &mut rows);
    let rows: Vec<Row<'static>> = rows.into_iter().map(|(_, _, row)| row).collect();
    let widths = vec![
        Constraint::Min(12),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(9),
    ];
    widgets::table(
        frame,
        area,
        block,
        &["Sensor", "Value", "Min", "Max"],
        widths,
        rows,
        ctx.scroll,
        ctx.theme,
        ctx.stale,
    );
}

fn draw_fans(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block("Fans".to_string(), ctx.theme, ctx.stale, ctx.focused);
    let labels: Vec<String> = ctx
        .bindings
        .fans
        .iter()
        .map(|row| row.fan.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = ctx
        .bindings
        .fans
        .iter()
        .zip(duplicated)
        .map(|(fan, duplicated)| {
            let hardware_name = ctx
                .snapshot
                .hardware(&fan.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&fan.fan.label, &hardware_name, duplicated);
            let value = slot_value(ctx.snapshot, &Some(fan.fan.clone()));
            let row = Row::new(vec![
                Cell::from(name.clone()),
                widgets::right_cell(
                    slot_display(ctx.snapshot, &Some(fan.fan.clone())),
                    with_dim(Style::new().fg(ctx.theme.text), ctx.stale),
                ),
                widgets::right_cell(
                    slot_display(ctx.snapshot, &fan.control),
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ),
            ]);
            (value.map(|(_, value)| value), name, row)
        })
        .collect();

    sort_table_rows(ctx.sort, &mut rows);
    let rows: Vec<Row<'static>> = rows.into_iter().map(|(_, _, row)| row).collect();
    let widths = vec![
        Constraint::Min(12),
        Constraint::Length(12),
        Constraint::Length(10),
    ];
    widgets::table(
        frame,
        area,
        block,
        &["Fan", "Speed", "Control"],
        widths,
        rows,
        ctx.scroll,
        ctx.theme,
        ctx.stale,
    );
}

fn draw_power(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block("Power".to_string(), ctx.theme, ctx.stale, ctx.focused);
    let labels: Vec<String> = ctx
        .bindings
        .power
        .iter()
        .map(|slot| slot.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = ctx
        .bindings
        .power
        .iter()
        .zip(duplicated)
        .filter_map(|(slot, duplicated)| {
            let sensor = bindings::slot_sensor(ctx.snapshot, slot)?;
            let hardware_name = ctx
                .snapshot
                .hardware(&slot.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&slot.label, &hardware_name, duplicated);
            let style = with_dim(Style::new().fg(ctx.theme.text), ctx.stale);
            let row = Row::new(vec![
                Cell::from(name.clone()),
                widgets::right_cell(format::sensor_value(sensor), style),
            ])
            .style(style);
            Some((sensor.value, name, row))
        })
        .collect();

    sort_table_rows(ctx.sort, &mut rows);
    let mut rows: Vec<Row<'static>> = rows.into_iter().map(|(_, _, row)| row).collect();

    // Total row: CPU package + GPU packages where available.
    let mut total = 0.0f64;
    let mut have_total = false;
    for slot in ctx
        .bindings
        .cpu
        .iter()
        .filter_map(|cpu| cpu.power.as_ref())
        .chain(
            ctx.bindings
                .gpus
                .iter()
                .filter_map(|gpu| gpu.power.as_ref()),
        )
    {
        if let Some(sensor) = bindings::slot_sensor(ctx.snapshot, slot) {
            if let Some(value) = sensor.value {
                total += value;
                have_total = true;
            }
        }
    }
    if have_total {
        let style = with_dim(Style::new().fg(ctx.theme.accent).bold(), ctx.stale);
        rows.insert(
            0,
            Row::new(vec![
                Cell::from("Total (CPU+GPU)").style(style),
                widgets::right_cell(format::power_w(total), style),
            ])
            .style(style),
        );
    }

    let widths = vec![Constraint::Min(14), Constraint::Length(12)];
    widgets::table(
        frame,
        area,
        block,
        &["Sensor", "Power"],
        widths,
        rows,
        ctx.scroll,
        ctx.theme,
        ctx.stale,
    );
}

fn draw_storage(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block("Storage".to_string(), ctx.theme, ctx.stale, ctx.focused);
    let text_style = with_dim(Style::new().fg(ctx.theme.text), ctx.stale);
    let muted = with_dim(Style::new().fg(ctx.theme.muted), ctx.stale);

    let rows: Vec<Row<'static>> = ctx
        .bindings
        .storage
        .iter()
        .map(|drive| {
            let temp_style = drive
                .temperature
                .as_ref()
                .and_then(|slot| bindings::slot_sensor(ctx.snapshot, slot))
                .map(|sensor| {
                    with_dim(
                        ctx.theme.sensor_style(&sensor.kind, sensor.value),
                        ctx.stale,
                    )
                })
                .unwrap_or(muted);
            Row::new(vec![
                Cell::from(drive.name.clone()).style(text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &drive.temperature), temp_style),
                widgets::right_cell(slot_display(ctx.snapshot, &drive.used_space), text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &drive.activity), text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &drive.read_rate), muted),
                widgets::right_cell(slot_display(ctx.snapshot, &drive.write_rate), muted),
            ])
        })
        .collect();

    let widths = vec![
        Constraint::Min(14),
        Constraint::Length(9),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(11),
        Constraint::Length(11),
    ];
    widgets::table(
        frame,
        area,
        block,
        &["Drive", "Temp", "Used", "Activity", "Read", "Write"],
        widths,
        rows,
        ctx.scroll,
        ctx.theme,
        ctx.stale,
    );
}

fn draw_network(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block("Network".to_string(), ctx.theme, ctx.stale, ctx.focused);
    let text_style = with_dim(Style::new().fg(ctx.theme.text), ctx.stale);
    let muted = with_dim(Style::new().fg(ctx.theme.muted), ctx.stale);

    let rows: Vec<Row<'static>> = ctx
        .bindings
        .network
        .iter()
        .map(|adapter| {
            Row::new(vec![
                Cell::from(adapter.name.clone()).style(text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &adapter.upload), text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &adapter.download), text_style),
                widgets::right_cell(slot_display(ctx.snapshot, &adapter.uploaded), muted),
                widgets::right_cell(slot_display(ctx.snapshot, &adapter.downloaded), muted),
            ])
        })
        .collect();

    let widths = vec![
        Constraint::Min(14),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(11),
    ];
    widgets::table(
        frame,
        area,
        block,
        &["Adapter", "Upload", "Download", "Up total", "Down total"],
        widths,
        rows,
        ctx.scroll,
        ctx.theme,
        ctx.stale,
    );
}
