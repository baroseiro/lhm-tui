//! Preset panels (SPEC §6.3): CPU, per-GPU, memory, thermal, fans, power,
//! storage and network. Panels compose the leaf widgets in `ui::widgets`.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Row};
use ratatui::Frame;

use crate::bindings::{self, Bindings, CpuBindings, GpuBindings, MemoryBindings, Slot};
use crate::config::{Config, TableSort};
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
    match &spec.kind {
        PanelKind::Cpu(_) | PanelKind::Gpu(_) => 7,
        PanelKind::Memory => 6,
        PanelKind::Thermal => (bindings.thermal.len() + 3).clamp(4, max) as u16,
        PanelKind::Fans => (bindings.fans.len() + 3).clamp(4, max) as u16,
        PanelKind::Power => (bindings.power.len() + 4).clamp(4, max) as u16,
        PanelKind::Storage => (bindings.storage.len() + 3).clamp(4, max) as u16,
        PanelKind::Network => (bindings.network.len() + 3).clamp(4, max) as u16,
        PanelKind::Custom(_) => 5,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    spec: &PanelSpec,
    snapshot: &Snapshot,
    bindings: &Bindings,
    cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    sort: TableSort,
    scroll: u16,
) {
    if area.width < 8 || area.height < 3 {
        return;
    }
    match &spec.kind {
        PanelKind::Cpu(index) => {
            if let Some(cpu) = bindings.cpu.get(*index) {
                draw_cpu(frame, area, snapshot, cpu, theme, stale, focused);
            }
        }
        PanelKind::Gpu(index) => {
            if let Some(gpu) = bindings.gpus.get(*index) {
                draw_gpu(
                    frame,
                    area,
                    snapshot,
                    cpu_display_index(*index),
                    gpu,
                    theme,
                    stale,
                    focused,
                );
            }
        }
        PanelKind::Memory => {
            if let Some(memory) = &bindings.memory {
                draw_memory(frame, area, snapshot, memory, theme, stale, focused);
            }
        }
        PanelKind::Thermal => draw_thermal(
            frame, area, snapshot, bindings, cfg, theme, stale, focused, sort, scroll,
        ),
        PanelKind::Fans => draw_fans(
            frame, area, snapshot, bindings, cfg, theme, stale, focused, sort, scroll,
        ),
        PanelKind::Power => draw_power(
            frame, area, snapshot, bindings, cfg, theme, stale, focused, sort, scroll,
        ),
        PanelKind::Storage => draw_storage(
            frame, area, snapshot, bindings, cfg, theme, stale, focused, scroll,
        ),
        PanelKind::Network => draw_network(
            frame, area, snapshot, bindings, cfg, theme, stale, focused, scroll,
        ),
        PanelKind::Custom(id) => {
            let block = panel_block(format!("custom:{id}"), theme, stale, focused);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "custom panels land in M4 (T4.2)",
                    with_dim(Style::new().fg(theme.muted), stale),
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

/// Label with a hardware suffix when the same name occurs on several devices.
fn distinct_label(label: &str, hardware_name: &str, duplicated: bool) -> String {
    if duplicated {
        format!("{label} ({hardware_name})")
    } else {
        label.to_string()
    }
}

// ------------------------------------------------------------------ CPU

fn draw_cpu(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    cpu: &CpuBindings,
    theme: &Theme,
    stale: bool,
    focused: bool,
) {
    let block = panel_block(format!("CPU · {}", cpu.name), theme, stale, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas::<4>(inner);

    // Temperature gauge.
    if let Some((sensor, value)) = slot_value(snapshot, &cpu.temp) {
        widgets::gauge(
            frame,
            rows[0],
            "Temp",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Temperature, value),
            theme,
            stale,
        );
    }

    // Total load gauge.
    if let Some((sensor, value)) = slot_value(snapshot, &cpu.load_total) {
        widgets::gauge(
            frame,
            rows[1],
            "Load",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Load, value),
            theme,
            stale,
        );
    }

    // Per-core bars.
    let core_values: Vec<Option<f64>> = cpu
        .core_loads
        .iter()
        .map(|slot| bindings::slot_sensor(snapshot, slot).and_then(|sensor| sensor.value))
        .collect();
    widgets::bars(frame, rows[2], &core_values, theme, stale);

    // Clock / power info line.
    let mut spans: Vec<Span> = Vec::new();
    if let Some((sensor, _)) = slot_value(snapshot, &cpu.clock) {
        spans.push(Span::styled(
            "Clock ",
            with_dim(Style::new().fg(theme.muted), stale),
        ));
        spans.push(Span::styled(
            format::sensor_value(sensor),
            with_dim(Style::new().fg(theme.text).bold(), stale),
        ));
        spans.push(Span::raw("    "));
    }
    if let Some((sensor, _)) = slot_value(snapshot, &cpu.power) {
        spans.push(Span::styled(
            "Power ",
            with_dim(Style::new().fg(theme.muted), stale),
        ));
        spans.push(Span::styled(
            format::sensor_value(sensor),
            with_dim(Style::new().fg(theme.text).bold(), stale),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[3]);
}

fn cpu_display_index(index: usize) -> usize {
    index
}

// ------------------------------------------------------------------ GPU

#[allow(clippy::too_many_arguments)]
fn draw_gpu(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    index: usize,
    gpu: &GpuBindings,
    theme: &Theme,
    stale: bool,
    focused: bool,
) {
    let block = panel_block(format!("GPU {index} · {}", gpu.name), theme, stale, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<5>(inner);

    if let Some((sensor, value)) = slot_value(snapshot, &gpu.temp) {
        widgets::gauge(
            frame,
            rows[0],
            "Temp",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Temperature, value),
            theme,
            stale,
        );
    }
    if let Some((sensor, value)) = slot_value(snapshot, &gpu.hotspot) {
        widgets::gauge(
            frame,
            rows[1],
            "Hot Spot",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Temperature, value),
            theme,
            stale,
        );
    }
    if let Some((sensor, value)) = slot_value(snapshot, &gpu.load) {
        widgets::gauge(
            frame,
            rows[2],
            "Load",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Load, value),
            theme,
            stale,
        );
    }

    // VRAM gauge (two-tone): used / total, colored by fill ratio.
    let used = slot_value(snapshot, &gpu.mem_used);
    let total = slot_value(snapshot, &gpu.mem_total);
    if let Some((used_sensor, used_value)) = used {
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
            theme.value_style(Metric::GpuMemory, 0.0)
        } else {
            theme.value_style(Metric::GpuMemory, fill_ratio * 100.0)
        };
        widgets::gauge(
            frame, rows[3], "VRAM", &display, fill_ratio, style, theme, stale,
        );
    }

    let mut spans: Vec<Span> = Vec::new();
    for (label, slot) in [
        ("Power", &gpu.power),
        ("Fan", &gpu.fan),
        ("Clock", &gpu.clock),
    ] {
        if let Some((sensor, _)) = slot_value(snapshot, slot) {
            spans.push(Span::styled(
                format!("{label} "),
                with_dim(Style::new().fg(theme.muted), stale),
            ));
            spans.push(Span::styled(
                format::sensor_value(sensor),
                with_dim(Style::new().fg(theme.text).bold(), stale),
            ));
            spans.push(Span::raw("    "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[4]);
}

// ------------------------------------------------------------------ memory

fn draw_memory(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    memory: &MemoryBindings,
    theme: &Theme,
    stale: bool,
    focused: bool,
) {
    let block = panel_block(format!("Memory · {}", memory.name), theme, stale, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(inner);

    let used = slot_value(snapshot, &memory.used);
    let available = slot_value(snapshot, &memory.available);
    if let Some((used_sensor, used_value)) = used {
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
            rows[0],
            "Used",
            &display,
            fill_ratio,
            theme.value_style(Metric::GpuMemory, fill_ratio * 100.0),
            theme,
            stale,
        );
    }
    if let Some((sensor, value)) = slot_value(snapshot, &memory.load) {
        widgets::gauge(
            frame,
            rows[1],
            "Load",
            &format::sensor_value(sensor),
            value / 100.0,
            theme.value_style(Metric::Load, value),
            theme,
            stale,
        );
    }

    let virtual_used = slot_value(snapshot, &memory.virtual_used);
    let virtual_available = slot_value(snapshot, &memory.virtual_available);
    if let Some((used_sensor, used_value)) = virtual_used {
        let display = match virtual_available {
            Some((_, available_value)) => {
                format::data_ratio(&used_sensor.kind, used_value, used_value + available_value)
            }
            None => format::sensor_value(used_sensor),
        };
        let spans = vec![
            Span::styled("Virtual ", with_dim(Style::new().fg(theme.muted), stale)),
            Span::styled(display, with_dim(Style::new().fg(theme.text).bold(), stale)),
        ];
        frame.render_widget(Paragraph::new(Line::from(spans)), rows[2]);
    }
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

/// Sort helper: `(sort key value, row, sort name)`. Rows without a value
/// always sort last, in both directions.
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

#[allow(clippy::too_many_arguments)]
fn draw_thermal(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    bindings: &Bindings,
    _cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    sort: TableSort,
    scroll: u16,
) {
    let block = panel_block("Temperatures".to_string(), theme, stale, focused);
    let labels: Vec<String> = bindings
        .thermal
        .iter()
        .map(|slot| slot.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = bindings
        .thermal
        .iter()
        .zip(duplicated)
        .filter_map(|(slot, duplicated)| {
            let sensor = bindings::slot_sensor(snapshot, slot)?;
            let hardware_name = snapshot
                .hardware(&slot.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&slot.label, &hardware_name, duplicated);
            let style = with_dim(theme.sensor_style(&sensor.kind, sensor.value), stale);
            let row = Row::new(vec![
                ratatui::widgets::Cell::from(name.clone()),
                widgets::right_cell(format::sensor_value(sensor), style),
                widgets::right_cell(
                    format::stat(&sensor.kind, sensor.min),
                    with_dim(Style::new().fg(theme.muted), stale),
                ),
                widgets::right_cell(
                    format::stat(&sensor.kind, sensor.max),
                    with_dim(Style::new().fg(theme.muted), stale),
                ),
            ])
            .style(style);
            Some((sensor.value, name, row))
        })
        .collect();

    sort_table_rows(sort, &mut rows);
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
        scroll,
        theme,
        stale,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_fans(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    bindings: &Bindings,
    _cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    sort: TableSort,
    scroll: u16,
) {
    let block = panel_block("Fans".to_string(), theme, stale, focused);
    let labels: Vec<String> = bindings
        .fans
        .iter()
        .map(|row| row.fan.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = bindings
        .fans
        .iter()
        .zip(duplicated)
        .map(|(fan, duplicated)| {
            let hardware_name = snapshot
                .hardware(&fan.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&fan.fan.label, &hardware_name, duplicated);
            let value = slot_value(snapshot, &Some(fan.fan.clone()));
            let row = Row::new(vec![
                ratatui::widgets::Cell::from(name.clone()),
                widgets::right_cell(
                    slot_display(snapshot, &Some(fan.fan.clone())),
                    with_dim(Style::new().fg(theme.text), stale),
                ),
                widgets::right_cell(
                    slot_display(snapshot, &fan.control),
                    with_dim(Style::new().fg(theme.muted), stale),
                ),
            ]);
            (value.map(|(_, v)| v), name, row)
        })
        .collect();

    sort_table_rows(sort, &mut rows);
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
        scroll,
        theme,
        stale,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_power(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    bindings: &Bindings,
    _cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    sort: TableSort,
    scroll: u16,
) {
    let block = panel_block("Power".to_string(), theme, stale, focused);
    let labels: Vec<String> = bindings
        .power
        .iter()
        .map(|slot| slot.label.clone())
        .collect();
    let duplicated = duplicate_labels(&labels);

    let mut rows: Vec<(Option<f64>, String, Row<'static>)> = bindings
        .power
        .iter()
        .zip(duplicated)
        .filter_map(|(slot, duplicated)| {
            let sensor = bindings::slot_sensor(snapshot, slot)?;
            let hardware_name = snapshot
                .hardware(&slot.hardware_id)
                .map(|hardware| hardware.name.clone())
                .unwrap_or_default();
            let name = distinct_label(&slot.label, &hardware_name, duplicated);
            let style = with_dim(Style::new().fg(theme.text), stale);
            let row = Row::new(vec![
                ratatui::widgets::Cell::from(name.clone()),
                widgets::right_cell(format::sensor_value(sensor), style),
            ])
            .style(style);
            Some((sensor.value, name, row))
        })
        .collect();

    sort_table_rows(sort, &mut rows);
    let mut rows: Vec<Row<'static>> = rows.into_iter().map(|(_, _, row)| row).collect();

    // Total row: CPU package + GPU packages where available.
    let mut total = 0.0f64;
    let mut have_total = false;
    for slot in bindings.cpu.iter().filter_map(|cpu| cpu.power.as_ref()) {
        if let Some(sensor) = bindings::slot_sensor(snapshot, slot) {
            if let Some(value) = sensor.value {
                total += value;
                have_total = true;
            }
        }
    }
    for slot in bindings.gpus.iter().filter_map(|gpu| gpu.power.as_ref()) {
        if let Some(sensor) = bindings::slot_sensor(snapshot, slot) {
            if let Some(value) = sensor.value {
                total += value;
                have_total = true;
            }
        }
    }
    if have_total {
        let style = with_dim(Style::new().fg(theme.accent).bold(), stale);
        rows.insert(
            0,
            Row::new(vec![
                ratatui::widgets::Cell::from("Total (CPU+GPU)").style(style),
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
        scroll,
        theme,
        stale,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_storage(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    bindings: &Bindings,
    _cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    scroll: u16,
) {
    let block = panel_block("Storage".to_string(), theme, stale, focused);
    let text_style = with_dim(Style::new().fg(theme.text), stale);
    let muted = with_dim(Style::new().fg(theme.muted), stale);

    let rows: Vec<Row<'static>> = bindings
        .storage
        .iter()
        .map(|drive| {
            let temp_style = drive
                .temperature
                .as_ref()
                .and_then(|slot| bindings::slot_sensor(snapshot, slot))
                .map(|sensor| with_dim(theme.sensor_style(&sensor.kind, sensor.value), stale))
                .unwrap_or(muted);
            Row::new(vec![
                ratatui::widgets::Cell::from(drive.name.clone()).style(text_style),
                widgets::right_cell(slot_display(snapshot, &drive.temperature), temp_style),
                widgets::right_cell(slot_display(snapshot, &drive.used_space), text_style),
                widgets::right_cell(slot_display(snapshot, &drive.activity), text_style),
                widgets::right_cell(slot_display(snapshot, &drive.read_rate), muted),
                widgets::right_cell(slot_display(snapshot, &drive.write_rate), muted),
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
        scroll,
        theme,
        stale,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_network(
    frame: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    bindings: &Bindings,
    _cfg: &Config,
    theme: &Theme,
    stale: bool,
    focused: bool,
    scroll: u16,
) {
    let block = panel_block("Network".to_string(), theme, stale, focused);
    let text_style = with_dim(Style::new().fg(theme.text), stale);
    let muted = with_dim(Style::new().fg(theme.muted), stale);

    let rows: Vec<Row<'static>> = bindings
        .network
        .iter()
        .map(|adapter| {
            Row::new(vec![
                ratatui::widgets::Cell::from(adapter.name.clone()).style(text_style),
                widgets::right_cell(slot_display(snapshot, &adapter.upload), text_style),
                widgets::right_cell(slot_display(snapshot, &adapter.download), text_style),
                widgets::right_cell(slot_display(snapshot, &adapter.uploaded), muted),
                widgets::right_cell(slot_display(snapshot, &adapter.downloaded), muted),
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
        scroll,
        theme,
        stale,
    );
}
