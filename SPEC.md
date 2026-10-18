# lhm-tui — Technical Specification

**A terminal dashboard (TUI) for [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)**

|                  |                                                                          |
| ---------------- | ------------------------------------------------------------------------ |
| Status           | Draft v1.0                                                               |
| Target release   | v0.1.0 - monitoring + opt-in GPU fan anti-stop floor                      |
| Implementation   | Rust (edition 2021) · `ratatui` 0.30 · `tokio` · `reqwest` · `serde`      |
| Data source      | LHM built-in **Remote Web Server** (`GET /data.json`), polling-based      |
| Platforms        | TUI runs on Windows / Linux / macOS; the LHM host is Windows by definition |
| Config format    | JSON file + CLI flags + environment variables                             |

---

## Table of Contents

1. [Overview](#1-overview)
2. [Requirements](#2-requirements)
3. [LibreHardwareMonitor Web API (as consumed)](#3-librehardwaremonitor-web-api-as-consumed)
4. [Architecture](#4-architecture)
5. [Domain Model & Sensor Binding Heuristics](#5-domain-model--sensor-binding-heuristics)
6. [TUI Design](#6-tui-design)
7. [Charting Engine](#7-charting-engine)
8. [Configuration](#8-configuration)
9. [Command-Line Interface](#9-command-line-interface)
10. [Error Handling & Diagnosability](#10-error-handling--diagnosability)
11. [Milestones](#11-milestones)
12. [Testing Strategy](#12-testing-strategy)
13. [Performance Budget](#13-performance-budget)
14. [Packaging & Distribution](#14-packaging--distribution)
15. [Risks & Open Questions](#15-risks--open-questions)
16. [Future Work](#16-future-work)
17. [Appendix A — `data.json` Sample Fixture](#appendix-a--datajson-sample-fixture)
18. [Appendix B — Enumerations](#appendix-b--enumerations)
19. [Appendix C — Config JSON Schema (abridged)](#appendix-c--config-json-schema-abridged)

---

## 1. Overview

### 1.1 Problem

LibreHardwareMonitor (LHM) is the de-facto standard for reading CPU/GPU temperatures, clocks, fan
speeds, power draw and memory usage on Windows. Its GUI is a WinForms window and its embedding as
a library requires C#. Users who live in a terminal — on the same machine or over SSH/LAN — have
no good way to watch live sensor data and history.

### 1.2 Solution

`lhm-tui` is a single-binary terminal dashboard that polls LHM's built-in **Remote Web Server**
(`http://host:8085/data.json` by default) at a fixed interval, keeps a rolling history per sensor,
and renders btop-style panels and live charts:

- CPU total load graph with per-core bars, package temperature, clocks and power
- One panel per GPU (multi-GPU aware): load, VRAM usage, core/hotspot temperature, power, fans
- Memory usage (physical + virtual) with history graph
- Sortable temperature table for every thermal sensor in the tree, plus fans, power and
  storage/network presets
- Behavior controlled by a JSON config file, CLI flags and environment variables

Because the data source is HTTP, the dashboard can monitor this machine or any reachable Windows
box running LHM — e.g. from a Linux workstation or over SSH.

### 1.3 Goals

| #   | Goal                                                                                          |
| --- | --------------------------------------------------------------------------------------------- |
| G1  | Zero-configuration start: `lhm-tui` works against `http://127.0.0.1:8085` with no config file |
| G2  | Live charts (≥ 15 min history) for CPU, every GPU, memory, temperatures and load              |
| G3  | Everything meaningful is configurable via a single JSON file                                  |
| G4  | Robust against LHM version differences, malformed values, dropped connections                 |
| G5  | Single static binary, publishable on GitHub Releases / crates.io / winget / scoop              |
| G6  | Sensible on Windows Terminal, WSL, tmux, SSH and non-truecolor terminals                       |

### 1.4 Non-Goals (v0.1.0)

| #   | Non-goal                                                                    |
| --- | --------------------------------------------------------------------------- |
| N1  | General sensor writes (fan/pump control UI, arbitrary `Sensor?action=Set`) — planned v0.2. The single exception is the opt-in, off-by-default fan anti-stop floor (§7.5), which only ever writes GPU fan `Control` sensors and only ever restores automatic control |
| N2  | Direct hardware access; `lhm-tui` never needs admin rights, LHM does         |
| N3  | Bundling/installing/running LHM itself                                      |
| N4  | Persisting history to disk (in-memory ring buffers only; CSV recording is v0.3) |
| N5  | Alerting/notifications (desktop toast, webhook)                              |
| N6  | Windows service / daemon mode                                                |

---

## 2. Requirements

### 2.1 Functional

| ID  | Requirement                                                                                              | Priority |
| --- | -------------------------------------------------------------------------------------------------------- | -------- |
| F1  | Poll `GET {base}/data.json` on an interval (default 1000 ms, configurable 250–60000 ms)                   | must     |
| F2  | Tolerate HTTP Basic auth (username/password from config/env), gzip responses, and transient failures      | must     |
| F3  | Parse the full LHM tree: hardware → sensor-type groups → sensors, with `SensorId`, `Type`, min/max/values | must     |
| F4  | Maintain in-memory rolling history per sensor (≥ 15 min at default interval)                              | must     |
| F5  | Render preset panels: CPU, one per GPU, memory, thermal table, fans, power                                | must     |
| F6  | Live braille line charts with auto-scaling, multiple series, legend, time window cycling                  | must     |
| F7  | JSON config file for server, layout, charts, theme, panel overrides and key remapping                     | must     |
| F8  | CLI flags override config values (`--url`, `--poll`, `--config`, …)                                       | must     |
| F9  | Recognize and allow pinning sensors by exact `SensorId` **or** by match rules (kind/type/name regex)      | must     |
| F10 | Auto-discovery: multiple GPUs of any vendor each get a panel; layout adapts to sensor count                | must     |
| F11 | Connection/auth failure UX: status dot, error banner, exponential backoff, last-known data marked stale   | must     |
| F12 | `--probe` (connectivity check), `--once` (one-shot render or JSON dump), `--init-config`, `--print-config` | should   |
| F13 | Reset LHM min/max (`GET /ResetAllMinMax`) via a keybinding                                                | should   |
| F14 | ASCII fallback mode and `NO_COLOR` compliance                                                              | should   |
| F15 | Config hot-reload via keybinding (`r`)                                                                     | should   |
| F16 | Zoom a panel to fullscreen (`Enter`) and a full-screen sensor browser (`b`)                                | could    |

### 2.2 Non-Functional

| ID  | Requirement                                                                                        |
| --- | -------------------------------------------------------------------------------------------------- |
| N1  | ≤ 1 % of one CPU core average utilization on a 16-thread machine at defaults                       |
| N2  | ≤ 40 MB resident memory with 500 sensors tracked and 15 min history                                 |
| N3  | Cold start to first rendered frame < 250 ms including first successful poll on localhost            |
| N4  | Release binary ≤ 12 MB (MSVC, `lto = "thin"`, `strip = true`)                                       |
| N5  | No unsafe code (`#![forbid(unsafe_code)]` in all crates of the workspace)                           |
| N6  | Rendering artifacts and poll loop are decoupled: a slow/hung LHM server never freezes the UI        |
| N7  | Works in Windows Terminal, conhost (VT), WezTerm, Alacritty, kitty, iTerm2, GNOME Terminal, tmux/SSH  |
| N8  | Minimum terminal size 60×20; below that render a friendly "too small" screen                        |
| N9  | All timings/units rendered with correct SI/binary handling (`GB` vs `GiB` per config)                |

---

## 3. LibreHardwareMonitor Web API (as consumed)

### 3.1 Enabling the server

1. Run LHM **as administrator** (required for most sensors).
2. `Options → Remote Web Server → Run` ✓, default port **8085**.
3. Optional: `Options → Settings → Remote Web Server` → username/password (HTTP Basic auth),
   bind address (localhost or all interfaces), and "Network Discovery" (mDNS)
   in recent versions.
4. `File → Hardware` → tick the hardware you want exposed (unticked items are missing from the API).

LHM has no TLS and no WebSocket; the API is plain HTTP request/response JSON.

### 3.2 Endpoints

| Endpoint                       | Method   | Purpose                                            | v0.1 use     |
| ------------------------------ | -------- | -------------------------------------------------- | ------------ |
| `/data.json`                   | GET      | Full hardware/sensor tree (the primary source)      | **primary**  |
| `/Sensor?action=Get&id=<ID>`   | GET/POST | Single sensor value/min/max/format                  | optional     |
| `/Sensor?action=Set&id=<ID>&value=<pct>` | POST | Set a `Control` sensor's software duty cycle (0-100) | fan anti-stop floor only (§7.5) |
| `/Sensor?action=Set&id=<ID>&value=null` | POST | `SetDefault()`: hand the sensor back to driver control | fan anti-stop floor only (§7.5) |
| `/Sensor?action=ResetMinMax&id=<ID>` | GET/POST | Reset one sensor's min/max                    | future       |
| `/ResetAllMinMax`              | GET      | Reset min/max of all sensors                        | keybinding `m` |
| `/metrics`                     | GET      | Prometheus text exposition (incl. `archive=` history) | future (see §16) |
| `/` , `/index.html`            | GET      | LHM's own web UI (not used)                         | —            |

Notes:

- Responds with `Cache-Control: no-cache`, `Access-Control-Allow-Origin: *`, UTF-8 JSON;
  sends gzip when the client advertises `Accept-Encoding: gzip`.
- On failed Basic auth: HTTP 401.
- `/data.json` returns HTTP 200 only; no ETag / long-poll.
- `/Sensor?action=Set` replies HTTP 200 with a JSON body: `{"result": "ok"}` on success or
  `{"result": "fail", "message": "..."}` when the write is rejected — clients must inspect the
  body, not just the status code. Writing requires LHM to run with the privileges needed to
  control the device (typically administrator on Windows).
- lhm-tui writes via **POST** with an empty body plus `Content-Length: 0` (parameters stay in the
  query; LHM's `HttpListener` rejects bodyless POSTs with 411 otherwise). The GET path is avoided:
  it lets handler exceptions escape uncaught — the request hangs without a reply and reports no
  result marker on success. A 200 JSON body without an explicit `"result": "fail"` is accepted as
  success (older builds answered `{}`).

### 3.3 `/data.json` schema

Root object:

```jsonc
{
  "id": 0,
  "Version": "0.9.6",        // LHM version string (root only)
  "Text": "Sensor",
  "Min": "Min", "Value": "Value", "Max": "Max",
  "ImageURL": "",
  "Children": [ /* single machine node ("SILVERWIN", computer.png) on current
                   builds; the hardware items are its Children. Both shapes parse. */ ]
}
```

Every node shares: `id` (int, **request-scoped sequential index — not stable**), `Text`, `Min`,
`Value`, `Max`, `ImageURL`, `Children[]`. Node variants are discriminated by extra fields:

| Variant      | Discriminator    | Extra fields                                                  |
| ------------ | ---------------- | ------------------------------------------------------------- |
| HardwareNode | `HardwareId`     | `HardwareId` e.g. `/amdcpu/0`, `ImageURL` e.g. `images_icon/cpu.png` |
| TypeNode     | neither id field | `ImageURL` e.g. `images_icon/temperature.png` (groups: "Temperatures", "Load", "Clocks", "Powers", "Fans", "Data", "Throughput", "Voltages") |
| SensorNode   | `SensorId`       | `SensorId` (e.g. `/amdcpu/0/temperature/2`), `Type` (SensorType name), `RawMin`/`RawValue`/`RawMax` |

Nodes without either id are **containers**: group `TypeNode`s (whose children are sensors) and the
**machine node** — current builds wrap all hardware in a single `computer.png` node (e.g.
`"Text": "SILVERWIN"`) whose `Children` are the hardware items. The parser must descend both shapes
(verified against a live instance).

### 3.4 Compatibility quirks (mandatory handling)

| #   | Quirk                                                                                                                                                                                            | Handling rule                                                                                                                                                  |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Q1  | **In LHM ≤ 0.9.6, `RawMin`/`RawValue`/`RawMax` contain the *formatted* string** (`"46.9 °C"`), not numbers ([issue #2253](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/issues/2253)). PR [#2254](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/pull/2254) (merged 2026-03, post-0.9.6) makes them numeric. | Parse `Value`/`Raw*`/`Min`/`Max` with a **dual deserializer**: JSON number → `f64`; JSON string → trim, strip unit suffix/prefix, parse leading float (tolerate `°C`, `%`, `RPM`, `MHz`, `GB`, `B/s`, `,` decimal comma → `.`). Unparseable/empty → `None`. |
| Q2  | Bare `NaN` / `Infinity` literals may appear (server serializes with `AllowNamedFloatingPointLiterals`); strict JSON parsers reject them.                                                            | Before `serde_json`, run a sanitizer that replaces the **whole tokens** `NaN`, `Infinity`, `-Infinity` with `null` (regex anchored to value position: after `:` or `,`, before `,`/`}`/`]`; never inside strings). |
| Q3  | Node `id` is sequential per response and changes with hardware set.                                                                                                                                | Never key maps by `id`. Use `SensorId`/`HardwareId` as stable identity (stable until LHM restarts).                                                             |
| Q4  | Sensor `Value` may be `null`, `""`, or formatted without unit; `Min`/`Max` may be empty at first poll.                                                                                              | All numeric fields are `Option<f64>`; UI renders `—`.                                                                                                            |
| Q5  | Hardware **type is not present as a field** — only icon + identifier prefix reveal it.                                                                                                             | Classify by `ImageURL` (`images_icon/cpu.png` → Cpu, `nvidia.png`/`ati.png`/`intel.png` → Gpu\*, `ram.png` → Memory, …), fall back to `HardwareId` prefix (`amdcpu`/`intelcpu`/`cpu`, `nvidiagpu`/`gpu-nvidia`, `atigpu`/`gpu-amd`, `gpuintel`/`gpu-intel`, `ram`/`vram`/`memory`, `nvme`, `hdd`, `motherboard`, `lpc`, `nic`, `psu`, `battery`, `cooler`, `embeddedcontroller` — classic and hyphenated id styles both occur). Unknown → `Other`. |
| Q6  | Values formatted with unknown/new sensors types may appear after LHM updates.                                                                                                                       | `Type` maps to enum with `Other(String)` catch-all; unknown types still render in tables.                                                                       |
| Q7  | `Version` exists on the root node only, format `major.minor.build` — and is **absent on 0.9.6-era servers** (verified live).                                                                        | Treat as optional: extract once per poll; `--probe` prints `unknown` when missing; compatibility warning only when a parsed version is `< 0.9.5`.                  |
| Q8  | Sensor groups contain a mix of `SensorNode`s only; groups themselves are `TypeNode`s.                                                                                                              | Two-level flattening in the parser.                                                                                                                              |

### 3.5 Request behavior

- One `GET /data.json` per poll interval (server recomputes the tree per request — cheap, but
  intervals < 250 ms are refused by config validation).
- Client sends `Accept-Encoding: gzip` (handled transparently by `reqwest`'s `gzip` feature).
- Client sets `User-Agent: lhm-tui/<version>`.
- No keep-alive tuning needed (reqwest connection pool reuse).
- On HTTP 401 → distinct `AuthError`; on 2xx parse errors → `BadPayload` with the first 200 bytes
  logged at debug level.

---

## 4. Architecture

### 4.1 Crate layout

```
lhm-tui/
├─ Cargo.toml               # workspace root = the binary crate
├─ SPEC.md / README.md / LICENSE
├─ lhm-tui.example.json     # copy of default config
├─ schema/lhm-tui.schema.json
├─ src/
│  ├─ lib.rs                # library root: pub modules shared with integration tests
│  ├─ main.rs               # thin binary entrypoint (CLI parse, dispatch, exit codes)
│  ├─ cli.rs                # clap definitions, env merging, effective-config build
│  ├─ config.rs             # serde structs, defaults, discovery, validation, hot-reload
│  ├─ lhm/
│  │  ├─ client.rs          # reqwest client, fetch_data_json(), probe()
│  │  ├─ raw.rs             # wire types (RawNodeDoc) incl. dual numeric deserializer + NaN sanitizer
│  │  ├─ parse.rs           # RawNodeDoc -> Snapshot (normalization, classification)
│  │  └─ model.rs           # Snapshot, Hardware, Sensor, SensorKind, HardwareKind
│  ├─ history.rs            # RingBuffer, SeriesDb (SensorId -> ring), stats (min/max/avg)
│  ├─ bindings.rs           # Panel slot -> SensorId resolution (SPEC §5.2 heuristics)
│  ├─ app.rs                # AppState, events, focus, panels registry, backoff/stale logic
│  ├─ theme.rs              # Theme, color parsing/downgrade, thresholds, NO_COLOR
│  ├─ charts.rs             # braille canvas, decimation, autoscale, graph widget
│  ├─ ui/
│  │  ├─ mod.rs             # root layout, header/footer, overlays (help, debug, error)
│  │  ├─ presets.rs         # cpu/gpu/memory/thermal/fans/power panel builders
│  │  ├─ widgets.rs         # gauge bar, core bars, sensor table, legend, status dot
│  │  └─ format.rs          # value/unit/rate formatting helpers
│  └─ util.rs
├─ tests/                   # integration: mock LHM server + parser fixtures
│  └─ fixtures/*.json       # captured/crafted data.json samples
└─ .github/workflows/ci.yml, release.yml
```

### 4.2 Runtime model

```
┌──────────────┐  every poll_interval_ms   ┌───────────────┐
│  poll task   │ ──── GET /data.json ────▶ │  LHM server   │
│ (tokio task) │ ◀─── JSON / 401 / err ─── │  :8085        │
└──────┬───────┘                           └───────────────┘
       │ watch::Sender<PollEvent>
       ▼
┌──────────────────────────── AppState (UI thread) ────────────────────────────┐
│ last good Snapshot ── history: RingBuffer per SensorId ── bindings (panel→sensors) │
│ status: Live{at}|Stale{since}|Reconnecting{next_retry_in}   focus/zoom/pause       │
└──────┬───────────────────────────────────────────────────────────────────────┘
       │ terminal.draw() on tick / dirty flag
       ▼
┌─────────────┐    crossterm input thread (keys, resize)
│  ratatui    │ ◀──────────────────────────────────────┐
│  renderer   │                                        │
└─────────────┘                              tokio::select! { tick, poll-event, key }
```

**Concurrency & channels**

| Item          | Choice                                                                                            |
| ------------- | ------------------------------------------------------------------------------------------------- |
| Runtime       | `tokio` (multi-thread, 2 workers), `ratatui::init()/restore()` helpers                             |
| Poller        | Dedicated task owning the `reqwest::Client`; loop `interval.tick()` → fetch → sanitize/parse → `watch::Sender<PollEvent>` (latest-value-wins; UI never blocks on the network) |
| UI loop       | `tokio::select!` over: render tick (`ui.fps`, default 30), key/resize events from the crossterm input thread (`mpsc`), `watch::Receiver` change → update `AppState` |
| History       | Lives on the UI side; on each applied snapshot append raw values to ring buffers. Paused (`p`) suspends polling, not time |
| Backoff       | On error: retry after `initial_ms * factor^n` capped at `max_ms` + ±10 % jitter; success resets, status back to Live; UI keeps rendering last snapshot as `Stale{age}` |
| Redraw policy | Request a redraw when: new snapshot applied, key handled, resize, or every 1 s while Stale (age ticker). Not a busy 30 fps loop |
| Shutdown      | `q`/Ctrl-C → cancel token → restore terminal (must survive panics via `ratatui::restore()` in a guard) |

### 4.3 Dependencies

| Crate                       | Version | Purpose                                                    |
| --------------------------- | ------- | ---------------------------------------------------------- |
| `ratatui`                   | 0.30    | TUI framework, `Canvas`/`Marker::Braille`, layout/widgets  |
| `crossterm`                 | 0.29    | Backend events (via ratatui re-export; input read thread)  |
| `tokio`                     | 1       | Async runtime, `sync::watch`, `time`                       |
| `reqwest`                   | 0.12    | HTTP client, `features = ["json","gzip","rustls-tls"]`     |
| `serde` / `serde_json`      | 1       | Config + wire types, `preserve_order` for stable node order |
| `serde_ignored`             | 0.1     | Unknown config keys → warnings / `--strict-config` errors  |
| `clap`                      | 4       | CLI (`derive`)                                             |
| `anyhow` / `thiserror`      | 1/2     | Error plumbing / typed errors                              |
| `regex`                     | 1       | Sensor match rules (`name_regex`)                           |
| `directories`               | 6       | Platform config dir discovery                              |
| `humantime`                 | 2       | Parse `--poll 1s`, `--window 5m`                            |
| `tracing` + `tracing-subscriber` | 0.1/0.3 | Debug logging to `--log <file>` (never stdout)         |
| `unicode-width`             | 0.2     | Table column alignment (CJK/units)                          |

No unsafe crates in the tree other than what transitive deps require; no `unsafe` in our code.

---

## 5. Domain Model & Sensor Binding Heuristics

### 5.1 Normalized snapshot

```rust
pub struct Snapshot {
    pub fetched_at: SystemTime,
    pub lhm_version: Option<Version>,
    pub hardware: Vec<Hardware>,
}

pub struct Hardware {
    pub id: String,              // HardwareId, e.g. "/amdcpu/0"
    pub name: String,            // "AMD Ryzen 7 7800X3D"
    pub kind: HardwareKind,      // Cpu | GpuNvidia | GpuAmd | GpuIntel | Memory | Storage | …
    pub sensors: Vec<Sensor>,    // flattened from type groups
    pub groups: Vec<SensorGroup>,// original grouping (name + sensor idx) kept for table views
    pub sub: Vec<Hardware>,      // SuperIO children etc.
}

pub struct Sensor {
    pub id: String,              // SensorId "/amdcpu/0/temperature/2"
    pub name: String,            // "Core (Tctl/Tdie)"
    pub kind: SensorKind,        // Temperature | Load | Clock | Power | Fan | Data | …
    pub value: Option<f64>,      // parsed raw numeric (see Q1)
    pub value_text: String,      // original formatted string, rendered verbatim when present
    pub min: Option<f64>,
    pub max: Option<f64>,
}
```

`Hardware::sensors` also yields derived lookups: `first(kind, name_exact/regex)`, `all(kind)`,
`by_id(id)`.

### 5.2 Preset binding heuristics

Heuristics are documented, deterministic, and overridable per panel in config (F9). Ordered
candidate lists; first match wins. `(name)` match is case-insensitive exact, then regex fallback.

**CPU panel** — hardware candidates: `HardwareKind::Cpu` (first; extra CPUs get their own panel;
common case is 1).

| Slot            | SensorType    | Candidate names (in order)                                   |
| --------------- | ------------- | ------------------------------------------------------------ |
| `load` (graph)  | Load          | `CPU Total`, `Total`, first sensor whose id ends `/load/0`    |
| `core_loads`    | Load          | all sensors matching `^CPU Core #\d+$` (bars, up to 64)       |
| `temp` (graph)  | Temperature   | `Core (Tctl/Tdie)`, `CPU Package`, `Core Average`, `CPU Cores`, `Core Max` |
| `clock`         | Clock         | `Cores (Average)`, `CPU Core #1`, `P-Core #0`, `Bus Speed`    |
| `power`         | Power         | `Package`, `CPU Package`, first `Power` sensor                |
| `voltage`       | Voltage       | `CPU Core`, `CPU IOD Hotspot`, first `Voltage`                |

**GPU panel** — one panel per hardware whose kind is `Gpu*`; panel title `GPU {n} · {name}` using
hardware order in the tree (stable within a session). Slots:

| Slot          | SensorType  | Candidate names                                               |
| ------------- | ----------- | -------------------------------------------------------------- |
| `load`        | Load        | `GPU Core`, `D3D 3D`, first `Load`                              |
| `mem_load`    | Load        | `GPU Memory`, `D3D Dedicated Memory Used`, first containing `Memory` |
| `mem_used`    | SmallData/Data | `GPU Memory Used`, `D3D Dedicated Memory Used`               |
| `mem_total`   | SmallData/Data | `GPU Memory Total`, `D3D Dedicated Memory Total`             |
| `temp`        | Temperature | `GPU Core`, `GPU Hot Spot` (drawn as second series when preset) |
| `hotspot`     | Temperature | `GPU Hot Spot`, `Hot Spot`                                      |
| `mem_temp`    | Temperature | `GPU Memory Junction`, `GPU Memory` (VRAM temp; exact names only — never falls back to the core temp) |
| `power`       | Power       | `GPU Package`, `GPU Power`, first `Power`                       |
| `clock_gfx`   | Clock       | `GPU Core`, `GFX Core`                                          |
| `clock_mem`   | Clock       | `GPU Memory`, `Mem Core`                                        |
| `fan`         | Fan         | **all** fans on the card, paired with Controls by name (`GPU Fan 1`, `GPU Fan 2`, …); `gpu.<n>.fan` override pins a single fan |

**Memory panel** — hardware `Memory` (RAM):

| Slot         | SensorType | Names                                     |
| ------------ | ---------- | ----------------------------------------- |
| `used`       | Data       | `Memory Used`, `Used Memory`               |
| `available`  | Data       | `Memory Available`, `Available Memory`     |
| `load`       | Load       | `Memory`                                   |
| `virtual_*`  | Data       | `Virtual Memory Used/Available` (optional) |
| `pagefile`   | Data       | `Page File Used` (optional)                |

**Thermal table** — every `Temperature` sensor across all hardware, sorted by value desc;
threshold-colored (theme thresholds), shows LHM min/max and our window min/max toggleable (key `s`).

**Fans table** — every `Fan` sensor + `Control` sensors rendered as secondary column ("ctr 45 %").

**Power panel** — every `Power` sensor, totals row for CPU+GPU package where available.

**Storage / Network panels** — optional presets: `Temperature`, `Data` (used/free/activity) from
`Storage` hardware; `Throughput` (`Upload`/`Download` heuristics) from `Network` hardware.

### 5.3 Bindings evaluation

Bindings are (re)evaluated only when the hardware set changes (compare sorted `HardwareId` +
`SensorId` set) — not every poll. A binding failure (sensor disappears) marks the slot
`missing` and renders `n/a` after the first missing poll; it re-binds automatically when the
sensor returns. `--probe` prints a binding report (slot → SensorId) to make mismatches diagnosable.

---

## 6. TUI Design

### 6.1 Default layout

Panels flow vertically in the order of `ui.layout` (default `["cpu", "gpu", "memory"]`;
the temperatures table is hidden by default — add `thermal` to the layout or press `t`). With
`ui.auto_height` (default true) charts absorb spare vertical space first, then tables grow up to
`ui.max_height`, so no blank band is left at the bottom.
CPU, memory and each GPU occupy the full width; two GPUs (or GPU + thermal) sit side-by-side when
`width ≥ 120`; thermal/fans/power tables are full-width and grow to fit content up to `ui.max_height`.

```text
┌─ lhm-tui ───────────── AMD Ryzen 9 7950X3D ─────── LHM 0.9.6 ───── 128×40 ──● live ──┐
│ ┌─ CPU ────────────────────────────────────┐ ┌─ GPU 0 · RTX 4090 ────────────────────┐ │
│ │  120°C┤      ╭╮                          │ │   96°C┤   ╭────╮                      │ │
│ │   80°C┤ ╭────╯╰╮   ╭───╮                 │ │   64°C┤ ╭─╯    ╰─╮                    │ │
│ │   40°C└─╯       ╰───╯   ╰───  69.5 °C    │ │   32°C└─╯        ╰───  52.0 °C        │ │
│ │ ▁▃▅█▇▅▃▁▂▄▆█▇▅▃ (32 cores)  Load 41 %     │ │░░░░░░░░ (VRAM 34 %) Load 78 % Mem 8.4/24.0 GB│
│ └──────────────────────────────────────────┘ └───────────────────────────────────────┘ │
│ ┌─ Memory ─────────────────────────────────┐ ┌─ Temperatures (sorted desc) ───────────┐ │
│ │ ██████████████░░░░░░░ 62 %  20.1 / 32.0 GB│ │ Core (Tctl/Tdie)    69.5 °C  ██████░│ │
│ │      ╰╮╭────────╮  ╰───────────  38.9 GB  │ │ GPU Core            52.0 °C  ████░░░│ │
│ └──────────────────────────────────────────┘ └───────────────────────────────────────┘ │
│ q quit  ? help  p pause  g window(5m)  enter zoom  tab focus  r reload  m reset mm  d dbg│
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

Responsive rules:

- `< 120` cols → all panels stacked, GPU panels full width.
- `< 80` cols → GPU side-by-side stats collapse into one-line gauges; tables lose the min/max columns.
- `< 60`×`20` → "Terminal too small — needs ≥ 60×20" screen, poll loop keeps running.
- Height allocation: charts `charts.height` rows (default 8, min 5); tables get `min(content, ui.max_height)`.

### 6.2 Widget catalog

| Widget      | Description                                                                                           |
| ----------- | ----------------------------------------------------------------------------------------------------- |
| `graph`     | Multi-series live line chart (see §7) with legend row: `label value unit · min/max` and color swatch  |
| `bars`      | Row of vertical bars, one per sensor (per-core loads); value colored by threshold                       |
| `gauge`     | Single horizontal gauge: label, value, unit, bar; supports `used/total` two-tone ratio                  |
| `table`     | Scrollable sensor table (columns: name, value, unit, LHM min, LHM max, window min, window max, bar)     |
| `sparkline` | One-line 60-second trend for a value inside headers/gauges                                              |
| `statusdot` | `●` green live / `●` yellow stale (age) / `●` red reconnecting / `○` paused                            |

### 6.3 Preset panels (v0.1)

| Key           | Contents                                                                        |
| ------------- | ------------------------------------------------------------------------------- |
| `cpu`         | temp graph + total-load graph (dual series), per-core bars, clock/power gauges   |
| `gpu`         | one panel per GPU; load graph + VRAM gauge + temp graph (core / hot spot / memory junction) + power/clock gauges + a row with every fan |
| `memory`      | used/available gauge + used-load graph + optional page file/virtual gauges       |
| `thermal`     | table of all temperature sensors, sorted desc (hidden by default; `t` or list it in `ui.layout`) |
| `fans`        | table of fans (+control % column)                                                 |
| `power`       | table of power sensors with total row                                             |
| `storage`     | table: temperature/activity/used/free per drive                                  |
| `network`     | table + sparkline: throughput per adapter                                        |
| `custom:<id>` | user-defined widget list from `panels.custom` (see §8)                            |

### 6.4 Keybindings (defaults, all remappable via `keys` in config)

| Keys                     | Action                                                          |
| ------------------------ | --------------------------------------------------------------- |
| `q`, `Esc`, `Ctrl+C`     | Quit                                                            |
| `?`                      | Help overlay (live keymap with remaps)                           |
| `p`, `Space`             | Pause / resume polling (charts keep rendering last data)         |
| `g` / `G`                | Cycle chart window forward/backward through `charts.window_cycle` |
| `Tab` / `Shift+Tab`      | Move focus to next/previous panel                               |
| `←` `↓` `↑` `→`, `h j k l` | Scroll focused table / move selection                         |
| `Enter`                  | Toggle zoom: focused panel fullscreen                            |
| `b`                      | Sensor browser overlay (searchable list; `Enter` pins sensor to zoom chart) |
| `s`                      | Cycle table sort: value ↓ · value ↑ · name · default             |
| `r`                      | Reload config from disk (keeps history)                          |
| `m`                      | `GET /ResetAllMinMax` on LHM (with 2 s toast confirmation)       |
| `a`                      | Toggle ASCII rendering (overrides config)                        |
| `t`                      | Toggle the temperatures table visibility                         |
| `c`                      | Toggle charts (metric graphs ↔ gauges/bars)                      |
| `f`                      | Toggle the GPU fan anti-stop floor (§7.5); off by default        |
| `d`                      | Debug overlay: poll latency, payload size, sensor/hw counts, last error, LHM version |
| `y`                      | Copy current snapshot summary to clipboard (OSC 52) — optional v0.1.1 |

Key conflicts after remapping → config validation error listing offending actions.

### 6.5 Visual language

- Panel chrome: rounded borders; focused panel border in `theme.accent` + title bold; unfocused `theme.muted`.
- Graph series colors: per-slot palette (`theme.cpu`, `theme.gpu`, `theme.memory`, `theme.network`, `theme.storage`; extra series cycle).
- Thresholds: values ≥ `warn` → `theme.warn`, ≥ `crit` → `theme.crit`; applies to gauges, bars, tables and graph line/latest-value marker. Defaults: 80/95 °C, 90/98 % load, GPU mem 90/97 %.
- Numbers right-aligned, `tabular`-friendly fixed decimals: temps `69.5 °C`, loads `41.2 %`, memory `20.1 / 32.0 GB`, clocks `5.42 GHz`, fans `1205 RPM`, power `142 W`.
- `NO_COLOR` env or `--no-color`: honor by dropping colors but keeping bold/dim and bar glyphs.
  RGB theme colors are downgraded automatically to the terminal's detected color depth
  (truecolor → 256 → 16); built-in palettes: `default`, `gruvbox`, `nord`, `mono` (or a theme
  file via `--theme <path>`).

---

## 7. Charting Engine

Custom widget built on `ratatui::widgets::canvas::Canvas` (or direct braille writes) rather than the
stock `Chart` widget, because we need multi-series braille lines, min/max envelopes, autoscale
hysteresis and legends. Charts are optional: `charts.enabled` (or `--no-charts`) keeps the
gauges/bars-only layouts from P2, and the `c` key toggles at runtime; history is always collected
(cheap, about 1 MB for 500 series) so switching is instant. Series shorter than the pixel width
are right-aligned so live charts grow from the right edge.

### 7.1 Data pipeline per series

1. **Append**: each applied poll pushes `Option<f64>` (gaps preserved) into the series ring buffer.
2. **Clip**: window = `charts.window` (default `5m`) → take last `window / poll_interval` points.
3. **Decimate** to the chart's horizontal pixel budget (braille: `2 × inner_width` px; halfblock:
   `inner_width`): per output column compute min/max of the source samples in that column and
   render a 2-pixel vertical span (classic min/max envelope — preserves spikes, O(n), no allocs via scratch buffers).
4. **Scale**:
   - `y_mode: "percent"` → fixed `[0, 100]` (loads, VRAM).
   - `y_mode: "auto"` → `[min(visible) - pad, max(visible) + pad]`, `pad = max(span × 5 %, charts.min_span)`;
     rounded outward to "nice" steps (1/2/5 × 10ⁿ). `charts.min_span` (default 1.0 °C/%) prevents
     flat lines from filling the chart with noise.
   - **Hysteresis**: only shrink the axis if the new range fits inside 90 % of the current range for
     ≥ 5 s; grow immediately. Prevents axis jitter.
   - `y_mode: "fixed"` → `charts.y_min`/`charts.y_max`.
5. **Draw**: braille glyph per `(x, y)` px cell with per-series color; overlapping series render
   top-most = last in z-order (config `series_order`), or blend to `theme.muted` when
   `charts.blend_overlap` (default off).
6. **Annotate**: right-edge current-value marker `▏`, legend beneath the chart, optional
   horizontal grid at nice steps (`charts.grid`), x-axis time labels (`-5m`, `-2.5m`, now) if height ≥ 8.

### 7.2 Buffers & capacity

`history.window_s` (default 900 s) with `history.max_points_per_series` (default 3600) guard:
capacity per series = `min(window_s × 1000 / poll_interval_ms, max_points)`. Buffer grows lazily to
capacity and then overwrites oldest — a true ring (`VecDeque<Option<f32>>` with one contiguous
allocation via `vec_deque` is fine; f32 halves memory, sensors are ≤ 7 significant digits).
500 series × 900 points × 4 B ≈ 1.8 MB — well within budget.

### 7.3 Statistics

Per series: `min`, `max`, `avg` over the visible window computed during decimation pass (no extra scan).
Tables may show window min/max columns sourced from here. LHM-provided `Min`/`Max` are displayed
separately and labeled "LHM min/max" (since-boot or since-reset values).

### 7.4 Design references & license hygiene

- **nvtop** (<https://github.com/Syllo/nvtop>) is the visual reference for the multi-GPU realtime
  charts: one column per GPU with stacked metric graphs (GPU/memory utilization, temperature,
  power, clocks, PCIe throughput), per-graph legends carrying the live value, and user-selectable
  metrics — the same information model as our `panels.gpu.show` config. Borrow *ideas*: per-GPU
  grouping, consistent metric order across GPUs, dense legends, selectable series.
- **License hygiene (hard rule):** nvtop is **GPL-3.0-or-later** — its code, assets or theme files
  must never be copied into this MIT project (inspiration only). btop (already referenced for the
  overall "btop-style" layout) is Apache-2.0 — permissive, but any reuse requires attribution and
  license notice. Verify the license of any external reference before reusing anything from it;
  the default is an original implementation from this spec.

### 7.5 GPU fan anti-stop floor (opt-in)

Stock NVIDIA fan curves stop the fans entirely below a driver-defined temperature (zero-RPM /
silent mode) and expose no switch to disable it. `fan_guard` works around that by holding every
controllable GPU fan at a software floor while the GPU is idle and handing control back to the
driver as soon as real work starts. It is the only hardware write the tool performs, it is
**disabled by default** (`fan_guard.enabled`, or the `f` key at runtime), and it only ever touches
`Control` sensors (LHM `Sensor.Control.SetSoftware` / `SetDefault`) — never tachometers, voltages
or anything else.

Decision per GPU, evaluated on every applied snapshot (load = bound GPU load; temp = hottest of
core / hot spot / memory junction):

| GPU state | Guard off | Guard on |
| --------- | --------- | -------- |
| idle: load ≤ `idle_load_percent` and temp ≤ `idle_temp_c` | no write | set every bound fan `Control` to `floor_percent` |
| busy: load > idle + `release_load_margin_percent` or temp > idle + `release_temp_margin_c` | no write | `value=null` → `SetDefault()` (driver curve) |
| in between | no write | hold current mode (hysteresis, no flapping) |
| load/temp unknown (sensor missing, stale payload) | no write | release to driver control (fail-safe) |

- Only transitions write HTTP; steady state stays silent. A failed write surfaces as a toast and
  leaves the GPU tracked as released, so the next idle snapshot retries.
- `floor_percent` must be ≥ 5 — 0 % would stop the fans the guard exists to keep spinning — and
  validation rejects smaller values.
- On quit, and on `f` toggle-off, every engaged control gets a best-effort `SetDefault()` (1 s
  timeout per fan) so no manual duty cycle is left behind on the LHM side.
- Writes need LHM to run with the privileges to control the GPU (Windows: administrator). Without
  a `Controls` group the guard reports "no controllable fans" and does nothing.
- `panels.sensor_overrides` (e.g. `gpu.<n>.fan` pinning one fan) decides *which* fans are
  controllable, exactly as for the dashboards; display-only gating (`panels.gpu.show`, `t`, `c`)
  never affects the guard.

---

## 8. Configuration

### 8.1 Discovery & precedence

1. `--config <path>` CLI flag
2. `$LHM_TUI_CONFIG` environment variable
3. `./lhm-tui.json` (current directory)
4. Platform config dir: Windows `%APPDATA%\lhm-tui\config.json`; Linux/macOS `~/.config/lhm-tui/config.json`
   (via `directories` crate)
5. Built-in defaults (no file needed — F1)

Merge order (later wins): **defaults → config file → env vars (`LHM_TUI_*`) → CLI flags.**
Env overrides: `LHM_TUI_URL`, `LHM_TUI_USERNAME`, `LHM_TUI_PASSWORD`, `LHM_TUI_CONFIG`, `LHM_TUI_THEME`.
`--print-config` prints the fully merged config with secrets redacted as `***`.

Format rules: strict JSON (RFC 8259 — **no comments, no trailing commas**). A `$schema` key pointing
at `schema/lhm-tui.schema.json` is allowed and ignored. Unknown keys are collected and reported as
warnings (typo protection: `"pol_interval_ms"` must not be silently ignored); `--strict-config`
escalates them to errors. Semantic references are validated with actionable errors: sensor-override
slot keys, `panels.*.show` slots, `gpu.include` kinds, custom-widget sensor sources and regexes,
theme names/colors, and key actions/specs/conflicts. Malformed JSON or type errors → exit code 2
with a `serde_path_to_error` path (e.g. `charts.window: invalid duration "5x"`).

### 8.2 Annotated example

> Comments are **not** valid JSON — shown here for documentation only. The shipped
> `lhm-tui.example.json` is comment-free and reflects the full built-in defaults (every key
> action and the storage/network panels included — a superset of the fields shown here).

```jsonc
{
  "$schema": "./schema/lhm-tui.schema.json",

  "server": {
    "url": "http://127.0.0.1:8085",
    "username": null,            // HTTP Basic auth; null = anonymous
    "password": null,            // prefer password_env / password_file over inline passwords
    "password_env": null,        // e.g. "LHM_PASSWORD" — read from environment
    "password_file": null,       // read password from first line of this file
    "poll_interval_ms": 1000,    // 250..60000
    "request_timeout_ms": 2000,
    "gzip": true,
    "verify_version": true,      // warn if LHM < 0.9.5
    "retry": { "initial_ms": 500, "max_ms": 30000, "factor": 2.0, "jitter": 0.1 }
  },

  "history": { "window_s": 900, "max_points_per_series": 3600 },

  "ui": {
    "fps": 30,                        // render tick 1..60 (redraw-on-change still applies)
    "layout": ["cpu", "gpu", "memory"],  // thermal hidden by default — t toggles it
    "show_header": true,
    "show_footer": true,
    "max_height": 12,                 // max rows a table panel may grow to
    "auto_height": true,              // stretch panels (charts first) to fill the terminal
    "table_sort": "value_desc",       // value_desc | value_asc | name | none
    "ascii": false,
    "no_color": false,
    "pause_position": false           // start paused (chart history warms after unpause)
  },

  "charts": {
    "enabled": true,                  // false (or --no-charts) → gauges/bars-only panels
    "marker": "braille",              // braille | halfblock | ascii
    "height": 8,
    "window": "5m",                   // initial window
    "window_cycle": ["1m", "5m", "15m"],
    "y_mode": "auto",                 // auto | percent | fixed
    "y_min": null, "y_max": null,
    "min_span": 1.0,
    "grid": true,
    "legend": true,
    "blend_overlap": false
  },

  "theme": {
    "name": "default",                // default | gruvbox | nord | mono (built-ins)
    "accent": "cyan",                 // ratatui color name or "#RRGGBB"
    "text": "#c0caf5",
    "muted": "gray",
    "warn": "#e0af68",
    "crit": "#f7768e",
    "cpu": "#7aa2f7",
    "gpu": "#bb9af7",
    "memory": "#9ece6a",
    "network": "#7dcfff",
    "storage": "#e0af68",
    "thresholds": {
      "temperature": { "warn": 80.0, "crit": 95.0 },
      "load":        { "warn": 90.0, "crit": 98.0 },
      "gpu_memory":  { "warn": 90.0, "crit": 97.0 }
    }
  },

  "panels": {
    "cpu":  { "enabled": true, "show": ["temp", "load", "core_loads", "clock", "power"] },
    "gpu":  { "enabled": true, "include": ["GpuNvidia", "GpuAmd", "GpuIntel"], "per_row": 2,
              "show": ["load", "memory", "temp", "power", "fan", "clock"] },
    "memory": { "enabled": true, "show": ["used", "load", "virtual"] },
    "thermal": { "enabled": true },
    "fans":   { "enabled": false },
    "power":  { "enabled": false },

    // Slot overrides: pin exact SensorIds, or describe a match rule.
    "sensor_overrides": {
      "cpu.temp":  { "sensor": "/amdcpu/0/temperature/2" },
      "gpu.0.load": { "match": { "hardware": "GpuNvidia", "type": "Load", "name": "GPU Core" } }
    },

    // Fully custom panels, referenced from ui.layout as "custom:<id>".
    "custom": [
      {
        "id": "nvme",
        "title": "NVMe",
        "widgets": [
          { "kind": "graph", "sensor": "/nvme/0/temperature/1", "label": "Temp", "unit": "°C", "color": "storage" },
          { "kind": "gauge", "sensor": "/nvme/0/load/0", "label": "Activity", "unit": "%", "y_mode": "percent" }
        ]
      }
    ]
  },

  "fan_guard": {
    "enabled": false,                 // opt-in GPU fan anti-stop floor (§7.5); writes to Control sensors
    "floor_percent": 45.0,            // duty held while idle; 5..=100 (0 would stop the fans)
    "idle_load_percent": 10.0,        // floor engages when load ≤ this …
    "idle_temp_c": 65.0,              // … and the hottest GPU temperature ≤ this
    "release_load_margin_percent": 10.0,  // hysteresis: back to auto above idle + margin
    "release_temp_margin_c": 5.0
  },

  "keys": {
    "quit":  ["q", "esc", "ctrl+c"],
    "pause": ["p", "space"],
    "window": ["g"],
    "reload": ["r"],
    "reset_minmax": ["m"]
  }
}
```

### 8.3 Field reference (summary)

| Path                              | Type            | Default          | Notes                                        |
| --------------------------------- | --------------- | ---------------- | -------------------------------------------- |
| `server.url`                      | string (url)    | `http://127.0.0.1:8085` | Without trailing slash                |
| `server.poll_interval_ms`         | int             | `1000`           | 250–60000                                    |
| `server.request_timeout_ms`       | int             | `2000`           | Per request                                  |
| `server.username`/`password*`     | string/null     | `null`           | Exactly one password source may be set       |
| `history.window_s`                | int             | `900`            | 60–86400                                     |
| `ui.layout`                       | string[]        | `["cpu","gpu","memory"]` | Valid panel keys enforced; `thermal` hidden by default |
| `ui.auto_height`                  | bool            | `true`           | Charts absorb spare space, then tables to `max_height` |
| `charts.enabled`                  | bool            | `true`           | `false`/`--no-charts` keeps gauges/bars only |
| `charts.marker`                   | enum            | `braille`        | Fallback automatically if glyph unsupported  |
| `charts.window(_cycle)`           | duration/string | `"5m"`/`[1m,5m,15m]` | Window > `history.window_s` clamps       |
| `theme.*`                         | color/float     | see example      | Names or `#RRGGBB`; downgraded for 256/16 colors |
| `panels.*.show` / `enabled`       | per preset      | all/true         | Honored by CPU/GPU/memory panels; invalid slots are errors |
| `panels.sensor_overrides`         | map             | `{}`             | Key = `panel.slot` (e.g. `cpu.temp`, `gpu.2.load`) |
| `panels.custom[]`                 | array           | `[]`             | See widget kinds in §6.2                     |
| `fan_guard.enabled`               | bool            | `false`          | Opt-in GPU fan floor; the only hardware write (§7.5) |
| `fan_guard.floor_percent`         | float           | `45.0`           | 5–100 (0 rejected)                           |
| `fan_guard.idle_load_percent` / `idle_temp_c` | float | `10.0` / `65.0` | Engage thresholds; unknown data never engages |
| `fan_guard.release_*_margin_*`    | float           | `10.0` / `5.0`   | Hysteresis before returning to auto control  |
| `keys.*`                          | string[]        | see §6.4         | Validated for conflicts                      |

### 8.4 Config JSON Schema

`schema/lhm-tui.schema.json` (hand-maintained; Appendix C is an abridged excerpt) ships in the
repo and is referenced by `lhm-tui.example.json` via `$schema`. It enables editor validation and
completion, mirrors the runtime validation (including `additionalProperties: false`), and is
enforced by `tests/schema.rs` — which also proves the schema rejects broken configs.

---

## 9. Command-Line Interface

```text
lhm-tui [OPTIONS]

  -c, --config <PATH>         Config file (see §8.1 discovery)
      --url <URL>             LHM base URL                     [env: LHM_TUI_URL]
      --username <USER>       Basic auth user                  [env: LHM_TUI_USERNAME]
      --password <PASS>       Basic auth password              [env: LHM_TUI_PASSWORD]
      --poll <DURATION>       Poll interval (1s, 500ms, …)
      --window <DURATION>     Initial chart window (5m, 30s, …)
      --theme <NAME|PATH>     Built-in theme name or JSON theme file
      --ascii                 ASCII rendering
      --no-color              Disable colors (also honors NO_COLOR)
      --layout <KEYS>         Override ui.layout, e.g. "cpu,gpu,memory,fans"
      --charts                Enable charts (overrides charts.enabled)
      --no-charts             Disable charts; gauges and bars only
      --log <PATH>            Debug log file (never written to the TUI)
  -v, --verbose               Raise log level (repeatable)
      --probe                 Check connectivity/auth, print URL, LHM version, hardware/sensor
                              counts and a binding report, then exit
      --once                  Fetch one snapshot, render once to stdout (no alt-screen), exit
      --dump <PATH|->         Write normalized snapshot JSON to a file or stdout, exit
      --print-config          Print effective merged config (secrets redacted), exit
      --init-config [PATH]    Write default config to path (default: platform config dir), exit
      --strict-config         Unknown config keys become errors
  -V, --version
  -h, --help
```

Exit codes: `0` success · `1` runtime failure · `2` config error · `3` connection failure
(`--probe`/`--once`/`--dump`) · `4` auth failure (`--probe`/`--once`/`--dump`).

`--once`/`--dump` are CI-friendly: `--probe` doubles as a health check for the LHM server.
`--dump` writes the normalized snapshot (stable field order; `fetched_at` as Unix seconds) —
treat it as a public machine surface. `--once` renders a single frame without entering the
alternate screen, restoring the cursor, and disables colours when stdout is not a TTY
(best effort: pseudo-terminals always report as TTYs).

---

## 10. Error Handling & Diagnosability

### 10.1 Failure matrix

| Condition                  | UI behavior                                                                 | Recovery                          |
| -------------------------- | --------------------------------------------------------------------------- | --------------------------------- |
| Connection refused/timeout | Status dot red; banner "Cannot reach LHM at {url} — is Options → Remote Web Server running?"; last snapshot rendered dimmed `stale {age}`; retry countdown shown | Auto (backoff, §4.2)              |
| HTTP 401                   | Banner "Authentication failed — check server.username/password"              | Auto, but banner persists until success |
| HTTP 5xx / malformed JSON  | Banner "Unexpected response (HTTP {code})"; debug log captures payload head  | Auto                              |
| Schema drift (unknown node layouts) | Parser skips unknown nodes, counts them; `d` overlay lists skipped count  | Next release, tolerant by design  |
| LHM version < 0.9.5        | One-time toast warning (config `server.verify_version`)                      | —                                 |
| Terminal too small         | Dedicated screen; polling continues                                          | Auto on resize                    |
| Panic                      | Terminal restored via drop guard; error + log path printed                   | —                                 |

### 10.2 Logging

`tracing` → file only (`--log`, default `%TEMP%/lhm-tui.log` on Windows, `$XDG_STATE_HOME` elsewhere)
with 10 MB rotation (keep 3). Levels: `info` lifecycle (config path, URL, poll stats hourly),
`debug` request timings/sizes, `trace` parse details. `--verbose` raises console log to the log file
(same file; TUI stdout is never polluted — `--once`/`--dump`/`--probe` write to stdout).

---

## 11. Milestones

Each milestone ships green CI and is independently reviewable.

| #   | Milestone                | Deliverable                                                                                              | Acceptance criteria                                                                                 |
| --- | ------------------------ | -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| M0  | Bootstrap                | Cargo project, CI (fmt/clippy/test on win+linux), config structs + discovery + `--print-config`/`--init-config`, `ratatui::init` hello screen with graceful quit | `cargo run` shows placeholder UI; `--print-config` outputs merged defaults; CI green                |
| M1  | LHM client               | `lhm::client` + `raw`/`parse`/`model`, NaN sanitizer, dual numeric deserializer, retry/backoff, `--probe` | `--probe` against real LHM prints version + counts; fixture tests pass incl. Q1/Q2 quirks            |
| M2  | Panels (no charts)       | Presets cpu/gpu/memory/thermal with gauges/bars/tables from bindings; header/footer; error banner        | Live values match LHM GUI within one poll interval; multi-GPU box renders one panel per GPU          |
| M3  | Charts                   | Ring buffers, decimation, autoscale+hysteresis, braille rendering, window cycling, legend/stats          | 15 min @ 1 s smooth on 60 Hz redraw; no axis jitter (visual soak test); CPU < 1 %                     |
| M4  | Config depth             | Full `panels.*` overrides, custom panels, themes + thresholds, key remap, validation errors, JSON schema | Example config round-trips: `--print-config`; every §8.3 field exercised by ≥ 1 test                 |
| M5  | Polish                   | Help/debug overlays, zoom, sensor browser, reload, reset min/max, ascii/no-color, responsive rules       | Manual QA matrix (§12.4) passes on Windows Terminal + tmux + kitty                                   |
| M6  | Release v0.1.0           | README (with screenshots/GIF), packaging (`dist`), winget + scoop manifests, CHANGELOG, tag             | `cargo install lhm-tui` works; release archives attach to GitHub Release; winget validation passes   |

---

## 12. Testing Strategy

### 12.1 Unit

- `lhm::parse`: fixture-driven; asserts hardware classification, sensor flattening, binding
  heuristics, and every quirk: formatted `RawValue` strings (0.9.6) **and** numeric (master),
  bare `NaN`/`Infinity`, `null`/empty values, unit variants (`°C`, `%`, `RPM`, `MHz`, `GB`, `B/s`,
  `L/h`, `µS/cm`, `dBA`), unknown `Type` strings, unknown hardware icon, 1000+ sensor synthetic tree.
- `config`: precedence (file/env/CLI), unknown-key report, duration parsing, validation errors,
  secrets redaction, password source exclusivity.
- `history`: ring capacity math, wraparound, gap handling.
- `charts`: decimation (envelope correctness incl. spike preservation), nice-step rounding,
  hysteresis transitions, percent/fixed modes.
- `fan_guard`: idle engage, hysteresis hold, load/temp release, unknown-data fail-safe (§7.5),
  no-controllable-fans no-op, validation bounds.

### 12.2 Integration

- Mock LHM server (`wiremock` or a 40-line `axum` server) replaying fixtures with injectable
  latency/failures: assert backoff schedule, stale transitions, recovery, 401 handling.
- `--dump -` snapshot stability test (golden JSON via `insta`).

### 12.3 TUI

- `ratatui::backend::TestBackend` + `insta` snapshot tests for: default layout 120×40, narrow
  80×24, too-small 50×15, ascii mode, no-color, error banner, each preset panel, help overlay.
- Key-event handling tests (remaps, conflicts).

### 12.4 Manual QA matrix (pre-release checklist)

- Windows 11 + Windows Terminal (truecolor + conhost fallback), WSL2, tmux over SSH, kitty, wezterm.
- Real hardware: AMD CPU + Nvidia GPU; Intel CPU + iGPU; AMD GPU; 2 GPUs (mixed vendor); no GPU;
  memory-only machine; LHM 0.9.5 and 0.9.6; with and without Basic auth.
- Fan anti-stop floor (§7.5), on real hardware at idle: `f` drops every fan control to the floor
  (zero-RPM stops, fans spin again), a load spike returns them to the driver curve, and quitting
  the TUI (or `f` off) leaves LHM back in automatic control — verify in LHM's own UI.
- 24 h soak: memory flat (±2 MB), no fd leak, reconnect after LHM restart and network flicker.

---

## 13. Performance Budget

| Metric                          | Target       | Measurement                              |
| ------------------------------- | ------------ | ---------------------------------------- |
| UI CPU (defaults, 128×40)       | < 1 % core   | `perf` / Task Manager, 15 min steady     |
| RSS (500 sensors, 15 m history) | < 40 MB      | `--dump`-loaded synthetic harness        |
| Poll latency (`/data.json` localhost) | < 30 ms | debug logs                              |
| Frame time (redraw)             | < 5 ms       | criterion bench on `charts::decimate`    |
| Idle redraws                    | ≤ 1/s        | redraw policy (§4.2)                     |

---

## 14. Packaging & Distribution

- **CI** (`.github/workflows/ci.yml`): `cargo fmt --check`, `clippy -D warnings`, `cargo test`,
  `cargo build --release` on `windows-latest`, `ubuntu-latest`, `macos-latest`.
- **Release** (`release.yml` via [`dist`](https://opensource.axo.dev/cargo-dist/)): tag `v*` →
  archives + installers for `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`,
  `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`,
  `aarch64-apple-darwin`; SHA256SUMS; GitHub Release with generated notes.
- **Channels**: crates.io (`cargo install lhm-tui`), winget manifest (`lhm-tui.lhm-tui`),
  scoop bucket, direct GitHub Releases zip.
- **Docs**: README with screenshots (`docs/preview.png`), animated GIF (`docs/demo.gif`), config
  reference auto-derived from the schema, `CHANGELOG.md` (keep-a-changelog), `LICENSE` (MIT).

---

## 15. Risks & Open Questions

| #   | Risk / question                                                                                          | Mitigation / decision                                                                                      |
| --- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| R1  | LHM changes `/data.json` shape again (Raw* precedent #2254)                                               | Tolerant dual parser + fixture matrix (0.9.5/0.9.6/master); version warning; runtime skip-count in `d` overlay |
| R2  | Value formatting is culture-invariant today — could it become localized?                                  | Numeric extraction is lenient (`.`/`,`); unit suffix matched by leading-digit parse; add fixture when observed |
| R3  | Huge machines (3000+ sensors, 128-core Threadripper) → tall tables, big payloads                          | Payload gzip; table `max_height` + sort/scroll; 2 s minimum poll for >1500 sensors adv. in docs            |
| R4  | Braille glyph rendering in exotic terminals (legacy conhost)                                              | Auto-detect (probe `TERM`/Windows build) + `a` toggle + `charts.marker` config                             |
| R5  | mDNS discovery of LHM instances: service type string differs across LHM versions — worth supporting?      | Deferred to v0.3; `--url` + config cover the need; revisit with `mdns-sd` crate                             |
| R6  | Basic auth password in config file                                                                        | `password_env`/`password_file` first in docs; redaction in `--print-config`; refuse world-readable config file on Unix with a warning |
| R7  | Windows conhost default 9001 scrollback / alt-screen semantics in old builds                              | Require Windows Terminal in docs; graceful VT capability check                                              |
| R8  | `Sensor`-endpoint polling as optimization for pinned-only configs — needed in v0.1?                       | No; keep one `/data.json` fetch (simpler, atomic). Revisit if payload > 500 KB                              |
| R9  | Fan anti-stop floor leaves a software duty cycle on LHM / conflicts with vendor tools (Afterburner)       | Off by default; only transitions write; `SetDefault()` on toggle-off, quit and unknown-data fail-safe; validation forbids floor < 5 %; document conflict caveat (§7.5) |

---

## 16. Future Work

Ordered backlog beyond v0.1.0:

1. **v0.2 — Control**: set fan/pump `Control` sensors (`Sensor?action=Set`), confirmation modal,
   safe-range config, `--read-only` lock flag. *(The v0.1 fan anti-stop floor — §7.5 — is a narrow,
   opt-in slice of this: it only ever writes GPU fan controls and only to auto or the configured
   floor.)*
2. **v0.3 — Recording & alerts**: CSV/JSONL logging per interval; threshold triggers
   (terminal bell, OSC 9 desktop notification, webhook).
3. **v0.3 — History backfill** via the Prometheus `/metrics?archive=<s>` endpoint to pre-fill
   charts at startup.
4. **v0.4 — Multi-host**: tabs/sidebar monitoring several LHM hosts concurrently.
5. **v0.4 — Theme editor** + user theme file drop-in directory.
6. **v0.5 — Plugins/scripting**: template-based custom panels in config (no code), later Lua/WASM.

---

## Appendix A — `data.json` Sample Fixture

Trimmed realistic response (LHM 0.9.6-style: `Raw*` are formatted strings; master style: numbers —
both covered in fixtures):

```json
{
  "id": 0,
  "Version": "0.9.6",
  "Text": "Sensor",
  "Min": "Min", "Value": "Value", "Max": "Max",
  "ImageURL": "",
  "Children": [
    {
      "id": 1, "Text": "AMD Ryzen 7 7800X3D",
      "Min": "", "Value": "", "Max": "",
      "HardwareId": "/amdcpu/0",
      "ImageURL": "images_icon/cpu.png",
      "Children": [
        {
          "id": 2, "Text": "Temperatures", "Min": "", "Value": "", "Max": "",
          "ImageURL": "images_icon/temperature.png",
          "Children": [
            {
              "id": 3, "Text": "Core (Tctl/Tdie)",
              "Min": "43.5 °C", "Value": "46.9 °C", "Max": "77.3 °C",
              "SensorId": "/amdcpu/0/temperature/2", "Type": "Temperature",
              "RawMin": "43.5 °C", "RawValue": "46.9 °C", "RawMax": "77.3 °C",
              "ImageURL": "images/transparent.png", "Children": []
            }
          ]
        },
        {
          "id": 10, "Text": "Load", "Min": "", "Value": "", "Max": "",
          "ImageURL": "images_icon/load.png",
          "Children": [
            {
              "id": 11, "Text": "CPU Total",
              "Min": "2.0 %", "Value": "41.2 %", "Max": "100.0 %",
              "SensorId": "/amdcpu/0/load/0", "Type": "Load",
              "RawMin": "2.0 %", "RawValue": "41.2 %", "RawMax": "100.0 %",
              "ImageURL": "images/transparent.png", "Children": []
            }
          ]
        }
      ]
    },
    {
      "id": 40, "Text": "NVIDIA GeForce RTX 4090",
      "Min": "", "Value": "", "Max": "",
      "HardwareId": "/nvidiagpu/0",
      "ImageURL": "images_icon/nvidia.png",
      "Children": [
        {
          "id": 41, "Text": "GPU Memory",
          "Min": "1861.0 MB", "Value": "8619.9 MB", "Max": "9800.2 MB",
          "SensorId": "/nvidiagpu/0/smalldata/1", "Type": "SmallData",
          "RawMin": 1861.0, "RawValue": 8619.9, "RawMax": 9800.2,
          "ImageURL": "images/transparent.png", "Children": []
        }
      ]
    },
    {
      "id": 60, "Text": "Generic Memory",
      "Min": "", "Value": "", "Max": "",
      "HardwareId": "/ram/0",
      "ImageURL": "images_icon/ram.png",
      "Children": [
        {
          "id": 61, "Text": "Memory Used",
          "Min": "6.2 GB", "Value": "20.1 GB", "Max": "26.4 GB",
          "SensorId": "/ram/0/data/0", "Type": "Data",
          "RawMin": 6.19, "RawValue": 20.11, "RawMax": 26.42,
          "ImageURL": "images/transparent.png", "Children": []
        }
      ]
    }
  ]
}
```

## Appendix B — Enumerations

**SensorType** (as sent in `Type`, newest LHM first): `Voltage`, `Current`, `Power`, `Clock`,
`Temperature`, `Load`, `Frequency`, `Fan`, `Flow`, `Control`, `Level`, `Factor`, `Data`,
`SmallData`, `Throughput`, `TimeSpan`, `Timing`, `Energy`, `Noise`, `Conductivity`, `Humidity`.
(`Timing`/`Noise`/`Conductivity`/`Humidity` may be absent on older builds → `Other`.)

**HardwareType** (not in payload — derived per Q5): `Motherboard`, `SuperIO`, `Cpu`, `Memory`,
`GpuNvidia`, `GpuAmd`, `GpuIntel`, `Storage`, `Network`, `Cooler`, `EmbeddedController`, `Psu`,
`Battery`, `PowerMonitor`.

**Hardware icon → kind** (from `ImageURL`): `cpu.png`→Cpu; `nvidia.png`→GpuNvidia;
`ati.png`→GpuAmd; `intel.png`→GpuIntel; `ram.png`→Memory; `hdd.png`→Storage; `nic.png`→Network;
`mainboard.png`/`chip.png`(lpc)→Motherboard/SuperIO; `psu.png`→Psu; `battery.png`→Battery;
`cooler.png`→Cooler; `embeddedcontroller.png`→EmbeddedController; `computer.png`→Other.

## Appendix C — Config JSON Schema (abridged)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://raw.githubusercontent.com/OWNER/lhm-tui/main/schema/lhm-tui.schema.json",
  "title": "lhm-tui configuration",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "$schema": { "type": "string" },
    "server": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "url": { "type": "string", "format": "uri", "default": "http://127.0.0.1:8085" },
        "username": { "type": ["string", "null"] },
        "password": { "type": ["string", "null"] },
        "password_env": { "type": ["string", "null"] },
        "password_file": { "type": ["string", "null"] },
        "poll_interval_ms": { "type": "integer", "minimum": 250, "maximum": 60000, "default": 1000 },
        "request_timeout_ms": { "type": "integer", "minimum": 100, "default": 2000 },
        "gzip": { "type": "boolean", "default": true },
        "verify_version": { "type": "boolean", "default": true },
        "retry": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "initial_ms": { "type": "integer", "minimum": 50, "default": 500 },
            "max_ms": { "type": "integer", "minimum": 1000, "default": 30000 },
            "factor": { "type": "number", "minimum": 1.0, "default": 2.0 },
            "jitter": { "type": "number", "minimum": 0.0, "maximum": 0.5, "default": 0.1 }
          }
        }
      }
    },
    "history": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "window_s": { "type": "integer", "minimum": 60, "maximum": 86400, "default": 900 },
        "max_points_per_series": { "type": "integer", "minimum": 60, "default": 3600 }
      }
    },
    "ui": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "fps": { "type": "integer", "minimum": 1, "maximum": 60, "default": 30 },
        "layout": { "type": "array", "items": { "type": "string" } },
        "show_header": { "type": "boolean", "default": true },
        "show_footer": { "type": "boolean", "default": true },
        "max_height": { "type": "integer", "minimum": 4, "default": 12 },
        "auto_height": { "type": "boolean", "default": true },
        "table_sort": { "enum": ["value_desc", "value_asc", "name", "none"], "default": "value_desc" },
        "ascii": { "type": "boolean", "default": false },
        "no_color": { "type": "boolean", "default": false },
        "pause_position": { "type": "boolean", "default": false }
      }
    },
    "charts": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "enabled": { "type": "boolean", "default": true },
        "marker": { "enum": ["braille", "halfblock", "ascii"], "default": "braille" },
        "height": { "type": "integer", "minimum": 3, "default": 8 },
        "window": { "type": "string", "default": "5m" },
        "window_cycle": { "type": "array", "items": { "type": "string" } },
        "y_mode": { "enum": ["auto", "percent", "fixed"], "default": "auto" },
        "y_min": { "type": ["number", "null"] },
        "y_max": { "type": ["number", "null"] },
        "min_span": { "type": "number", "minimum": 0.0, "default": 1.0 },
        "grid": { "type": "boolean", "default": true },
        "legend": { "type": "boolean", "default": true },
        "blend_overlap": { "type": "boolean", "default": false }
      }
    },
    "theme": { "type": "object" },
    "panels": { "type": "object" },
    "fan_guard": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "enabled": { "type": "boolean", "default": false },
        "floor_percent": { "type": "number", "minimum": 5, "maximum": 100, "default": 45 },
        "idle_load_percent": { "type": "number", "minimum": 0, "maximum": 100, "default": 10 },
        "idle_temp_c": { "type": "number", "minimum": -20, "maximum": 110, "default": 65 },
        "release_load_margin_percent": { "type": "number", "minimum": 0, "default": 10 },
        "release_temp_margin_c": { "type": "number", "minimum": 0, "default": 5 }
      }
    },
    "keys": { "type": "object", "additionalProperties": { "type": "array", "items": { "type": "string" } } }
  }
}
```

---

*End of specification — v1.0 draft.*
