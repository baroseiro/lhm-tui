# lhm-tui — Implementation Plan

|              |                                                                                    |
| ------------ | ---------------------------------------------------------------------------------- |
| Derived from | [SPEC.md](SPEC.md) v1.0                                                             |
| Goal         | Ship **v0.1.0** — terminal dashboard for LibreHardwareMonitor (monitoring + the GPU fan anti-stop floor, on by default) |
| Phases       | P0–P6, mapped 1:1 to SPEC §11 milestones M0–M6                                      |
| Effort       | ≈ 10–14 ideal days for one focused developer (see §2)                               |
| Tracking     | Each task `T<phase>.<n>` becomes a GitHub issue labeled `phase-<n>`, closed by its PR |

---

## 0. How to use this plan

- **One phase = one epic.** Do not start a phase before the previous phase's exit checklist (§6)
  is fully green.
- **Task anatomy.** Every task lists **Goal**, **Files**, **Notes** and **Done when**. "Done when"
  is the merge gate — the PR must not merge without it.
- **PR strategy.** One PR per task, or per small task group (≤ 400 diff lines). Every PR adds a
  `CHANGELOG.md` entry under `[Unreleased]` (mandatory — rules in AGENTS.md, "Change tracking").
- **Daily loop.** `cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo test` →
  manual smoke (`cargo run -- --probe` / `cargo run`) against a real LHM instance. Fix red CI
  before starting the next task.
- **Ordering.** The critical path is P0 → P1 → P2 → P3 → P4 → P5 → P6. Within a phase, tasks are
  listed in dependency order; §4 marks the two places where work can be parallelized.

## 1. Ground rules (apply to every task)

| Area            | Rule                                                                                                        |
| --------------- | ----------------------------------------------------------------------------------------------------------- |
| Rust            | Edition 2021, MSRV **1.89**, `#![forbid(unsafe_code)]` in `main.rs`, no `unwrap()` in non-test code (`expect()` only with a message and only at startup) |
| Errors          | Library-ish modules return `thiserror` types; CLI/boundary code uses `anyhow`; user-facing errors must name the fix (see SPEC §10.1 wording) |
| Stdout hygiene  | The TUI owns stdout. Only `--probe`, `--once`, `--dump`, `--print-config`, `--init-config` may write there (SPEC §9). Everything else → `tracing` file log |
| Config          | Never panic on bad config: collect **all** validation errors, print them with their `serde_path_to_error` path, exit 2 (SPEC §8.1) |
| Network         | Poller task only. The UI thread/task never awaits HTTP. All state crosses via channels (SPEC §4.2)          |
| Parsing         | All `/data.json` numeric fields are `Option<f64>` extracted by the lenient parser (SPEC §3.4 Q1–Q4); unknown wire fields/types are skipped, never fatal |
| Panics          | Terminal restore must survive panics and `Ctrl+C` (SPEC §4.2 shutdown)                                       |
| Tests           | New parser heuristic ⇒ new fixture. New widget ⇒ `TestBackend` snapshot. CI (windows + ubuntu + macos) must pass |
| Docs            | Public config field ⇒ SPEC §8.3 + JSON Schema + example config updated in the same PR                        |

## 2. Phase overview

| Phase | SPEC §11 | Scope                                                     | Depends on | Effort (ideal days) |
| ----- | -------- | --------------------------------------------------------- | ---------- | ------------------- |
| P0    | M0       | Bootstrap: repo, CI, config skeleton, terminal lifecycle   | —          | 0.5–1               |
| P1    | M1       | LHM client: HTTP, tolerant parsing, probe, backoff         | P0         | 1.5–2               |
| P2    | M2       | Preset panels with static values (no charts)               | P1         | 1.5–2               |
| P3    | M3       | Charts: history buffers, decimation, braille rendering     | P2         | 2–3                 |
| P4    | M4       | Config depth: overrides, custom panels, themes, schema     | P3         | 1.5–2               |
| P5    | M5       | Polish: overlays, zoom, reload, ascii, `--once`/`--dump`   | P4         | 1.5–2               |
| P6    | M6       | Release: packaging, manifests, docs, v0.1.0 tag            | P5         | ~1                  |

Estimates assume a working LHM instance with auth-optional localhost access for smoke tests, and
assume SPEC is frozen. Spec changes go through a SPEC PR first, then adjust tasks here.

---

## 3. Task breakdown

### Phase P0 — Bootstrap (SPEC §11 M0)

#### T0.1 — Repo scaffold + CI
**Goal:** `cargo init` in place (binary crate `lhm-tui` in repo root), add dependencies from SPEC §4.3, `.gitignore`, MIT `LICENSE`; `CHANGELOG.md` already exists — keep it current from here on.
**Files:** `Cargo.toml`, `src/main.rs`, `.github/workflows/ci.yml`, `LICENSE`, `.gitignore`.
**Notes:** CI matrix `windows-latest`/`ubuntu-latest`/`macos-latest`; steps: fmt --check, clippy `-D warnings`, `cargo test`, `cargo build --release`. Cache via `Swatinem/rust-cache`.
**Done when:** CI green on the scaffold; `cargo run` prints version banner.

#### T0.2 — CLI surface (SPEC §9)
**Goal:** Full clap v4 derive definition for every flag in SPEC §9, env-var bindings (`LHM_TUI_*`), `-v` repeatable verbosity, exit-code constants.
**Files:** `src/cli.rs`, `src/main.rs`.
**Notes:** No behavior yet beyond parsing + dispatch to `--help`/`--version`; un-implemented modes exit 1 (runtime failure, per SPEC §9) with a "not implemented yet" milestone hint until their phases land.
**Done when:** `--help` output matches SPEC §9 (manual diff); unit test asserts precedence skeleton (CLI beats env).

#### T0.3 — Config core (SPEC §8.1, §8.3)
**Goal:** serde structs for the full schema with `#[serde(default)]` defaults, discovery chain (`--config` → `LHM_TUI_CONFIG` → `./lhm-tui.json` → platform dir → defaults), merge order, `--print-config` (secrets → `***`), `--init-config`, unknown-key collection via `serde_ignored`-style capture, duration parsing via `humantime`.
**Files:** `src/config.rs`, `src/cli.rs`, `tests/config.rs`, `lhm-tui.example.json`.
**Notes:** Implement ALL fields now (panels/theme/keys included) so later phases only consume them; validation (`poll_interval_ms` 250–60000 etc.) collects errors, never panics.
**Done when:** `--print-config` round-trips the repo's example config; tests cover precedence, redaction, each validation error, unknown-key warning, `--strict-config` escalation.

#### T0.4 — Terminal lifecycle + placeholder UI
**Goal:** `ratatui::init()/restore()` with panic/`Ctrl+C` safe restore (`Drop` guard), 30 fps `tokio::select!` loop skeleton, quit keys, "connecting…" placeholder frame showing resolved config URL.
**Files:** `src/main.rs`, `src/app.rs` (state stub), `src/ui/mod.rs`.
**Notes:** Wire the redraw-on-change policy now (dirty flag) so P3 doesn't retrofit it; handle resize events.
**Done when:** App opens/restores cleanly on Windows Terminal, conhost and tmux; `q`/`Esc`/`Ctrl+C` exit 0; killing via `Ctrl+C` mid-draw leaves terminal usable.

#### T0.5 — Logging
**Goal:** `tracing` to file only (SPEC §10.2): default path per platform, `--log` override, `-v` levels, 10 MB rotation keeping 3 files.
**Files:** `src/util.rs` (log init), `src/main.rs`.
**Notes:** Use `tracing-subscriber` with an `EnvFilter` built from verbosity (`RUST_LOG` overrides). 10 MB × 3 rotation needs a small hand-rolled writer — `tracing-appender` only rotates by time.
**Shipped:** overlay implemented as a centered popup with live poll stats (T5.1).
**Done when:** Log file appears with startup lines; no output leaks to the TUI.

**Exit checklist P0:** placeholder app runs; `--print-config`/`--init-config` implemented; CI green; log file written.

---

### Phase P1 — LHM client (SPEC §11 M1, §3)

#### T1.1 — Wire types + NaN sanitizer (SPEC §3.3, Q2)
**Goal:** `RawNodeDoc` serde types for the raw tree; string-level sanitizer replacing whole-token `NaN|Infinity|-Infinity` with `null` before `serde_json`.
**Files:** `src/lhm/raw.rs`, `tests/fixtures/*.json`, `tests/parse.rs`.
**Notes:** Sanitizer must not touch content inside strings (regex anchored to value position, not a blind replace). Keep sanitizer as a pure `fn sanitize(&str) -> Cow<str>` for direct unit testing.
**Done when:** Fixtures with bare `NaN` values parse; fuzz-ish test with `"NaN"` inside text fields stays intact.

#### T1.2 — Lenient numeric deserializer (SPEC Q1)
**Goal:** Deserializer accepting `number | string | null` for `Value`/`Raw*`/`Min`/`Max`: strip unit prefix/suffix by parsing the leading (or embedded) float; tolerate `,` decimal comma; empty/`--`/unparseable → `None`.
**Files:** `src/lhm/raw.rs` (custom `Deserialize` impl), `tests/parse.rs`.
**Notes:** Must pass the **version matrix**: 0.9.6-style (`"46.9 °C"` strings in Raw*), master-style (numbers), and mixed/incoherent payloads. Unit set: `°C`, `%`, `RPM`, `MHz`, `GHz`, `V`, `A`, `W`, `GB`, `MB`, `KB/s`, `B/s`, `L/h`, `µS/cm`, `dBA`, `mWh`.
**Done when:** Matrix tests green; no `panic!`/`Err` on any fabricated malformed value.

#### T1.3 — Normalization (SPEC §3.4 Q5–Q6, §5.1)
**Goal:** `parse.rs`: raw tree → `Snapshot`; hardware classification (icon → kind, prefix fallback), sensor flattening from TypeNodes, `HardwareKind::Other`, `SensorKind::Other(String)`, preserve tree order (`serde_json` `preserve_order`).
**Files:** `src/lhm/model.rs`, `src/lhm/parse.rs`, `tests/parse.rs`.
**Notes:** Stable identity: `HardwareId`/`SensorId` only (never node `id`, SPEC Q3). Descend the machine container node above hardware (confirmed in live captures). Count skipped/unknown nodes for the debug overlay.
**Done when:** Fixtures: AMD CPU + NVIDIA GPU, Intel CPU + iGPU, 2 mixed GPUs, memory-only, 1000-sensor synthetic → expected hardware/sensor counts, kinds, ids.

#### T1.4 — HTTP client + `--probe` (SPEC §3.5, §9)
**Goal:** `reqwest::Client` (gzip, timeout, Basic auth, `User-Agent: lhm-tui/<version>`), `fetch_snapshot()`, version extraction (Q7), typed errors `ConnErr | AuthErr | HttpErr(code) | BadPayload`.
**Files:** `src/lhm/client.rs`, `src/cli.rs` (`--probe` branch), `tests/mock_server.rs`.
**Notes:** `--probe` prints URL, LHM version, hardware/sensor counts, binding report table, and exits 3/4 per SPEC §9; redact credentials.
**Done when:** `--probe` against real LHM and against mock (200/401/timeout/garbage) behaves per failure matrix; version `< 0.9.5` warns.

#### T1.5 — Poller task + backoff + status (SPEC §4.2, §10.1)
**Goal:** Dedicated tokio task: interval tick → fetch → `watch::Sender<PollEvent>`; exponential backoff with jitter on failure (`server.retry.*`), reset on success; `AppState.status = Live|Stale|Reconnecting{next_in}`.
**Files:** `src/lhm/client.rs`, `src/app.rs`.
**Notes:** Skip-tick when a request is still in flight (never queue). Expose poll latency + payload size for the `d` overlay.
**Done when:** Integration test with mock that fails N times then recovers: status transitions Live→Stale(age)→Reconnecting(countdown)→Live, backoff schedule asserted (500 ms, 1 s, 2 s, … cap 30 s ± jitter).

#### T1.6 — Parser fixture corpus
**Goal:** Lock in the fixture matrix as data: `lhm-0.9.5.json`, `lhm-0.9.6.json` (formatted Raw*), `lhm-master.json` (numeric Raw*), `lhm-nan.json`, `lhm-intel-igpu.json`, `lhm-2gpu.json`, `lhm-ram-only.json`, `lhm-huge.json`.
**Files:** `tests/fixtures/`, `tests/parse.rs`.
**Notes:** Trim real captures to the relevant subtree; document source + LHM version in each file's sibling `.md` note.
**Done when:** Every fixture has assertions on hardware count, kinds, slot-binding expectations (input for T2.2).

**Exit checklist P1:** `--probe` works against real LHM incl. auth; all fixtures parse; backoff verified; nothing fetches from the UI task.

---

### Phase P2 — Preset panels, no charts (SPEC §11 M2, §5.2, §6)

#### T2.1 — AppState + focus + status line (SPEC §4.2, §6.5)
**Goal:** `AppState` holding latest `Snapshot`, group-free derived indexes (`by_id`, `first(kind, name)`), focus/scroll state, pause flag, status; header (`●` status dot, LHM version, terminal size) and footer (contextual key hints).
**Files:** `src/app.rs`, `src/ui/mod.rs`, `src/ui/widgets.rs` (statusdot).
**Notes:** Stale rendering: dim + `stale {age}` age ticker redraw 1/s while Stale (SPEC §4.2 redraw policy).
**Done when:** Toggling pause (`p`) freezes polls and shows `○`; killing LHM mid-session shows red dot + banner, restoring LHM recovers without restart.

#### T2.2 — Binding engine (SPEC §5.2–5.3)
**Goal:** Evaluate panel slots → sensor ids using the documented candidate lists (CPU/GPU/memory/thermal/fans/power/storage/network); support `panels.sensor_overrides` (`{"sensor": "/id"}` or `{"match": {...}}`); recalc only when the hardware-id set changes; slots go `missing` → `n/a` → auto re-bind.
**Files:** `src/bindings.rs` (engine), `src/lhm/model.rs` (lookup helpers), `tests/bindings.rs`.
**Notes:** Binding report struct is shared with `--probe` (T1.4). Heuristics are the single source of truth in SPEC §5.2 — add each candidate list as a table-driven test.
**Done when:** Fixture tests assert slot→SensorId for cpu/gpu/mem/thermal per fixture, including a pinned-override case and a name-regex case.

#### T2.3 — Formatting helpers (SPEC §6.5, N9)
**Goal:** `ui/format.rs`: temps 1 decimal + `°C`, loads 1 decimal + `%`, memory `x.x / y.y GB` (binary option), clocks `MHz/GHz` auto, power `W`, throughput `KB/s|MB/s`, durations; right-aligned, `unicode-width` aware.
**Files:** `src/ui/format.rs`, unit tests.
**Notes:** Threshold coloring helper `style_for(value, kind)` from `theme.thresholds`.
**Done when:** Table-driven tests over unit/decimal cases; no `NaN`/`inf` ever rendered.

#### T2.4 — Core widgets (SPEC §6.2)
**Goal:** `gauge` (incl. two-tone used/total), `bars` (per-core vertical bars), `table` (scroll, sort modes `value_desc|value_asc|name|none`, threshold colors, LHM min/max columns), `sparkline` (60 s trend), `legend` row.
**Files:** `src/ui/widgets.rs`.
**Notes:** Keep widgets pure (`render(area, &data, &theme, frame)`) so snapshot tests are trivial.
**Done when:** `TestBackend` snapshot per widget; scroll/sort key tests.

#### T2.5 — Preset panels: cpu / gpu / memory / thermal (SPEC §6.3)
**Goal:** Compose widgets into the four must-have presets; derive panel layout per §6.1 (GPU per_row, stacking rules, `max_height` for tables); panel titles (`GPU 0 · RTX 4090`).
**Files:** `src/ui/presets.rs`, `src/ui/mod.rs`.
**Notes:** GPU panels are generated from discovered GPU hardware — count is data-driven, not config-dependent (F10). Focus order follows `ui.layout`.
**Done when:** Live values match LHM GUI within one poll interval; 2-GPU fixture renders two panels at 120+ cols and stacked at 80; `n/a` slots render gracefully.

#### T2.6 — Fans / power / storage / network presets
**Goal:** Remaining presets from §6.3 (table + controls column, totals row, drive/adapter rows).
**Files:** `src/ui/presets.rs`.
**Notes:** Disabled by default (`panels.*.enabled: false` except fans? — keep SPEC default: enabled per §8.2 example: fans/power disabled; storage/network only via `custom`/layout opt-in).
**Done when:** Fixture-driven snapshot tests; enabling via config works.

**Exit checklist P2:** default layout live against real hardware; all failure states render; binding report correct on 2-GPU box; CPU < 2 % (charts not in yet).

---

### Phase P3 — Charts (SPEC §11 M3, §7)

#### T3.1 — History buffers (SPEC §7.2–7.3)
**Goal:** `history.rs`: `SeriesDb` keyed by `SensorId`, `VecDeque<Option<f32>>` ring, capacity = `min(window_s × 1000 / poll_interval_ms, max_points)`, wraparound, gap preservation (paused/missing), per-window `min/max/avg` computed in one pass.
**Files:** `src/history.rs`, unit tests.
**Notes:** Append happens only on applied snapshots; `p` pause suspends polling → gaps appear (desired); history survives LHM reconnects (sensor ids stable until LHM restarts).
**Done when:** Capacity math tests; wraparound keeps last N; window stats correct on gaps.

#### T3.2 — Decimation + autoscale (SPEC §7.1)
**Goal:** Vertical min/max envelope decimation to `2 × width` (braille) / `width` (halfblock) px; nice-step outward rounding (1/2/5×10ⁿ); `y_mode` auto/percent/fixed; `min_span`; shrink hysteresis (only shrink if fits 90 % of current range for ≥ 5 s; grow immediately).
**Files:** `src/charts.rs`, unit tests, `benches/decimate.rs` (criterion, optional to merge but required before P6).
**Notes:** No allocation per frame: reusable scratch buffers on the chart instance. Spike-preservation test is the core one (single-sample 100 % spike must appear at display width).
**Done when:** Property tests (random series → envelope bounds hold); hysteresis transition tests; bench ≤ 5 ms at 900 pts × full width.

#### T3.3 — Graph widget rendering (SPEC §6.2, §7.1)
**Goal:** Multi-series braille line chart inside a bordered block: series colors (palette + cycle), z-order/blend option, latest-value marker, legend (`label 69.5 °C · min/max`), optional grid + x-axis labels when height ≥ 8, gap rendering (pen-up).
**Files:** `src/charts.rs`, `src/ui/widgets.rs`, snapshot tests.
**Notes:** Prefer direct braille cell writes over `canvas::Canvas` if per-series color control fights the API — canvas is allowed but keep a `GraphSeries` abstraction either way. Overlap slightly with T3.2's internal API by design (same PR if small).
**Done when:** Snapshot tests: single series, two series, gaps, flat line (min_span), full-range percent mode, 40×6 tiny chart.

#### T3.4 — Wire charts into presets + window cycling
**Goal:** cpu/gpu/memory panels use graphs for their primary series (temp + load dual; VRAM; RAM used %); `g`/`G` cycles `charts.window_cycle` and re-renders instantly; charts append on each applied snapshot.
**Files:** `src/ui/presets.rs`, `src/app.rs`.
**Notes:** Chart window larger than `history.window_s` clamps (config validation + runtime). Chart
blocks render only when charts are enabled (config, CLI or the runtime `c` toggle); with charts
disabled the P2 gauges/bars layouts remain. `ui.auto_height` makes chart rows the layout's flex
element (grow with spare space, shrink to `MIN_CHART_HEIGHT` first on overflow). Layout
reference: nvtop's per-GPU stacked metric graphs (SPEC §7.4) — ideas only, its code is GPL-3.0-or-later.
**Done when:** Visual soak 15 min @ 1 s: smooth, no axis jitter, panes stable; window cycle updates all charts; CPU budget re-checked (< 1 %, SPEC §13).

#### T3.5 — Marker fallback + capability detection
**Goal:** `charts.marker = braille|halfblock|ascii`; auto-downgrade when the environment can't render braille (legacy conhost detection via `WT_SESSION`/`TERM` heuristics documented in code); `a` key overrides globally.
**Files:** `src/charts.rs`, `src/theme.rs` (env detection), `src/app.rs`.
**Notes:** ASCII mode uses the `_ - =` glyph set; braille and halfblock are implemented, forced modes are snapshot-tested, legacy-conhost sniffing is not (the `a` toggle and `charts.marker` cover it — manual QA matrix pending).
**Done early:** the `?` help overlay from T5.1 shipped with T4.4 (it lists the live, remapped keymap).
**Done when:** Manual QA on conhost and tmux; forced modes snapshot-tested.

**Exit checklist P3:** charts live on real hardware (incl. 2 GPUs); bench + CPU budget met; no allocations in steady-state render path beyond scratch reuse.

---

### Phase P4 — Config depth (SPEC §11 M4, §8)

#### T4.1 — Panel overrides
**Goal:** `panels.<preset>.enabled/show/include/per_row` + `panels.sensor_overrides` fully honored, validated against known slot names (`cpu.temp`, `gpu.2.load`, …); invalid keys → config error with hint listing valid slots.
**Files:** `src/config.rs`, `src/app.rs`, `tests/config.rs`.
**Notes:** `gpu.<n>` overrides apply to the n-th discovered GPU; document index instability across LHM restarts (ids are stable, order may change).
**Done when:** Tests per override kind; a config from SPEC §8.2 example drives a fixture-driven snapshot unchanged.

#### T4.2 — Custom panels (SPEC §8.2, §6.2)
**Goal:** `panels.custom[]` with `id/title/widgets[]`; widget kinds `graph|gauge|bars|table|sparkline`; sensor ref via exact `sensor` id or `match` rule; referenced from `ui.layout` as `custom:<id>`; invalid references are config errors.
**Files:** `src/config.rs`, `src/ui/presets.rs` (custom builder), snapshot tests.
**Notes:** This is the documented escape hatch — keep the implementation boring: reuse T2.4/T3.3 widgets, no new rendering code paths.
**Done when:** Sample custom panel (NVMe temp graph + activity gauge) renders from fixture; unknown sensor id renders `n/a`, not crash.

#### T4.3 — Themes + thresholds (SPEC §6.5, §8.2)
**Goal:** Built-ins (`default`, `gruvbox`, `nord`, `mono`); colors as ratatui names or `#RRGGBB` with automatic downgrade truecolor → 256 → 16; `theme.thresholds.{temperature,load,gpu_memory}` applied everywhere; `NO_COLOR` env + `--no-color`; `--theme <name|file>`; user theme files validated like config.
**Files:** `src/theme.rs`, `src/ui/*`, snapshot tests per theme at 3 color depths.
**Notes:** Color downgrade is a pure function `truecolor→(r,g,b) index` — unit test the mapping table.
**Done when:** Snapshot matrix (3 themes × 3 depths); `NO_COLOR=1` produces the mono rendering; thresholds change bar colors at expected values.

#### T4.4 — Key remapping (SPEC §6.4)
**Goal:** `keys` map action → key list, parsed into `crossterm::event::KeyCombination`s; conflict detection; unknown action/key names are config errors listing valid names.
**Files:** `src/config.rs`, `src/app.rs` (key dispatch table), tests.
**Notes:** Default map lives in one place used by both config defaults and help overlay (T5.1).
**Done when:** Remap test (`quit: ["z"]`), conflict test, help overlay reflects remaps.

#### T4.5 — JSON Schema + example config in CI
**Goal:** Complete `schema/lhm-tui.schema.json` (SPEC Appendix C is abridged — finish it), `lhm-tui.example.json` valid against it, CI step validating via a small `schema`-crate test.
**Files:** `schema/lhm-tui.schema.json`, `lhm-tui.example.json`, `tests/schema.rs`, `ci.yml`.
**Notes:** Keep schema `additionalProperties: false` so editors flag typos, mirroring `--strict-config`.
**Done when:** CI validates example; intentionally-broken fixture fails as expected.

#### T4.6 — Config UX pass
**Goal:** Unknown-key warnings printed on startup (to log + status line in TUI, `--strict-config` escalates); all validation errors aggregated with paths; `--print-config` output is itself a valid config (round-trip test).
**Files:** `src/config.rs`, `tests/config.rs`.
**Notes:** Round-trip property: `--print-config > tmp.json` + `--config tmp.json --print-config` is byte-identical given same env.
**Done when:** Round-trip test green; error message snapshot tests.

**Exit checklist P4:** every field in SPEC §8.3 exercised by ≥ 1 test; example config drives the app end-to-end on real hardware; schema published.

---

### Phase P5 — Polish (SPEC §11 M5)

#### T5.1 — Help + debug overlays, status messages
**Goal:** `?` help (keymap incl. remaps, config path, LHM URL/version); `d` debug overlay (poll latency, payload size, skipped nodes, history sizes, frame time, last error); status message for `m` reset confirmation and startup warnings.
**Files:** `src/ui/mod.rs` (overlays), `src/app.rs`.
**Notes:** Overlays are modal-ish but non-blocking (poll continues).
**Done when:** Snapshot tests per overlay; debug counters match logged values in a soak run.

#### T5.2 — Zoom + sensor browser
**Goal:** `Enter` zooms focused panel fullscreen (toggle); `b` opens searchable sensor browser (fuzzy filter on hardware+name+id) with `Enter` → ad-hoc fullscreen graph for that sensor.
**Files:** `src/app.rs` (focus/zoom), `src/ui/mod.rs`, `src/ui/widgets.rs` (list filter).
**Notes:** Ad-hoc graph is ephemeral (not persisted). Focus handling must survive layout changes (resize, panels appearing).
**Done when:** Key-path tests; snapshot of browser; zoom across all presets.
**Shipped:** zoom keeps the focus ring active while zoomed; the browser filters on a case-insensitive substring over `hardware name sensor name SensorId`, `enter` opens the ad-hoc graph, `q`/`esc`/`b` close it.

#### T5.3 — Reload + reset min/max
**Goal:** `r` reload config from the same path (re-validate; on error keep old config + status message; success applies layout/theme/keys live, keeps history); `m` calls `GET /ResetAllMinMax` with a confirmation status message; failure → banner.
**Files:** `src/app.rs`, `src/lhm/client.rs` (second endpoint fn).
**Notes:** Reload must not touch the poller's interval if unchanged, and vice versa if changed — restart just the interval timer.
**Done when:** Manual test matrix (valid reload / invalid reload / LHM down); integration test for `/ResetAllMinMax` via mock.
**Shipped:** reload re-runs the full discovery + precedence chain via the stored CLI handle; bindings are recomputed immediately (no empty-panel flash) and history survives. Server-level changes (URL/interval/auth/retry) respawn the poller automatically (`pending_poller_restart`). `/ResetAllMinMax` results land on the status line.

#### T5.4 — Responsive rules + ascii + too-small screen
**Goal:** Implement SPEC §6.1 rules fully: 120/80/60 col breakpoints; `<60×20` dedicated screen; `a` toggle ascii; tables drop min/max columns at narrow widths.
**Files:** `src/ui/mod.rs`, `src/ui/widgets.rs`, snapshot tests at 160/120/80/60/50 widths.
**Notes:** Resize events already handled (T0.4); this task is purely layout logic.
**Done when:** Snapshot matrix green; manual resize flicker-free on tmux.

#### T5.5 — `--once` / `--dump`
**Goal:** `--once` renders one snapshot frame to stdout (no alt-screen, no colors when piped); `--dump <PATH|->` emits normalized `Snapshot` JSON (stable field order, `serde` with `preserve_order`); exit codes per SPEC §9.
**Files:** `src/main.rs`, `src/ui/mod.rs` (headless render), `tests/golden.rs`.
**Notes:** `--dump` is the machine-readable product surface — document it in SPEC §9 note + README; golden test on fixture is the contract.
**Done when:** Golden JSON stable via `insta`; `--once` output snapshot; works with `--poll` and auth flags.
**Shipped:** `tests/dump.rs` goldens pin the normalized JSON (timestamp fixed); `--once` renders a fixed-viewport frame without alternate-screen/raw-mode and forces `no_color` when stdout is not a TTY (best-effort; pseudo-terminals always look like TTYs).

#### T5.6 — TUI snapshot suite consolidation + soak harness
**Goal:** Consolidate all `TestBackend` snapshots (layouts × themes × states: live/stale/error/paused/help/zoom/narrow), add a longest-path soak script (`scripts/soak.ps1` + `.sh`: run 1 h against local LHM, log RSS/CPU every 60 s).
**Files:** `tests/snapshots/`, `scripts/soak.*`.
**Notes:** Snapshot updates via `cargo insta review`; CI fails on unreviewed changes.
**Done when:** Suite runs < 15 s; soak results recorded in the M5 issue (memory flat ±2 MB, no fd leak).
**Shipped:** `tests/frames.rs` renders full frames at 160/120/80/60 columns across live/stale/disconnected/paused/zoom/debug/browser/adhoc states; `scripts/soak.ps1` + `scripts/soak.sh` sample RSS/CPU per minute into a CSV (execution happens on a real session, see `docs/qa-matrix.md`).

#### T5.7 — Manual QA matrix (SPEC §12.4)
**Goal:** Execute and record the QA matrix on real hardware/terminals: Windows Terminal + conhost, WSL2, tmux/SSH, kitty; AMD+NVIDIA, Intel iGPU, no-GPU, 2-GPU; LHM 0.9.5/0.9.6; auth on/off.
**Files:** `docs/qa-matrix.md` (checked list with dates/versions).
**Notes:** File bugs as issues with `--probe` output; fix or explicitly defer to v0.1.x before closing P5.
**Done when:** Every row executed; deferrals listed in release notes.
**Shipped:** matrix template lives at `docs/qa-matrix.md` with a per-row checklist; execution happens before tagging v0.1.0 (P6).

**Exit checklist P5:** QA matrix complete; soak passed; `--once`/`--dump` documented; no known S1/S2 bugs.

#### T5.8 — GPU fan anti-stop floor (pulled forward from v0.2)
**Goal:** GPU fan floor (SPEC §7.5), on by default: while a GPU is idle (load ≤ `fan_guard.idle_load_percent` **and** hottest temperature ≤ `idle_temp_c`), hold every bound fan `Control` at `fan_guard.floor_percent`; hand control back to the driver (`value=null` → `SetDefault`) when load/temp cross the hysteresis margins, on `f` toggle-off, on quit, and whenever load/temp are unknown (fail-safe). The only hardware write in v0.1.
**Files:** `src/fan_guard.rs`, `src/lhm/client.rs` (`set_sensor_control`), `src/app.rs` (control task, `f`, exit sweep), `src/ui/mod.rs` (status line + debug), `schema/lhm-tui.schema.json`, `lhm-tui.example.json`, SPEC §7.5/§8.2/§8.3/§3.2/§6.4.
**Notes:** `/Sensor?action=Set` answers HTTP 200 — accept/reject must be read from the JSON body (`result: ok|fail`). Write only on state transitions. `floor_percent < 5` is a validation error so the guard can never stop the fans. On by default (disable with `f` or `fan_guard.enabled: false`) and restricted to auto-stop capable cards (`only_auto_stop`); the only hardware write in v0.1.
**Done when:** Mock-server tests for set/null/rejection; decision-table unit tests (idle, hysteresis, fail-safe, no controls); interaction test for `f` engage → release; frames/help snapshots updated; real-hardware smoke recorded in `docs/qa-matrix.md`.
**Shipped:** `src/fan_guard.rs` (owned per-GPU views + pure decision table + zero-RPM eligibility from the control range / observed parked fans), background control task with per-URL client cache and status-line reporting, `f` toggle (message lands on the status line) + debug overlay line (`N skipped`), hysteresis/fail-safe/validation rails, best-effort `SetDefault()` sweep on quit. Real-hardware smoke is a QA-matrix row (needs an interactive session).

---

### Phase P6 — Release v0.1.0 (SPEC §11 M6, §14)

#### T6.1 — README finalization
**Goal:** Replace `OWNER` placeholders; add real screenshots (`docs/preview.png`) and GIF (`docs/demo.gif`, 15 s: startup, graphs, window cycle, zoom); verify all doc links; README quick start tested on a clean VM.
**Done when:** Clean-VM test passes following README verbatim.

#### T6.2 — Release metadata
**Goal:** Promote `CHANGELOG.md` `[Unreleased]` → `[0.1.0]`, `CONTRIBUTING.md`, issue templates (bug/feature) incl. `--probe` request, `rust-toolchain.toml` (MSRV pin).
**Done when:** Community files present; version bumped to `0.1.0` in `Cargo.toml`.

#### T6.3 — Packaging via `dist`
**Goal:** `dist` config (targets per SPEC §14: windows-msvc x64/arm64, linux-musl x64/arm64, macos x64/arm64), release workflow on `v*` tag, checksums, install script test on Windows + one Linux.
**Done when:** A pre-release tag produces downloadable artifacts that run `--probe` on clean machines.

#### T6.4 — Manifests: winget + scoop
**Goal:** winget manifest PR (`OWNER.lhm-tui`) and scoop bucket entry; both install the windows-msvc binary.
**Done when:** `winget install` / `scoop install` work on clean VM.

#### T6.5 — crates.io publish
**Goal:** `cargo publish --dry-run` clean (README/SPEC not required in package; ensure `exclude` list sane), then publish `0.1.0`; verify `cargo install lhm-tui`.
**Done when:** Install from crates.io works on clean VM; docs.rs builds.

#### T6.6 — Tag + release notes
**Goal:** Tag `v0.1.0`, GitHub Release with generated notes + hand-written highlights (features, known issues from T5.7 deferrals, compatibility statement LHM ≥ 0.9.5).
**Done when:** Release public; README badge resolve; announcement issue closed.

**Exit checklist P6:** all four install channels verified; README/screenshots final; release notes mention the v0.1 scope (monitoring + the fan anti-stop floor, no general writes) and v0.2 control plans.

---

## 4. Critical path & parallelization

```text
P0 ─▶ P1 ─▶ P2 ─▶ P3 ─▶ P4 ─▶ P5 ─▶ P6
        │         ↑
        └─ T1.6 fixture corpus feeds T2.2 binding tests (keep fixtures current from P2 on)

Parallel tracks once the boundary task below exists:
  ┌ after T0.3 (config core):  T0.5 logging, T4.5 schema drafting can proceed off-path
  ┌ after T1.3 (normalization): T2.3 format.rs + T2.4 widgets are UI-only, no network deps
  ┌ after T3.2 (charts core):   T4.3 theme matrix, T5.4 responsive snapshots can be built
    against TestBackend with synthetic data
```

- Highest-risk items are front-loaded: **T1.1/T1.2** (wire-format drift, SPEC §15 R1/R2) and
  **T3.2/T3.3** (custom chart rendering, R4) are the two tasks most likely to need extra budget —
  schedule them first in their phases.
- Everything UI is testable without hardware via fixtures + `TestBackend`; real-hardware smoke
  tests are required only at phase exits.

## 5. Per-PR verification protocol

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test` (unit + integration + snapshot)
4. If parser/config touched: `cargo test --test parse --test config`
5. If network touched: `cargo test --test mock_server`
6. Manual smoke, choose all that apply: `--probe` vs real LHM · `cargo run` 5 min (charts) ·
   `--dump -` diff against expectation
7. `CHANGELOG.md` updated; SPEC/README/schema updated if public surface changed

## 6. Phase exit checklists (summary)

| Phase | Close when…                                                                                        |
| ----- | -------------------------------------------------------------------------------------------------- |
| P0    | placeholder app runs + clean exit; config print/init works; CI green                                 |
| P1    | `--probe` correct on real LHM (auth on/off); fixture matrix green; backoff verified                  |
| P2    | default layout live-matches LHM GUI; failure states render; bindings tested per fixture              |
| P3    | 15 min soak with charts smooth; CPU < 1 %; bench < 5 ms; marker fallback verified                    |
| P4    | every config field tested; example config drives app; schema in CI                                   |
| P5    | QA matrix done; 1 h soak passed (RSS flat); overlays/zoom/reload/once/dump shipped                   |
| P6    | 4 install channels + clean-VM README test pass; v0.1.0 tagged with notes                             |

## 7. v0.1.0 Definition of Done (release checklist)

- [ ] All SPEC §2.1 must-requirements implemented (F1–F11), should-requirements either done or explicitly deferred in release notes (F12–F16)
- [ ] SPEC §2.2 budgets measured and recorded (CPU, RSS, start, binary size)
- [ ] CI green on all three OSes; release artifacts built and smoke-tested
- [ ] Docs: README + SPEC + example config + schema consistent; screenshots real, not mockups
- [ ] No S1/S2 bugs; S3 open count documented
- [ ] `CHANGELOG.md` v0.1.0 complete; version bumped; `v0.1.0` tagged

## 8. Risk burn-down schedule (SPEC §15)

| Risk | Burned down by                        | Deadline (phase) |
| ---- | ------------------------------------- | ---------------- |
| R1/R2 wire-format drift               | T1.1, T1.2, T1.6 fixture matrix        | end of P1        |
| R8 payload size / poll strategy       | measured in T1.5 (`d` overlay stats)   | end of P2        |
| R3 huge-machine performance           | T1.6 `lhm-huge.json` + T3.2 bench      | end of P3        |
| R4 braille on legacy conhost          | T3.5 fallback + snapshot modes         | end of P3        |
| R6 password handling                  | T0.3 (sources) + T4.6 (redaction)      | end of P4        |
| R7 conhost general                    | T5.4 + T5.7 manual matrix              | end of P5        |
| R5 mDNS discovery                     | intentionally deferred to v0.3         | n/a              |

## 9. Decisions to settle during implementation (SPEC open points)

| #   | Decision                                                              | When        | Owner note                                                        |
| --- | --------------------------------------------------------------------- | ----------- | ----------------------------------------------------------------- |
| D1  | `Canvas` widget vs direct braille writes for T3.3                     | start P3    | Try `Canvas` first; fall back if per-series color/overlap fights  |
| D2  | GPU panel ordering rule when hardware order differs between polls     | T2.2        | Sort by `HardwareId` for stability; document in SPEC if adopted    |
| D3  | Whether `--once` defaults to color-when-tty vs never-color-piped      | T5.5        | README examples must match final behavior                          |
| D4  | Ship `y` clipboard action (OSC 52) in v0.1 or defer                  | P5          | Cheap via `crossterm`, but terminal support varies; default defer to v0.1.1 unless trivial |
| D5  | `theme` custom file format = full config `theme` object subset or standalone | T4.3  | Keep identical subset (one parser) unless validation gets messy    |

---

*End of implementation plan — derived from SPEC.md v1.0; update both together.*
