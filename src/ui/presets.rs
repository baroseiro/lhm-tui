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
use ratatui::widgets::{Block, BorderType, Cell, Row};
use ratatui::Frame;

use crate::bindings::{self, Bindings, CpuBindings, FanRow, GpuBindings, MemoryBindings, Slot};
use crate::charts::{self, ChartsState, GraphSpec, Series};
use crate::config::{Config, CustomPanel, CustomWidget, Marker, TableSort, WidgetKind, YMode};
use crate::history::History;
use crate::lhm::model::{Sensor, SensorKind, Snapshot};
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
    Voltage,
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
    /// Effective charts flag (`charts.enabled` with the runtime `c` override).
    pub charts_enabled: bool,
    pub stale: bool,
    pub ascii: bool,
    pub focused: bool,
    pub sort: TableSort,
    pub scroll: u16,
    pub window: Duration,
    pub window_label: String,
    pub now: Instant,
    /// Height decisions shared with the height model (`panel_layouts`).
    pub layout: PanelLayout,
    /// Selected controllable fan row (index into the controllable fan list).
    pub fan_selected: usize,
    /// True when the panel owns the whole body (`enter` zoom): the voltages
    /// table then also shows per-core rails (SPEC §6.3).
    pub zoomed: bool,
}

impl PanelCtx<'_> {
    fn charts_on(&self) -> bool {
        self.charts_enabled && self.history.is_some()
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

/// Runtime panel-visibility overrides (the `t`/`v` keys).
#[derive(Debug, Clone, Copy, Default)]
pub struct PanelOverrides {
    /// `t` override for the temperatures table; `None` → membership in
    /// `ui.layout` decides.
    pub thermal: Option<bool>,
    /// `v` override for the voltages table.
    pub voltage: Option<bool>,
    /// `f` override for the fans & controls table.
    pub fans: Option<bool>,
}

/// Per-panel layout decisions that depend on the current readings. Computed
/// once per frame in `render_panels` and shared by the height model and the
/// draw code, so they can never disagree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelLayout {
    /// CPU: rows the bottom metrics strip occupies (0 = strip hidden).
    pub strip_rows: u8,
}

/// Expand `ui.layout` into the concrete panel list for the current bindings.
pub fn panel_list(cfg: &Config, bindings: &Bindings, overrides: PanelOverrides) -> Vec<PanelSpec> {
    let layout_has_thermal = cfg.ui.layout.iter().any(|key| key == "thermal");
    let thermal_visible = overrides.thermal.unwrap_or(layout_has_thermal)
        && cfg.panels.thermal.enabled
        && !bindings.thermal.is_empty();
    let layout_has_voltage = cfg.ui.layout.iter().any(|key| key == "voltage");
    let voltage_visible = overrides.voltage.unwrap_or(layout_has_voltage)
        && cfg.panels.voltage.enabled
        && !bindings.voltage.is_empty();
    let layout_has_fans = cfg.ui.layout.iter().any(|key| key == "fans");
    let fans_visible = overrides.fans.unwrap_or(layout_has_fans)
        && cfg.panels.fans.enabled
        && !bindings.fans.is_empty();

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
            "thermal" if thermal_visible => {
                panels.push(PanelSpec {
                    id: "thermal".to_string(),
                    kind: PanelKind::Thermal,
                    scrollable: true,
                });
            }
            "voltage" if voltage_visible => {
                panels.push(PanelSpec {
                    id: "voltage".to_string(),
                    kind: PanelKind::Voltage,
                    scrollable: true,
                });
            }
            "fans" if fans_visible => {
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
                    let scrollable = cfg
                        .panels
                        .custom
                        .iter()
                        .find(|panel| panel.id == id)
                        .is_some_and(|panel| {
                            panel
                                .widgets
                                .iter()
                                .any(|widget| widget.kind == WidgetKind::Table)
                        });
                    panels.push(PanelSpec {
                        id: other.to_string(),
                        kind: PanelKind::Custom(id.to_string()),
                        scrollable,
                    });
                }
            }
        }
    }

    // `t` / `v` / `f` can reveal the tables even when they are not in ui.layout.
    if thermal_visible && !layout_has_thermal {
        panels.push(PanelSpec {
            id: "thermal".to_string(),
            kind: PanelKind::Thermal,
            scrollable: true,
        });
    }
    if voltage_visible && !layout_has_voltage {
        panels.push(PanelSpec {
            id: "voltage".to_string(),
            kind: PanelKind::Voltage,
            scrollable: true,
        });
    }
    if fans_visible && !layout_has_fans {
        panels.push(PanelSpec {
            id: "fans".to_string(),
            kind: PanelKind::Fans,
            scrollable: true,
        });
    }
    panels
}

/// Group panel indices into layout rows: consecutive GPU panels share one row
/// while `per_row` allows it (SPEC §6.1). Shared by height fitting and
/// rendering so the two can never disagree.
pub fn layout_rows(panels: &[PanelSpec], per_row: usize) -> Vec<Vec<usize>> {
    let per_row = per_row.max(1);
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut index = 0;
    while index < panels.len() {
        if matches!(panels[index].kind, PanelKind::Gpu(_)) {
            let mut group = vec![index];
            let mut next = index + 1;
            while group.len() < per_row
                && next < panels.len()
                && matches!(panels[next].kind, PanelKind::Gpu(_))
            {
                group.push(next);
                next += 1;
            }
            rows.push(group);
            index = next;
        } else {
            rows.push(vec![index]);
            index += 1;
        }
    }
    rows
}

/// Height budget for a panel (SPEC §6.1, `ui.max_height` caps tables).
/// `layout` carries the CPU strip row count.
pub fn panel_height(
    spec: &PanelSpec,
    bindings: &Bindings,
    cfg: &Config,
    charts_on: bool,
    layout: PanelLayout,
) -> u16 {
    let max = cfg.ui.max_height.clamp(4, 40) as usize;
    let chart = if charts_on {
        cfg.charts.height as u16
    } else {
        0
    };
    match &spec.kind {
        PanelKind::Cpu(_) => {
            let show = &cfg.panels.cpu.show;
            let bars = if shows(show, "core_loads") { 2 } else { 0 };
            let info = u16::from(layout.strip_rows);
            let content = if charts_on {
                let graphs = u16::from(shows(show, "load")) + u16::from(shows(show, "temp"));
                if graphs > 0 {
                    chart + bars + info
                } else {
                    bars + info
                }
            } else {
                u16::from(shows(show, "temp")) + u16::from(shows(show, "load")) + bars + info
            };
            (content + 2).max(3)
        }
        PanelKind::Gpu(index) => {
            let show = &cfg.panels.gpu.show;
            let has_fans = shows(show, "fan")
                && bindings
                    .gpus
                    .get(*index)
                    .is_some_and(|gpu| !gpu.fans.is_empty());
            let info =
                u16::from(shows(show, "power") || shows(show, "clock") || shows(show, "voltage"));
            let fans = u16::from(has_fans);
            let vram = u16::from(shows(show, "memory"));
            let shared = u16::from(
                shows(show, "memory")
                    && bindings
                        .gpus
                        .get(*index)
                        .is_some_and(|gpu| gpu.shared_used.is_some()),
            );
            let content = if charts_on {
                let graphs = u16::from(shows(show, "load") || shows(show, "memory"))
                    + u16::from(shows(show, "temp"));
                if graphs > 0 {
                    chart + vram + shared + info + fans
                } else {
                    vram + shared + info + fans
                }
            } else {
                let temps = if shows(show, "temp") { 3 } else { 0 };
                let load = u16::from(shows(show, "load"));
                temps + load + vram + shared + info + fans
            };
            (content + 2).max(3)
        }
        PanelKind::Memory => {
            let show = &cfg.panels.memory.show;
            let used = u16::from(shows(show, "used"));
            let virtual_row = u16::from(shows(show, "virtual"));
            let content = if charts_on {
                if shows(show, "load") {
                    chart + used + virtual_row
                } else {
                    used + virtual_row
                }
            } else {
                used + u16::from(shows(show, "load")) + virtual_row
            };
            (content + 2).max(3)
        }
        PanelKind::Thermal => {
            let groups = hardware_groups(
                bindings
                    .thermal
                    .iter()
                    .map(|slot| slot.hardware_id.as_str()),
            );
            (bindings.thermal.len() + groups + 3).clamp(4, max) as u16
        }
        PanelKind::Voltage => {
            let slots = voltage_table_slots(&bindings.voltage, false);
            let groups = hardware_groups(slots.iter().map(|slot| slot.hardware_id.as_str()));
            (slots.len() + groups + 3).clamp(4, max) as u16
        }
        PanelKind::Fans => (bindings.fans.len() + 3).clamp(4, max) as u16,
        PanelKind::Power => (bindings.power.len() + 4).clamp(4, max) as u16,
        PanelKind::Storage => (bindings.storage.len() + 3).clamp(4, max) as u16,
        PanelKind::Network => (bindings.network.len() + 3).clamp(4, max) as u16,
        PanelKind::Custom(id) => match cfg.panels.custom.iter().find(|panel| panel.id == *id) {
            Some(panel) => {
                let widgets: u16 = panel
                    .widgets
                    .iter()
                    .map(|widget| custom_widget_height(widget.kind, cfg, charts_on))
                    .sum();
                (widgets + 2).max(3)
            }
            None => 5,
        },
    }
}

/// Number of distinct hardware devices in a table (group header rows).
fn hardware_groups<'a>(ids: impl Iterator<Item = &'a str>) -> usize {
    ids.collect::<std::collections::BTreeSet<_>>().len()
}

/// `(label, value)` pairs borrowed as the slice shape the info widgets take.
fn pair_refs(pairs: &[(String, String)]) -> Vec<(&str, String)> {
    pairs
        .iter()
        .map(|(label, value)| (label.as_str(), value.clone()))
        .collect()
}

/// GPU info pairs in display order: `Power`, `Clock`, `Voltage` (each gated by
/// `panels.gpu.show` and only when the sensor is bound).
fn gpu_info_pairs(gpu: &GpuBindings, snapshot: &Snapshot, cfg: &Config) -> Vec<(String, String)> {
    let show = |slot: &str| shows(&cfg.panels.gpu.show, slot);
    [
        ("Pwr", &gpu.power, show("power")),
        ("Clk", &gpu.clock, show("clock")),
        ("Volt", &gpu.voltage, show("voltage")),
    ]
    .into_iter()
    .filter(|(_, slot, visible)| *visible && slot.is_some())
    .map(|(label, slot, _)| (label.to_string(), slot_display(snapshot, slot)))
    .collect()
}

/// Two-row CPU metrics template (SPEC §6.3):
/// row 1 `Pwr · TDC · Volt Core  ……  Clk Avg · Clk Eff`,
/// row 2 `Bus Speed · Clk Fabric · Clk Uncore · Clk Memory`.
/// Clock fields are gated by the `clock` show slot, the rest by their own.
struct CpuStrip {
    first_left: Vec<(String, String)>,
    first_right: Vec<(String, String)>,
    second: Vec<(String, String)>,
}

impl CpuStrip {
    /// Rows this instance occupies (0 when every slot is disabled/missing).
    fn rows(&self) -> u8 {
        u8::from(!self.first_left.is_empty() || !self.first_right.is_empty())
            + u8::from(!self.second.is_empty())
    }
}

fn cpu_strip(cpu: &CpuBindings, snapshot: &Snapshot, cfg: &Config) -> CpuStrip {
    let show = |slot: &str| shows(&cfg.panels.cpu.show, slot);
    let mut strip = CpuStrip {
        first_left: Vec::new(),
        first_right: Vec::new(),
        second: Vec::new(),
    };
    if show("power") && cpu.power.is_some() {
        strip
            .first_left
            .push(("Pwr".to_string(), slot_display(snapshot, &cpu.power)));
    }
    if show("current") && cpu.current.is_some() {
        strip
            .first_left
            .push(("TDC".to_string(), slot_display(snapshot, &cpu.current)));
    }
    if show("voltage") && cpu.voltage.is_some() {
        strip.first_left.push((
            "Volt Core".to_string(),
            slot_display(snapshot, &cpu.voltage),
        ));
    }
    if show("clock") {
        if cpu.clock.is_some() {
            strip
                .first_right
                .push(("Clk Avg".to_string(), slot_display(snapshot, &cpu.clock)));
        }
        if cpu.clock_effective.is_some() {
            strip.first_right.push((
                "Clk Eff".to_string(),
                slot_display(snapshot, &cpu.clock_effective),
            ));
        }
        if cpu.bus_speed.is_some() {
            strip.second.push((
                "Bus Speed".to_string(),
                slot_display(snapshot, &cpu.bus_speed),
            ));
        }
        if cpu.clock_fabric.is_some() {
            strip.second.push((
                "Clk Fabric".to_string(),
                slot_display(snapshot, &cpu.clock_fabric),
            ));
        }
        if cpu.clock_uncore.is_some() {
            strip.second.push((
                "Clk Uncore".to_string(),
                slot_display(snapshot, &cpu.clock_uncore),
            ));
        }
        if cpu.clock_memory.is_some() {
            strip.second.push((
                "Clk Memory".to_string(),
                slot_display(snapshot, &cpu.clock_memory),
            ));
            // DDR transfer rate: LHM exposes the memory clock (usually half).
            if let Some((_, mhz)) = slot_value(snapshot, &cpu.clock_memory) {
                strip
                    .second
                    .push(("Speed".to_string(), format::memory_speed_mhz(mhz * 2.0)));
            }
        }
    }
    strip
}

/// Draw the fixed two-row CPU strip into `area` (1–2 rows tall).
fn draw_cpu_strip(frame: &mut Frame, area: Rect, ctx: &PanelCtx, cpu: &CpuBindings) {
    let strip = cpu_strip(cpu, ctx.snapshot, ctx.cfg);
    if area.height == 0 || strip.rows() == 0 {
        return;
    }
    let left = pair_refs(&strip.first_left);
    let right = pair_refs(&strip.first_right);
    if !left.is_empty() || !right.is_empty() {
        let rect = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        if right.is_empty() {
            widgets::info_lines_wrapped(frame, rect, &left, ctx.theme, ctx.stale);
        } else {
            widgets::info_line_split(frame, rect, &left, &right, ctx.theme, ctx.stale);
        }
    }
    let second = pair_refs(&strip.second);
    if !second.is_empty() && area.height > 1 {
        let rect = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        };
        widgets::info_lines_wrapped(frame, rect, &second, ctx.theme, ctx.stale);
    }
}

/// Per-panel layout decisions (CPU strip height depends on the current values).
pub fn panel_layouts(
    panels: &[PanelSpec],
    bindings: &Bindings,
    snapshot: &Snapshot,
    cfg: &Config,
) -> Vec<PanelLayout> {
    let mut layouts = vec![PanelLayout::default(); panels.len()];
    for (index, panel) in panels.iter().enumerate() {
        if let PanelKind::Cpu(cpu_index) = &panel.kind {
            if let Some(cpu) = bindings.cpu.get(*cpu_index) {
                layouts[index].strip_rows = cpu_strip(cpu, snapshot, cfg).rows();
            }
        }
    }
    layouts
}

/// Fit panel heights to the available space.
///
/// Charts are the flex element: on overflow they shrink first (down to
/// `MIN_CHART_HEIGHT` chart rows); with `ui.auto_height` and spare room they
/// grow to absorb the leftover space, so charts stay crisp and no blank band
/// is left at the bottom. When charts cannot absorb the overflow (charts off,
/// or a table-only screen), every row squeezes towards a 4-row minimum so all
/// panels stay visible — gauge panels drop their least important rows to match
/// (`fit_plan`, SPEC §6.1). Rows (not panels) are the unit of accounting —
/// side-by-side GPUs share one row and must never be counted twice.
pub fn fit_heights(
    panels: &[PanelSpec],
    bindings: &Bindings,
    cfg: &Config,
    available: u16,
    charts_on: bool,
    rows: &[Vec<usize>],
    layouts: &[PanelLayout],
) -> Vec<u16> {
    let mut heights: Vec<u16> = panels
        .iter()
        .enumerate()
        .map(|(index, panel)| {
            panel_height(
                panel,
                bindings,
                cfg,
                charts_on,
                layouts.get(index).copied().unwrap_or_default(),
            )
        })
        .collect();
    let mut row_heights: Vec<u16> = rows.iter().map(|row| heights[row[0]]).collect();
    let total: u32 = row_heights.iter().map(|height| u32::from(*height)).sum();
    let available = u32::from(available);

    let is_chart_row = |row: usize| {
        matches!(
            panels[rows[row][0]].kind,
            PanelKind::Cpu(_) | PanelKind::Gpu(_) | PanelKind::Memory
        )
    };

    if total > available {
        let mut overflow = total - available;
        if charts_on {
            for (row, row_height) in row_heights.iter_mut().enumerate() {
                if overflow == 0 {
                    break;
                }
                if !is_chart_row(row) {
                    continue;
                }
                let fixed = row_height.saturating_sub(cfg.charts.height as u16);
                let minimum = fixed + MIN_CHART_HEIGHT;
                let reducible = u32::from(row_height.saturating_sub(minimum));
                let cut = reducible.min(overflow);
                *row_height -= cut as u16;
                overflow -= cut;
            }
        }
        // Still over (gauges have no flex rows, or a table-only screen):
        // squeeze every row to its minimum so all panels stay visible. The
        // gauge draw drops its least important rows to match (SPEC §6.1).
        if overflow > 0 {
            const MIN_PANEL_HEIGHT: u16 = 4;
            for (row, row_height) in row_heights.iter_mut().enumerate() {
                if overflow == 0 {
                    break;
                }
                if charts_on && is_chart_row(row) {
                    continue; // already squeezed to its chart minimum above
                }
                let reducible = u32::from(row_height.saturating_sub(MIN_PANEL_HEIGHT));
                let cut = reducible.min(overflow);
                *row_height -= cut as u16;
                overflow -= cut;
            }
        }
    } else if cfg.ui.auto_height && total < available {
        let mut leftover = available - total;
        // Charts absorb spare space first (bigger graphs = better looking).
        if charts_on {
            let chart_rows: Vec<usize> = (0..rows.len()).filter(|row| is_chart_row(*row)).collect();
            if !chart_rows.is_empty() {
                let mut cursor = 0usize;
                while leftover > 0 {
                    row_heights[chart_rows[cursor % chart_rows.len()]] += 1;
                    leftover -= 1;
                    cursor += 1;
                }
            }
        }
        // Anything left grows tables up to `ui.max_height`.
        if leftover > 0 {
            let max = cfg.ui.max_height.clamp(4, 40) as u16;
            let table_rows: Vec<usize> =
                (0..rows.len()).filter(|row| !is_chart_row(*row)).collect();
            loop {
                let mut progressed = false;
                for &row in &table_rows {
                    if leftover == 0 {
                        break;
                    }
                    if row_heights[row] < max {
                        row_heights[row] += 1;
                        leftover -= 1;
                        progressed = true;
                    }
                }
                if !progressed {
                    break;
                }
            }
        }
    }

    for (row, indices) in rows.iter().enumerate() {
        for index in indices {
            heights[*index] = row_heights[row];
        }
    }
    heights
}

/// Minimum chart rows a squeezed chart panel keeps (SPEC §6.1 responsive rule).
const MIN_CHART_HEIGHT: u16 = 4;

/// One planned fixed row of a gauge layout: how many rows it takes, which
/// renderer to use, and how important it is (higher survives shrinking).
struct PlannedRow {
    kind: u8,
    height: u16,
    keep: u8,
}

/// Drop the least important rows until the plan fits `available` rows,
/// keeping the survivors in display order (SPEC §6.1 short-panel rule).
/// Never drops the last row, so callers always render something.
fn fit_plan(rows: Vec<PlannedRow>, available: u16) -> Vec<PlannedRow> {
    let mut rows = rows;
    let total = |rows: &[PlannedRow]| rows.iter().map(|row| row.height).sum::<u16>();
    while rows.len() > 1 && total(&rows) > available {
        if let Some((index, _)) = rows.iter().enumerate().min_by_key(|(_, row)| row.keep) {
            rows.remove(index);
        }
    }
    rows
}

/// Lay out a fitted plan and return each row's area.
fn plan_areas(rows: &[PlannedRow], area: Rect) -> Vec<Rect> {
    let constraints: Vec<Constraint> = rows
        .iter()
        .map(|row| Constraint::Length(row.height))
        .collect();
    Layout::vertical(constraints).split(area).to_vec()
}

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
        PanelKind::Thermal => {
            draw_slot_table(frame, area, "Temperatures", &ctx.bindings.thermal, ctx)
        }
        PanelKind::Voltage => {
            // Per-core rails are hidden unless the panel is zoomed (`enter`).
            let visible: Vec<Slot> = voltage_table_slots(&ctx.bindings.voltage, ctx.zoomed)
                .into_iter()
                .cloned()
                .collect();
            draw_slot_table(frame, area, "Voltages", &visible, ctx)
        }
        PanelKind::Fans => draw_fans(frame, area, ctx),
        PanelKind::Power => draw_power(frame, area, ctx),
        PanelKind::Storage => draw_storage(frame, area, ctx),
        PanelKind::Network => draw_network(frame, area, ctx),
        PanelKind::Custom(id) => {
            if let Some(panel) = ctx
                .cfg
                .panels
                .custom
                .iter()
                .find(|panel| panel.id == *id)
                .cloned()
            {
                draw_custom(frame, area, &panel, ctx);
            }
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

    let show = |slot: &str| shows(&ctx.cfg.panels.cpu.show, slot);
    let core_values: Vec<Option<f64>> = cpu
        .core_loads
        .iter()
        .map(|slot| bindings::slot_sensor(ctx.snapshot, slot).and_then(|sensor| sensor.value))
        .collect();
    // Bottom metrics template, SPEC §6.3 (two fixed rows).
    let strip_rows = u16::from(ctx.layout.strip_rows);
    let bars_shown = show("core_loads") && !core_values.is_empty();

    if ctx.charts_on() {
        let mut graphs: Vec<(String, Vec<Series>, YMode)> = Vec::new();
        if show("load") {
            let series: Vec<Series> = series_for(ctx, &cpu.load_total, ctx.theme.cpu)
                .into_iter()
                .collect();
            if !series.is_empty() {
                graphs.push((format!("{panel_key}:load"), series, YMode::Percent));
            }
        }
        if show("temp") {
            let series: Vec<Series> = series_for(ctx, &cpu.temp, ctx.theme.warn)
                .into_iter()
                .collect();
            if !series.is_empty() {
                graphs.push((format!("{panel_key}:temp"), series, YMode::Auto));
            }
        }
        if !graphs.is_empty() || bars_shown || strip_rows > 0 {
            let reserved = u16::from(bars_shown) * 2 + strip_rows;
            let chart_height = if graphs.is_empty() {
                0
            } else {
                inner.height.saturating_sub(reserved).max(MIN_CHART_HEIGHT)
            };
            let mut offset = 0u16;
            if !graphs.is_empty() {
                let chart_rect = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: chart_height,
                };
                let graph_rects = Layout::vertical(vec![
                    Constraint::Ratio(1, graphs.len() as u32);
                    graphs.len()
                ])
                .split(chart_rect);
                for (graph_index, (key, series, y_mode)) in graphs.into_iter().enumerate() {
                    draw_metric_graph(frame, graph_rects[graph_index], ctx, key, series, y_mode);
                }
                offset += chart_height;
            }
            if bars_shown {
                let bars_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 2,
                };
                widgets::bars(
                    frame,
                    bars_rect,
                    "Per-Core",
                    &core_values,
                    ctx.theme,
                    ctx.stale,
                );
                offset += 2;
            }
            if strip_rows > 0 {
                let info_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: strip_rows,
                };
                draw_cpu_strip(frame, info_rect, ctx, cpu);
            }
            return;
        }
    }

    // Gauge fallback (charts off, or nothing chartable). Shorter panels drop
    // the least important rows first (SPEC §6.1).
    let mut plan: Vec<PlannedRow> = Vec::new(); // 0 temp, 1 load, 2 bars, 3 info
    if show("temp") {
        plan.push(PlannedRow {
            kind: 0,
            height: 1,
            keep: 10,
        });
    }
    if show("load") {
        plan.push(PlannedRow {
            kind: 1,
            height: 1,
            keep: 11,
        });
    }
    if bars_shown {
        plan.push(PlannedRow {
            kind: 2,
            height: 2,
            keep: 5,
        });
    }
    if strip_rows > 0 {
        plan.push(PlannedRow {
            kind: 3,
            height: strip_rows,
            keep: 8,
        });
    }
    let plan = fit_plan(plan, inner.height);
    if plan.is_empty() {
        return;
    }
    let areas = plan_areas(&plan, inner);
    for (row, area) in plan.iter().zip(areas.iter()) {
        match row.kind {
            0 => gauge_for_slot(
                frame,
                *area,
                "Temp",
                ctx,
                &cpu.temp,
                100.0,
                Metric::Temperature,
            ),
            1 => gauge_for_slot(
                frame,
                *area,
                "Load",
                ctx,
                &cpu.load_total,
                100.0,
                Metric::Load,
            ),
            2 => widgets::bars(frame, *area, "Per-Core", &core_values, ctx.theme, ctx.stale),
            _ => draw_cpu_strip(frame, *area, ctx, cpu),
        }
    }
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

    let show = |slot: &str| shows(&ctx.cfg.panels.gpu.show, slot);
    // Info row pairs in display order: Pwr, Clk, Volt (SPEC §6.3).
    let info_pairs_owned = gpu_info_pairs(gpu, ctx.snapshot, ctx.cfg);
    let pairs: Vec<(&str, String)> = pair_refs(&info_pairs_owned);
    let vram_shown = show("memory");
    let shared_shown = show("memory") && gpu.shared_used.is_some();
    let info_row = !pairs.is_empty();
    // Fans always get their own row underneath the metrics (SPEC §6.3).
    let fan_row = show("fan") && !gpu.fans.is_empty();

    if ctx.charts_on() {
        let mut graphs: Vec<(String, Vec<Series>, YMode)> = Vec::new();
        if show("load") || show("memory") {
            let series: Vec<Series> = [
                if show("load") {
                    series_for(ctx, &gpu.load, ctx.theme.gpu)
                } else {
                    None
                },
                if show("memory") {
                    series_for(ctx, &gpu.mem_load, ctx.theme.memory)
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            if !series.is_empty() {
                graphs.push((format!("{panel_key}:load"), series, YMode::Percent));
            }
        }
        if show("temp") {
            let series: Vec<Series> = [
                series_for(ctx, &gpu.temp, ctx.theme.warn),
                series_for(ctx, &gpu.hotspot, ctx.theme.crit),
                series_for(ctx, &gpu.mem_temp, ctx.theme.network),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !series.is_empty() {
                graphs.push((format!("{panel_key}:temp"), series, YMode::Auto));
            }
        }
        if !graphs.is_empty() || vram_shown || shared_shown || info_row || fan_row {
            let reserved = u16::from(vram_shown)
                + u16::from(shared_shown)
                + u16::from(info_row)
                + u16::from(fan_row);
            let chart_height = if graphs.is_empty() {
                0
            } else {
                inner.height.saturating_sub(reserved).max(MIN_CHART_HEIGHT)
            };
            let mut offset = 0u16;
            if !graphs.is_empty() {
                let chart_rect = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: chart_height,
                };
                let graph_rects = Layout::vertical(vec![
                    Constraint::Ratio(1, graphs.len() as u32);
                    graphs.len()
                ])
                .split(chart_rect);
                for (graph_index, (key, series, y_mode)) in graphs.into_iter().enumerate() {
                    draw_metric_graph(frame, graph_rects[graph_index], ctx, key, series, y_mode);
                }
                offset += chart_height;
            }
            if vram_shown {
                let vram_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                draw_vram_gauge(frame, vram_rect, ctx, gpu);
                offset += 1;
            }
            if shared_shown {
                let shared_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                draw_shared_gauge(frame, shared_rect, ctx, gpu);
                offset += 1;
            }
            if info_row {
                let info_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                widgets::info_line(frame, info_rect, &pairs, ctx.theme, ctx.stale);
                offset += 1;
            }
            if fan_row {
                let fans_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                draw_gpu_fans(frame, fans_rect, ctx, gpu);
            }
            return;
        }
    }

    // Gauge fallback (charts off, or nothing chartable). Shorter panels drop
    // the least important rows first (SPEC §6.1).
    let mut plan: Vec<PlannedRow> = Vec::new(); // 0 temp, 1 hotspot, 2 mem_temp, 3 load, 4 vram, 5 info, 6 fans, 8 shared
    if show("temp") {
        plan.push(PlannedRow {
            kind: 0,
            height: 1,
            keep: 10,
        });
    }
    if show("temp") {
        plan.push(PlannedRow {
            kind: 1,
            height: 1,
            keep: 4,
        });
        plan.push(PlannedRow {
            kind: 2,
            height: 1,
            keep: 3,
        });
    }
    if show("load") {
        plan.push(PlannedRow {
            kind: 3,
            height: 1,
            keep: 11,
        });
    }
    if vram_shown {
        plan.push(PlannedRow {
            kind: 4,
            height: 1,
            keep: 6,
        });
    }
    if shared_shown {
        plan.push(PlannedRow {
            kind: 8,
            height: 1,
            keep: 5,
        });
    }
    if info_row {
        plan.push(PlannedRow {
            kind: 5,
            height: 1,
            keep: 8,
        });
    }
    if fan_row {
        plan.push(PlannedRow {
            kind: 6,
            height: 1,
            keep: 7,
        });
    }
    let plan = fit_plan(plan, inner.height);
    if plan.is_empty() {
        return;
    }
    let areas = plan_areas(&plan, inner);
    for (row, area) in plan.iter().zip(areas.iter()) {
        match row.kind {
            0 => gauge_for_slot(
                frame,
                *area,
                "Temp",
                ctx,
                &gpu.temp,
                100.0,
                Metric::Temperature,
            ),
            1 => gauge_for_slot(
                frame,
                *area,
                "Hot Spot",
                ctx,
                &gpu.hotspot,
                100.0,
                Metric::Temperature,
            ),
            2 => gauge_for_slot(
                frame,
                *area,
                "Mem Temp",
                ctx,
                &gpu.mem_temp,
                100.0,
                Metric::Temperature,
            ),
            3 => gauge_for_slot(frame, *area, "Load", ctx, &gpu.load, 100.0, Metric::Load),
            4 => draw_vram_gauge(frame, *area, ctx, gpu),
            6 => draw_gpu_fans(frame, *area, ctx, gpu),
            8 => draw_shared_gauge(frame, *area, ctx, gpu),
            _ => widgets::info_line(frame, *area, &pairs, ctx.theme, ctx.stale),
        }
    }
}

/// One line with every fan on the card: `Fan 1 1205 RPM  Fan 2 1180 RPM`
/// (single-fan cards keep the compact `Fan` label). Used when the fans do not
/// fit beside `Pwr`/`Clk` on the metrics row.
fn draw_gpu_fans(frame: &mut Frame, area: Rect, ctx: &PanelCtx, gpu: &GpuBindings) {
    if area.height == 0 || gpu.fans.is_empty() {
        return;
    }
    let pairs_owned = gpu_fan_pairs(gpu, ctx.snapshot);
    let pairs: Vec<(&str, String)> = pairs_owned
        .iter()
        .map(|(label, value)| (label.as_str(), value.clone()))
        .collect();
    widgets::info_line(frame, area, &pairs, ctx.theme, ctx.stale);
}

/// `Fan 1`/`Fan 2`… label plus displayed value for every fan on the card.
fn gpu_fan_pairs(gpu: &GpuBindings, snapshot: &Snapshot) -> Vec<(String, String)> {
    let multi = gpu.fans.len() > 1;
    gpu.fans
        .iter()
        .enumerate()
        .map(|(index, row)| {
            (
                fan_label(&row.fan.label, index, multi),
                slot_display(snapshot, &Some(row.fan.clone())),
            )
        })
        .collect()
}

/// `GPU Fan 2` → `Fan 2`; single-fan cards stay `Fan`.
fn fan_label(name: &str, index: usize, multi: bool) -> String {
    if !multi {
        return "Fan".to_string();
    }
    let digits = name
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    if digits.is_empty() {
        format!("Fan {}", index + 1)
    } else {
        format!("Fan {digits}")
    }
}

/// One `label used / total (pct %)` gauge row (VRAM, shared GPU memory).
/// `total_fallback_mb` covers cards that report no total sensor (NVIDIA shared
/// memory); it is expressed in MB and converted to the used sensor's unit.
fn draw_usage_gauge(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    ctx: &PanelCtx,
    used_slot: &Option<Slot>,
    total_slot: &Option<Slot>,
    total_fallback_mb: Option<f64>,
) {
    let used = slot_value(ctx.snapshot, used_slot);
    let bound_total = slot_value(ctx.snapshot, total_slot).map(|(_, value)| value);
    match used {
        Some((used_sensor, used_value)) => {
            let total = bound_total.filter(|value| *value > 0.0).or_else(|| {
                total_fallback_mb.map(|mb| match used_sensor.kind {
                    SensorKind::SmallData => mb,
                    _ => mb / 1024.0,
                })
            });
            let (display, ratio) = match total {
                Some(total_value) => (
                    format::data_ratio(&used_sensor.kind, used_value, total_value),
                    used_value / total_value,
                ),
                None => (format::sensor_value(used_sensor), f64::NAN),
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
                frame, area, label, &display, fill_ratio, style, ctx.theme, ctx.stale,
            );
        }
        None => {
            widgets::gauge(
                frame,
                area,
                label,
                "n/a",
                0.0,
                with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ctx.theme,
                ctx.stale,
            );
        }
    }
}

fn draw_vram_gauge(frame: &mut Frame, area: Rect, ctx: &PanelCtx, gpu: &GpuBindings) {
    draw_usage_gauge(
        frame,
        area,
        "VRAM",
        ctx,
        &gpu.mem_used,
        &gpu.mem_total,
        None,
    );
}

/// Windows shared GPU memory (D3D), shown under the VRAM row (SPEC §6.3).
fn draw_shared_gauge(frame: &mut Frame, area: Rect, ctx: &PanelCtx, gpu: &GpuBindings) {
    let fallback = if gpu.shared_total.is_none() {
        estimated_shared_total_mb(ctx, gpu)
    } else {
        None
    };
    draw_usage_gauge(
        frame,
        area,
        "Shared",
        ctx,
        &gpu.shared_used,
        &gpu.shared_total,
        fallback,
    );
}

/// Estimated shared-memory total when the card reports none (NVIDIA): system
/// RAM total (`used + available`, GB) minus this card's VRAM total, in MB.
fn estimated_shared_total_mb(ctx: &PanelCtx, gpu: &GpuBindings) -> Option<f64> {
    let memory = ctx.bindings.memory.as_ref()?;
    let (_, used_gb) = slot_value(ctx.snapshot, &memory.used)?;
    let (_, available_gb) = slot_value(ctx.snapshot, &memory.available)?;
    let (_, vram_mb) = slot_value(ctx.snapshot, &gpu.mem_total)?;
    ram_minus_vram_mb(used_gb, available_gb, vram_mb)
}

/// `(RAM used + available) × 1024 - VRAM total`, in MB; `None` when the
/// subtraction is not positive (nonsensical readings).
fn ram_minus_vram_mb(used_gb: f64, available_gb: f64, vram_mb: f64) -> Option<f64> {
    let shared_mb = (used_gb + available_gb) * 1024.0 - vram_mb;
    (shared_mb > 0.0).then_some(shared_mb)
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

    let show = |slot: &str| shows(&ctx.cfg.panels.memory.show, slot);
    let usage_shown = show("load");
    let virtual_shown = show("virtual");

    if ctx.charts_on() {
        let usage = if usage_shown {
            memory_usage_series(ctx, memory)
                .into_iter()
                .collect::<Vec<Series>>()
        } else {
            Vec::new()
        };
        if !usage.is_empty() || show("used") || virtual_shown {
            let reserved = u16::from(show("used")) + u16::from(virtual_shown);
            let chart_height = if usage.is_empty() {
                0
            } else {
                inner.height.saturating_sub(reserved).max(MIN_CHART_HEIGHT)
            };
            let mut offset = 0u16;
            if !usage.is_empty() {
                let chart_rect = Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: chart_height,
                };
                draw_metric_graph(
                    frame,
                    chart_rect,
                    ctx,
                    "memory:usage".to_string(),
                    usage,
                    YMode::Percent,
                );
                offset += chart_height;
            }
            if show("used") {
                let used_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                draw_memory_used_gauge(frame, used_rect, ctx, memory);
                offset += 1;
            }
            if virtual_shown {
                let virtual_rect = Rect {
                    x: inner.x,
                    y: inner.y + offset,
                    width: inner.width,
                    height: 1,
                };
                draw_virtual_gauge(frame, virtual_rect, ctx, memory);
            }
            return;
        }
    }

    // Gauge fallback (charts off, or nothing chartable). Shorter panels drop
    // the least important rows first (SPEC §6.1).
    let mut plan: Vec<PlannedRow> = Vec::new(); // 0 used, 1 load, 2 virtual
    if show("used") {
        plan.push(PlannedRow {
            kind: 0,
            height: 1,
            keep: 10,
        });
    }
    if show("load") {
        plan.push(PlannedRow {
            kind: 1,
            height: 1,
            keep: 11,
        });
    }
    if virtual_shown {
        plan.push(PlannedRow {
            kind: 2,
            height: 1,
            keep: 4,
        });
    }
    let plan = fit_plan(plan, inner.height);
    if plan.is_empty() {
        return;
    }
    let areas = plan_areas(&plan, inner);
    for (row, area) in plan.iter().zip(areas.iter()) {
        match row.kind {
            0 => draw_memory_used_gauge(frame, *area, ctx, memory),
            1 => gauge_for_slot(frame, *area, "Load", ctx, &memory.load, 100.0, Metric::Load),
            _ => draw_virtual_gauge(frame, *area, ctx, memory),
        }
    }
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

fn draw_virtual_gauge(frame: &mut Frame, area: Rect, ctx: &PanelCtx, memory: &MemoryBindings) {
    let virtual_used = slot_value(ctx.snapshot, &memory.virtual_used);
    let virtual_available = slot_value(ctx.snapshot, &memory.virtual_available);
    match virtual_used {
        Some((used_sensor, used_value)) => {
            let (display, ratio) = match virtual_available {
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
                "Virtual",
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
                "Virtual",
                "n/a",
                0.0,
                with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ctx.theme,
                ctx.stale,
            );
        }
    }
}

/// Single-graph helper for ad-hoc views (T5.2).
pub(crate) fn draw_series_graph(
    frame: &mut Frame,
    area: Rect,
    ctx: &mut PanelCtx,
    key: String,
    series: Vec<Series>,
    y_mode: YMode,
) {
    draw_metric_graph(frame, area, ctx, key, series, y_mode);
}

/// True when a `panels.<preset>.show` list includes `slot` (case-insensitive).
fn shows(list: &[String], slot: &str) -> bool {
    list.iter().any(|entry| entry.eq_ignore_ascii_case(slot))
}

// ------------------------------------------------------------- custom panels

fn custom_widget_height(kind: WidgetKind, cfg: &Config, charts_on: bool) -> u16 {
    match kind {
        WidgetKind::Graph if charts_on => cfg.charts.height as u16,
        WidgetKind::Table => (cfg.ui.max_height.clamp(4, 40) as u16).min(8),
        _ => 1,
    }
}

fn widget_filter_slots(widget: &CustomWidget, snapshot: &Snapshot) -> Vec<Slot> {
    let Some(filter) = widget.filter.as_ref() else {
        return Vec::new();
    };
    let kinds: Vec<SensorKind> = filter
        .types
        .iter()
        .map(|name| bindings::parse_sensor_kind(name))
        .collect();
    let name_regex = filter
        .name_regex
        .as_ref()
        .and_then(|pattern| regex::Regex::new(pattern).ok());
    let hardware_kind = filter
        .hardware
        .as_deref()
        .and_then(bindings::parse_hardware_kind);
    let hardware_id = filter
        .hardware
        .as_deref()
        .filter(|value| value.starts_with('/'));

    let mut slots = Vec::new();
    for hardware in snapshot.all_hardware() {
        if hardware_kind.is_some_and(|kind| hardware.kind != kind) {
            continue;
        }
        if hardware_id.is_some_and(|id| hardware.id != id) {
            continue;
        }
        for sensor in hardware.sensors() {
            if !kinds.is_empty() && !kinds.contains(&sensor.kind) {
                continue;
            }
            if name_regex
                .as_ref()
                .is_some_and(|regex| !regex.is_match(&sensor.name))
            {
                continue;
            }
            slots.push(bindings::slot_for(hardware, sensor));
        }
    }
    slots
}

fn widget_slot(widget: &CustomWidget, snapshot: &Snapshot) -> Option<Slot> {
    if widget.sensor.is_some() || widget.matcher.is_some() {
        let (hardware, sensor) = bindings::resolve_sensor_ref(
            snapshot,
            widget.sensor.as_deref(),
            widget.matcher.as_ref(),
        )?;
        return Some(bindings::slot_for(hardware, sensor));
    }
    widget_filter_slots(widget, snapshot).into_iter().next()
}

/// Display text honouring an optional unit override.
fn widget_display(sensor: &Sensor, unit: Option<&str>) -> String {
    match (unit, sensor.value) {
        (Some(unit), Some(value)) => format!("{value:.1} {unit}"),
        (Some(_), None) => "—".to_string(),
        (None, _) => format::sensor_value(sensor),
    }
}

fn widget_ratio(kind: &SensorKind, value: f64) -> f64 {
    match kind {
        SensorKind::Temperature | SensorKind::Load | SensorKind::Control | SensorKind::Level => {
            (value / 100.0).clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

fn draw_custom(frame: &mut Frame, area: Rect, panel: &CustomPanel, ctx: &mut PanelCtx) {
    let title = panel
        .title
        .clone()
        .unwrap_or_else(|| format!("custom:{}", panel.id));
    let block = panel_block(title, ctx.theme, ctx.stale, ctx.focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || panel.widgets.is_empty() {
        return;
    }
    let heights: Vec<u16> = panel
        .widgets
        .iter()
        .map(|widget| custom_widget_height(widget.kind, ctx.cfg, ctx.charts_on()))
        .collect();
    let constraints: Vec<Constraint> = heights
        .iter()
        .map(|height| Constraint::Length(*height))
        .collect();
    let areas = Layout::vertical(constraints).split(inner);
    for (index, (widget, rect)) in panel.widgets.iter().zip(areas.iter()).enumerate() {
        let key = format!("custom:{}:w{index}", panel.id);
        draw_custom_widget(frame, *rect, widget, &key, ctx);
    }
}

fn draw_custom_widget(
    frame: &mut Frame,
    area: Rect,
    widget: &CustomWidget,
    key: &str,
    ctx: &mut PanelCtx,
) {
    if area.height == 0 || area.width < 12 {
        return;
    }
    let color = ctx.theme.widget_color(widget.color.as_deref());
    let missing = |frame: &mut Frame, ctx: &PanelCtx| {
        widgets::gauge(
            frame,
            area,
            widget.label.as_deref().unwrap_or("sensor"),
            "n/a",
            0.0,
            with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
            ctx.theme,
            ctx.stale,
        );
    };

    match widget.kind {
        WidgetKind::Graph => {
            let Some(slot) = widget_slot(widget, ctx.snapshot) else {
                missing(frame, ctx);
                return;
            };
            if ctx.charts_on() {
                let points = ctx
                    .history
                    .map(|history| history.window(&slot.sensor_id, ctx.window_points()))
                    .unwrap_or_default();
                let (latest_text, latest_style) = match bindings::slot_sensor(ctx.snapshot, &slot) {
                    Some(sensor) => (
                        widget_display(sensor, widget.unit.as_deref()),
                        ctx.theme.sensor_style(&sensor.kind, sensor.value),
                    ),
                    None => ("n/a".to_string(), Style::new().fg(ctx.theme.muted)),
                };
                let series = Series {
                    label: widget.label.clone().unwrap_or_else(|| slot.label.clone()),
                    color,
                    latest_text,
                    latest_style,
                    points,
                };
                draw_metric_graph(
                    frame,
                    area,
                    ctx,
                    key.to_string(),
                    vec![series],
                    widget.y_mode.unwrap_or(YMode::Auto),
                );
            } else {
                draw_custom_gauge(frame, area, widget, &slot, color, ctx);
            }
        }
        WidgetKind::Gauge => match widget_slot(widget, ctx.snapshot) {
            Some(slot) => draw_custom_gauge(frame, area, widget, &slot, color, ctx),
            None => missing(frame, ctx),
        },
        WidgetKind::Bars => {
            let slots = if widget.filter.is_some() {
                widget_filter_slots(widget, ctx.snapshot)
            } else {
                widget_slot(widget, ctx.snapshot).into_iter().collect()
            };
            let values: Vec<Option<f64>> = slots
                .iter()
                .map(|slot| {
                    bindings::slot_sensor(ctx.snapshot, slot).and_then(|sensor| sensor.value)
                })
                .collect();
            widgets::bars(frame, area, "", &values, ctx.theme, ctx.stale);
        }
        WidgetKind::Sparkline => {
            if let Some(slot) = widget_slot(widget, ctx.snapshot) {
                let points: Vec<f64> = ctx
                    .history
                    .map(|history| history.window(&slot.sensor_id, ctx.window_points()))
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .map(f64::from)
                    .collect();
                widgets::sparkline(
                    frame,
                    area,
                    &points,
                    with_dim(Style::new().fg(color), ctx.stale),
                    ctx.stale,
                );
            }
        }
        WidgetKind::Table => {
            let slots = widget_filter_slots(widget, ctx.snapshot);
            let labels: Vec<String> = slots.iter().map(|slot| slot.label.clone()).collect();
            let duplicated = duplicate_labels(&labels);
            let mut rows: Vec<Row<'static>> = Vec::new();
            for (slot, duplicated) in slots.iter().zip(duplicated) {
                let Some(sensor) = bindings::slot_sensor(ctx.snapshot, slot) else {
                    continue;
                };
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
                rows.push(
                    Row::new(vec![
                        Cell::from(name),
                        widgets::right_cell(widget_display(sensor, widget.unit.as_deref()), style),
                        widgets::right_cell(
                            format::stat(&sensor.kind, sensor.min),
                            with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                        ),
                        widgets::right_cell(
                            format::stat(&sensor.kind, sensor.max),
                            with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                        ),
                    ])
                    .style(style),
                );
            }
            let widths = vec![
                Constraint::Min(12),
                Constraint::Length(12),
                Constraint::Length(9),
                Constraint::Length(9),
            ];
            widgets::table(
                frame,
                area,
                Block::default(),
                &["Sensor", "Value", "Min", "Max"],
                widths,
                rows,
                ctx.scroll,
                ctx.theme,
                ctx.stale,
            );
        }
    }
}

fn draw_custom_gauge(
    frame: &mut Frame,
    area: Rect,
    widget: &CustomWidget,
    slot: &Slot,
    color: Color,
    ctx: &PanelCtx,
) {
    let Some(sensor) = bindings::slot_sensor(ctx.snapshot, slot) else {
        return;
    };
    let label = widget.label.clone().unwrap_or_else(|| slot.label.clone());
    let display = widget_display(sensor, widget.unit.as_deref());
    let ratio = sensor
        .value
        .map_or(0.0, |value| widget_ratio(&sensor.kind, value));
    widgets::gauge(
        frame,
        area,
        &label,
        &display,
        ratio,
        with_dim(Style::new().fg(color), ctx.stale),
        ctx.theme,
        ctx.stale,
    );
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

fn cmp_values(a: Option<f64>, b: Option<f64>, descending: bool) -> std::cmp::Ordering {
    match (a, b) {
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
    }
}

fn sort_table_rows(sort: TableSort, rows: &mut Vec<(Option<f64>, String, Row<'static>)>) {
    match sort {
        TableSort::ValueDesc => rows.sort_by(|a, b| cmp_values(a.0, b.0, true)),
        TableSort::ValueAsc => rows.sort_by(|a, b| cmp_values(a.0, b.0, false)),
        TableSort::Name => rows.sort_by_key(|row| row.1.to_ascii_lowercase()),
        TableSort::Unspecified => {}
    }
}

/// One row before grouping; `selected` marks the selectable fan cursor.
struct TableRow {
    value: Option<f64>,
    label: String,
    row: Row<'static>,
    selected: bool,
}

struct TableGroup {
    hardware_id: String,
    hardware_name: String,
    rows: Vec<TableRow>,
}

fn sort_table_entries(sort: TableSort, rows: &mut [TableRow]) {
    match sort {
        TableSort::ValueDesc => rows.sort_by(|a, b| cmp_values(a.value, b.value, true)),
        TableSort::ValueAsc => rows.sort_by(|a, b| cmp_values(a.value, b.value, false)),
        TableSort::Name => rows.sort_by_key(|row| row.label.to_ascii_lowercase()),
        TableSort::Unspecified => {}
    }
}

/// Device-qualified label used by the flat fans table and its sort order.
fn fan_display_label(fan: &FanRow, snapshot: &Snapshot) -> String {
    let hardware_name = snapshot
        .hardware(&fan.hardware_id)
        .map(|hardware| hardware.name.clone())
        .unwrap_or_else(|| fan.hardware_id.clone());
    format!("{} ({hardware_name})", fan.fan.label)
}

/// Controllable fan indices in the order the fans table displays them, so
/// keyboard selection moves one visible row at a time.
pub fn fan_control_order(fans: &[FanRow], snapshot: &Snapshot, sort: TableSort) -> Vec<usize> {
    let mut rows: Vec<(Option<f64>, String, usize)> = fans
        .iter()
        .enumerate()
        .filter(|(_, fan)| fan.control.is_some())
        .map(|(index, fan)| {
            let value = bindings::slot_sensor(snapshot, &fan.fan).and_then(|sensor| sensor.value);
            (value, fan_display_label(fan, snapshot), index)
        })
        .collect();
    match sort {
        TableSort::ValueDesc => rows.sort_by(|a, b| cmp_values(a.0, b.0, true)),
        TableSort::ValueAsc => rows.sort_by(|a, b| cmp_values(a.0, b.0, false)),
        TableSort::Name => rows.sort_by_key(|row| row.1.to_ascii_lowercase()),
        TableSort::Unspecified => {}
    }
    rows.into_iter().map(|(_, _, index)| index).collect()
}

/// Group rows per hardware device (first-seen tree order), sort within each
/// device, and insert one bold device heading before its rows. Returns the
/// flattened rows and the flattened index of the row whose `id` is selected.
fn grouped_rows(
    entries: Vec<(String, String, TableRow)>,
    columns: usize,
    ctx: &PanelCtx,
) -> (Vec<Row<'static>>, Option<usize>) {
    let mut groups: Vec<TableGroup> = Vec::new();
    for (hardware_id, hardware_name, row) in entries {
        if let Some(index) = groups
            .iter()
            .position(|group| group.hardware_id == hardware_id)
        {
            groups[index].rows.push(row);
        } else {
            groups.push(TableGroup {
                hardware_id,
                hardware_name,
                rows: vec![row],
            });
        }
    }

    let mut rows = Vec::new();
    let mut selected_index = None;
    for mut group in groups {
        sort_table_entries(ctx.sort, &mut group.rows);
        rows.push(group_header_row(&group.hardware_name, columns, ctx));
        for row in group.rows {
            if row.selected {
                selected_index = Some(rows.len());
            }
            rows.push(row.row);
        }
    }
    (rows, selected_index)
}

fn group_header_row(name: &str, columns: usize, ctx: &PanelCtx) -> Row<'static> {
    let marker = if ctx.ascii { "> " } else { "▸ " };
    let style = with_dim(Style::new().fg(ctx.theme.accent).bold(), ctx.stale);
    let mut cells = vec![Cell::from(format!("{marker}{name}"))];
    for _ in 1..columns {
        cells.push(Cell::from(""));
    }
    Row::new(cells).style(style)
}

/// Per-core voltage rails (`Core #1 VID`, `CPU Core #2`, …) are hidden from
/// the voltages table; the zoomed panel (`enter`) reveals them (SPEC §6.3).
fn is_core_voltage_rail(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(index) = rest.find("core #") {
        let after = &rest[index + "core #".len()..];
        if after.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
        rest = after;
    }
    false
}

/// Slots shown by the voltages table: per-core rails only in the zoomed detail
/// view (`enter`), never in the docked table (SPEC §6.3).
fn voltage_table_slots(slots: &[Slot], zoomed: bool) -> Vec<&Slot> {
    if zoomed {
        slots.iter().collect()
    } else {
        slots
            .iter()
            .filter(|slot| !is_core_voltage_rail(&slot.label))
            .collect()
    }
}

fn draw_slot_table(frame: &mut Frame, area: Rect, title: &str, slots: &[Slot], ctx: &PanelCtx) {
    let narrow = area.width < 80; // drop Min/Max columns (SPEC §6.1)
    let block = panel_block(title.to_string(), ctx.theme, ctx.stale, ctx.focused);
    let columns = if narrow { 2 } else { 4 };
    let entries: Vec<(String, String, TableRow)> = slots
        .iter()
        .filter_map(|slot| {
            let sensor = bindings::slot_sensor(ctx.snapshot, slot)?;
            let hardware_name = ctx
                .snapshot
                .hardware(&slot.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_else(|| slot.hardware_id.clone());
            let style = with_dim(
                ctx.theme.sensor_style(&sensor.kind, sensor.value),
                ctx.stale,
            );
            let mut cells = vec![
                Cell::from(slot.label.clone()),
                widgets::right_cell(format::sensor_value(sensor), style),
            ];
            if !narrow {
                cells.push(widgets::right_cell(
                    format::stat(&sensor.kind, sensor.min),
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ));
                cells.push(widgets::right_cell(
                    format::stat(&sensor.kind, sensor.max),
                    with_dim(Style::new().fg(ctx.theme.muted), ctx.stale),
                ));
            }
            Some((
                slot.hardware_id.clone(),
                hardware_name,
                TableRow {
                    value: sensor.value,
                    label: slot.label.clone(),
                    row: Row::new(cells).style(style),
                    selected: false,
                },
            ))
        })
        .collect();

    let (rows, _) = grouped_rows(entries, columns, ctx);
    let (headers, widths): (&[&str], Vec<Constraint>) = if narrow {
        (
            &["Sensor", "Value"],
            vec![Constraint::Min(12), Constraint::Length(12)],
        )
    } else {
        (
            &["Sensor", "Value", "Min", "Max"],
            vec![
                Constraint::Min(12),
                Constraint::Length(12),
                Constraint::Length(9),
                Constraint::Length(9),
            ],
        )
    };
    widgets::table(
        frame, area, block, headers, widths, rows, ctx.scroll, ctx.theme, ctx.stale,
    );
}

/// Speed/setting pair for a fan row. A valueless Control sensor is LHM's
/// Default mode; it renders as `Auto` (Default and Auto mean the same).
fn control_setting(snapshot: &Snapshot, control: &Option<Slot>) -> (String, bool) {
    let Some(slot) = control else {
        return ("n/a".to_string(), false);
    };
    match bindings::slot_sensor(snapshot, slot) {
        None => ("n/a".to_string(), false),
        Some(sensor) => match sensor.value {
            Some(value) => (format!("{value:.0} %"), false),
            None => ("Auto".to_string(), true),
        },
    }
}

fn draw_fans(frame: &mut Frame, area: Rect, ctx: &PanelCtx) {
    let block = panel_block(
        "Fans & Controls".to_string(),
        ctx.theme,
        ctx.stale,
        ctx.focused,
    );
    let selected_binding = fan_control_order(&ctx.bindings.fans, ctx.snapshot, ctx.sort)
        .get(ctx.fan_selected)
        .copied();

    let mut entries: Vec<TableRow> = ctx
        .bindings
        .fans
        .iter()
        .enumerate()
        .map(|(index, fan)| {
            let speed_value =
                slot_value(ctx.snapshot, &Some(fan.fan.clone())).map(|(_, value)| value);
            let speed = slot_display(ctx.snapshot, &Some(fan.fan.clone()));
            let (setting, auto) = control_setting(ctx.snapshot, &fan.control);
            let selected = ctx.focused && selected_binding == Some(index);
            let marker = if selected {
                if ctx.ascii {
                    "> "
                } else {
                    "▸ "
                }
            } else {
                ""
            };
            // One flat table: the device name in parentheses identifies the
            // hardware, and there are no group headings to confuse with the
            // selected control row.
            let label = fan_display_label(fan, ctx.snapshot);
            let name = format!("{marker}{label}");
            let base = if selected {
                Style::new().fg(ctx.theme.accent).bold()
            } else {
                Style::new().fg(ctx.theme.text)
            };
            let base = with_dim(base, ctx.stale);
            let setting_style = if selected || !auto {
                base
            } else {
                with_dim(Style::new().fg(ctx.theme.muted), ctx.stale)
            };
            let row = Row::new(vec![
                Cell::from(name),
                widgets::right_cell(speed, base),
                widgets::right_cell(setting, setting_style),
            ])
            .style(base);
            TableRow {
                value: speed_value,
                label,
                row,
                selected,
            }
        })
        .collect();

    sort_table_entries(ctx.sort, &mut entries);
    let mut selected_row = None;
    let mut rows = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        if entry.selected {
            selected_row = Some(index);
        }
        rows.push(entry.row);
    }
    // Keep the selected control row visible.
    let visible = usize::from(area.height.saturating_sub(3));
    let offset = selected_row.map_or(0, |row| row.saturating_sub(visible.saturating_sub(1)));
    let widths = vec![
        Constraint::Min(12),
        Constraint::Length(12),
        Constraint::Length(12),
    ];
    widgets::table(
        frame,
        area,
        block,
        &["Fan", "Speed", "Setting"],
        widths,
        rows,
        offset as u16,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(kinds: &[(u8, u16, u8)]) -> Vec<PlannedRow> {
        kinds
            .iter()
            .map(|(kind, height, keep)| PlannedRow {
                kind: *kind,
                height: *height,
                keep: *keep,
            })
            .collect()
    }

    #[test]
    fn fit_plan_drops_the_least_important_rows_first() {
        let gpu = [
            (0, 1, 10),
            (1, 1, 4),
            (2, 1, 3),
            (3, 1, 11),
            (4, 1, 6),
            (5, 1, 8),
            (6, 1, 7),
        ];
        let fitted: Vec<u8> = fit_plan(rows(&gpu), 3).iter().map(|row| row.kind).collect();
        assert_eq!(
            fitted,
            vec![0, 3, 5],
            "temp + load + info survive in display order"
        );
        let fitted: Vec<u8> = fit_plan(rows(&gpu), 1).iter().map(|row| row.kind).collect();
        assert_eq!(fitted, vec![3], "a single row keeps the load gauge");

        // Two-row bars count as a unit: they fit or are dropped whole.
        let cpu = [(0, 1, 10), (1, 1, 11), (2, 2, 5), (3, 1, 8)];
        let fitted: Vec<u8> = fit_plan(rows(&cpu), 4).iter().map(|row| row.kind).collect();
        assert_eq!(fitted, vec![0, 1, 3], "bars dropped, not truncated");
    }

    #[test]
    fn core_voltage_rail_detection() {
        assert!(is_core_voltage_rail("Core #1 VID"));
        assert!(is_core_voltage_rail("CPU Core #12"));
        assert!(is_core_voltage_rail("core #3 vid"));
        assert!(!is_core_voltage_rail("Core (SVI2 TFN)"));
        assert!(!is_core_voltage_rail("SoC (SVI2 TFN)"));
        assert!(!is_core_voltage_rail("GPU Core Voltage"));
        assert!(!is_core_voltage_rail("GPU Core"));
    }

    #[test]
    fn voltage_table_hides_core_rails_unless_zoomed() {
        let slot = |label: &str| Slot {
            hardware_id: "/amdcpu/0".to_string(),
            sensor_id: format!("/amdcpu/0/voltage/{label}"),
            label: label.to_string(),
        };
        let slots = vec![
            slot("Core (SVI2 TFN)"),
            slot("Core #1 VID"),
            slot("Core #2 VID"),
            slot("SoC (SVI2 TFN)"),
        ];
        let docked = voltage_table_slots(&slots, false);
        let labels: Vec<&str> = docked.iter().map(|slot| slot.label.as_str()).collect();
        assert_eq!(labels, vec!["Core (SVI2 TFN)", "SoC (SVI2 TFN)"]);
        assert_eq!(
            voltage_table_slots(&slots, true).len(),
            4,
            "the zoomed detail view shows every rail"
        );
    }

    #[test]
    fn shared_total_is_estimated_as_ram_minus_vram() {
        let estimate = ram_minus_vram_mb(26.9, 101.0, 24_576.0).expect("estimate");
        assert!(
            (estimate - 106_393.6).abs() < 0.01,
            "127.9 GB RAM minus 24 GB VRAM, in MB: {estimate}"
        );
        assert_eq!(
            ram_minus_vram_mb(1.0, 2.0, 5_000.0),
            None,
            "a VRAM total larger than RAM yields no estimate"
        );
    }
}
