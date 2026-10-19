# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-10-19

### Added

- **Voltages table** (`v` / `panels.voltage` / `"voltage"` in `ui.layout`): every voltage sensor in
  the tree, sorted by value, with LHM min/max — hidden by default like the temperatures table
  (`T7.1`).
- **GPU voltage** on the panel metrics row: `Power · Clock · Voltage`, with the fans still
  right-aligned on the same row when it fits (`T7.2`).
- **CPU metrics strip**: `Power · Clock Average · Clock Effective · Bus Speed · Clock Fabric ·
  Clock Uncore · Clock Memory · Voltage Core · Current TDC`, wrapped onto extra rows when the panel
  is narrow; new binding slots (`cpu.clock_effective`, `cpu.bus_speed`, `cpu.clock_fabric`,
  `cpu.clock_uncore`, `cpu.clock_memory`, `cpu.voltage`, `cpu.current`) and `voltage`/`current`
  slots in `panels.cpu.show` (`T7.3`).
- `voltage`/`current` show slots for CPU, `voltage` for GPU, `panels.voltage`, the `v` keybinding
  and the `toggle_voltage` action; regenerated example config and schema.
- Tests: voltage table snapshot, live-fixture binding checks for every new slot, `v` toggle
  interaction test.
- README captures recorded from a live session with VHS: `docs/preview.png` (hero shot) and
  `docs/demo.gif` (~15 s: gauges → `c` charts → `t` temperatures → `v` voltages → `?` keymap →
  quit), plus the reproducible `docs/demo.tape` and a regeneration note in `CONTRIBUTING.md`.

### Changed

- Info rows no longer truncate labels at five columns (`Clock Effective`, `Voltage Core` render in
  full); the per-frame layout decisions (GPU fan merge, CPU strip rows) live in a shared
  `PanelLayout` used by both the height model and the draw code.
- Min/max table columns format voltages with two decimals and currents with one.
- CI housekeeping: `actions/checkout` bumped to v6 (v4 still targeted Node 20, which GitHub is
  retiring), runner images pinned to `windows-2022` / `ubuntu-24.04` / `macos-15` (the `-latest`
  labels migrate between OS versions; `ubuntu-latest` moves to Ubuntu 26 from October 2026), and
  the workflow now declares `permissions: contents: read`.
- **Repository published publicly.** The old private visibility made both README badges render
  "repo not found" and blocked anonymous users from downloading release assets or running
  `cargo install --git`. Public now, with a repo description and topics; CI badge, release badge,
  release downloads and source installs verified anonymously afterwards.
- Release notes for v0.1.0 polished after the release run: artifact verification notes, the
  real QA status on the reference rig, and a link to the changelog section.

## [0.1.0] - 2026-10-18

### Added

- `SPEC.md` — technical specification v1.0 for the read-only LibreHardwareMonitor TUI:
  LHM web API contract, architecture, config schema, chart engine, milestones.
- `PLAN.md` — phased implementation plan P0–P6 (tasks, exit checklists, risk burn-down)
  derived from `SPEC.md`.
- `README.md` — public project overview: quick start, configuration, keybindings, troubleshooting.
- `AGENTS.md` — repository instructions for AI agents, including mandatory `CHANGELOG.md` practice.
- `CHANGELOG.md` — change tracking for all notable changes (this file).
- Design reference recorded for the multi-GPU realtime charts: nvtop's per-GPU stacked metric
  graphs (SPEC §7.4), with a license-hygiene rule — nvtop is GPL-3.0-or-later, inspiration only.
- Crate scaffold: `Cargo.toml` (dependencies per SPEC §4.3), MIT `LICENSE`, `.gitignore` and
  GitHub Actions CI (fmt + clippy + test + release build on Windows/Linux/macOS) (`T0.1`).
- Full CLI surface per SPEC §9: config/URL/auth/poll/window/theme/ascii/layout/log flags and
  verbosity; `--probe`/`--once`/`--dump` exist as stubs that exit 1 with a milestone hint (`T0.2`).
- Configuration core: full JSON-schema structs with built-in defaults, discovery
  (`--config` → `LHM_TUI_CONFIG` → `./lhm-tui.json` → platform config dir), precedence
  (defaults < file < env < CLI), validation collecting all errors, unknown-key warnings with
  `--strict-config` escalation, `--print-config` (secrets redacted) and `--init-config` (`T0.3`).
- TUI shell: panic-safe terminal lifecycle, redraw-on-change event loop fed by a crossterm input
  thread, remap-aware quit keys, placeholder screen (`T0.4`).
- File logging: 10 MB × 3 size rotation (never stdout), `--log`, `-v`/`-vv` and `RUST_LOG`
  override (`T0.5`).
- `lhm-tui.example.json` generated from the built-in defaults and covered by tests (round-trip,
  no unknown keys, valid values).
- LHM client (P1): `/data.json` polling via `reqwest` (gzip, Basic auth, `User-Agent`, timeout)
  with the tolerant wire format — unit-suffixed strings, and bare `NaN`/`Infinity` sanitizing
  (`T1.1`/`T1.2`).
- Snapshot normalization: hardware classification (icon + id-prefix fallback incl. hyphenated
  `gpu-nvidia`/`gpu-amd` styles), machine-container descent, sensor grouping, stable `SensorId`
  identity, skipped-node counting (`T1.3`).
- `--probe` implemented: LHM version, payload size/latency, per-hardware sensor counts, exit 3
  (connection) / 4 (auth) per SPEC §9; verified against a live instance (9 hardware items,
  276 sensors, NVIDIA + AMD GPUs) (`T1.4`).
- Poller task with `watch` channel, exponential backoff + jitter reset on success, and
  `LinkStatus` transitions (Connecting/Live/Stale/Reconnecting) in the UI (`T1.5`).
- Test suite growth: 7-fixture version matrix (incl. the machine-node shape and NaN quirks), a
  scripted mock LHM server, poller recovery/backoff integration tests — 56 tests total (`T1.6`).
- `src/lib.rs` + thin `main.rs` split so integration tests exercise the same code the binary runs.
- Preset panels with live values (P2): CPU (temp/load gauges, per-core bars, clock, power), one
  panel per GPU (temp, hotspot, load, VRAM ratio, power/fan/clock), memory (used/total + virtual),
  plus sortable, scrollable tables for temperatures, fans (+control %), power (CPU+GPU total),
  storage and network (`T2.1`, `T2.5`, `T2.6`).
- Binding engine: SPEC §5.2 candidate heuristics over `SensorId`s, `panels.sensor_overrides`
  (exact pins, kind/name/name_regex matchers), fingerprint-based rebinding only when the hardware
  set changes (`T2.2`).
- Value formatting helpers (units, ratios, rates) with threshold-aware styling and a real theme
  implementation (color names/hex, warn/crit buckets, stale dimming) (`T2.3`).
- Widgets: gauge, core bars, sortable scrollable table, sparkline — locked in by `insta`
  snapshots (`T2.4`).
- Interaction: pause (`p`, freezes polling), focus cycling (`tab`/`shift+tab`), sort cycling
  (`s`), table scrolling (`j/k`/arrows), once-a-second stale/paused age ticker, and the
  terminal-too-small screen (SPEC N8).
- Real-world fixture `lhm-live-5900xt.json` (scrubbed capture of a 32-core dual-GPU machine) and
  ignored live tests: `cargo test --test live -- --ignored --nocapture` renders the full dashboard
  from a locally running LHM.
- Live charts (P3): per-sensor ring-buffer history, min/max envelope decimation, nice-step
  autoscale with 5 s shrink hysteresis, and a braille/halfblock/ascii graph widget with legends,
  grid and axis row — CPU (load + temp graphs), per-GPU (load + VRAM usage over
  core/hotspot/memory-junction temps) and memory (usage %, synthesized from used/available
  history when no load sensor exists) (`T3.1`–`T3.4`).
- Charts are optional: `charts.enabled` in the config or `--charts`/`--no-charts` on the CLI;
  disabled keeps the compact gauges/bars layout. History is always collected (≈1 MB) so toggling
  is instant.
- Chart window cycling (`g`/`G`) through `charts.window_cycle`, window label in the header;
  `a` toggles ASCII rendering at runtime; chart panels shrink first (down to a 4-row chart) when
  the terminal is short.
- UI fixes from live feedback: fixed-width label column aligns gauges and info rows, table
  headers align with right-aligned numeric columns, and the GPU **memory temperature** is now
  bound and shown (NVIDIA "GPU Memory Junction", AMD "GPU Memory") with exact-name matching so
  hotspot/mem-temp never silently duplicate the core temperature.
- Tests: history and chart unit suites, chart/squeezed-layout snapshots.
- Temperatures table hidden by default (`ui.layout` no longer lists `thermal`); `t` toggles it at
  runtime and `c` toggles charts between graphs and gauges (both remappable via `keys`).
- `ui.auto_height` (default true): charts grow into spare vertical space and shrink first on
  overflow — also fixing a row double-counting bug that needlessly squeezed side-by-side GPUs.
- Chart polish: series shorter than the pixel width right-align, so live charts grow from the
  right edge instead of stretching one sample across the screen.
- Tests: layout-row grouping, auto-height distribution, right-aligned decimation, and the `t`/`c`
  toggle keys.
- Config depth (P4) completed:
  - `panels.*.show` lists now really control CPU/GPU/memory panel rows (temps, load, core bars,
    VRAM, clock, power, fan, virtual memory, …) and panel heights follow the selection.
  - Full config validation with actionable errors: sensor-override slot keys, show-list slots,
    `gpu.include` kinds, custom-widget sensor sources and regexes, theme names/colors, and key
    actions/specs/conflicts — all collected and exit 2; typos are no longer silent.
  - Custom panels (T4.2) render for real: `graph`, `gauge`, `bars`, `sparkline` and `table`
    widgets with exact `sensor` ids or `match` rules, filters for tables/bars, palette-or-hex
    colors and unit overrides; unknown sensors render `n/a`.
  - Themes (T4.3): built-in palettes `default`/`gruvbox`/`nord`/`mono` with per-field overrides,
    automatic truecolor → 256 → 16 downgrades for the detected terminal, working
    `NO_COLOR`/`--no-color` plain rendering, and `--theme <name|file>` (theme files are validated
    like the main config).
  - Key remapping hardened (T4.4) and the `?` help overlay shows the live, remapped keymap.
  - `schema/lhm-tui.schema.json` ships complete (T4.5) and is enforced in tests against the
    example config, plus negative tests proving it rejects broken configs.
  - Startup config warnings surface on the 12 s-highlighted status line in the TUI (T4.6);
    validation output has a snapshot test.
- Key matching ignores the SHIFT bit for character keys (fixes `?` on terminals that report
  `shift+/`), and `--theme` accepts built-in names or a theme file path.
- Polish (P5):
  - Overlays: `d` debug panel (poll latency/payload, hardware/sensor/skipped counts, history
    sizes, chart axes, failed polls, last error), `enter` zooms the focused panel fullscreen,
    `b` opens the sensor browser (type to filter hardware/sensor/id, arrows move, `enter` opens
    an ad-hoc fullscreen graph, `q`/`esc`/`b` close it).
  - `r` reloads the configuration live (full file → env → CLI re-resolution; bindings rebind
    immediately; server-level changes respawn the poller; errors keep the old config and appear on
    the status line). `m` calls `GET /ResetAllMinMax` and reports the result there too.
  - `--once` and `--dump <PATH|->` are implemented (fixed-viewport frame without alternate screen;
    stable normalized JSON, golden-tested); both follow the SPEC exit codes 3/4.
  - `NO_COLOR`/`--no-color` now strip the *entire* UI (header, status, footer previously hardcoded
    their colours); theme palettes gained a dedicated `ok` colour with truecolor → 256 → 16
    downgrades for all chrome.
  - Narrow terminals (< 80 columns) drop the Min/Max columns from temperature tables.
  - `tests/frames.rs` renders full frames at 160/120/80/60 columns across live/stale/disconnected/
    paused/zoom/debug/browser/adhoc states; `scripts/soak.{ps1,sh}` sample RSS/CPU per minute;
    `docs/qa-matrix.md` carries the pre-release checklist.
- **GPU fan anti-stop floor** (SPEC §7.5, `T5.8`): guard that holds every controllable GPU
  fan at a software duty cycle while the GPU is idle and hands it back to the driver's automatic
  curve the moment load or temperature rises — the fix for NVIDIA's zero-RPM parking. On by
  default and limited to auto-stop capable cards (`only_auto_stop`); `f` toggles it (the message
  lands on the status line; the debug overlay shows the guard state) and `fan_guard.*` configures
  it (floor %, idle load/temp, release hysteresis).
  Safety rails: hysteresis instead of flapping, fail-safe to auto when load/temp are
  unknown, back to auto on `f` off and on quit, `floor_percent < 5` rejected (0 % would stop the
  fans the guard exists to keep spinning). Writes use `POST /Sensor?action=Set` with a zero-length
  body (`value=null` → `SetDefault`) — the GET path hangs on errors and carries no result marker —
  and are the only hardware interaction in v0.1.
- `h` opens the help/key-map popup (second default binding of `help`, alongside `?`), and the
  popup now also lists the fixed scroll keys.
- `lhm-tui.example.json` and `schema/lhm-tui.schema.json` gained the `fan_guard` section, and the
  default keymap gained `f` (`toggle_fan_guard`).
- Release pipeline via cargo-dist 0.33 (`dist-workspace.toml`, generated
  `.github/workflows/release.yml`): pushing a `v*` tag builds archives for
  windows-msvc/linux-musl/macos (x64 + arm64), shell + PowerShell installers, per-artifact SHA256
  checksums and a combined `sha256.sum`, then publishes a GitHub Release whose body is this
  changelog's matching section (`T6.3`).
- `CONTRIBUTING.md`, bug/feature issue templates (the bug form asks for `--probe` output) and a PR
  checklist, plus `rust-toolchain.toml` pinning the MSRV toolchain (1.89, rustfmt + clippy) and
  `docs/release-notes-v0.1.0.md` with the hand-written release highlights (`T6.2`/`T6.6`).
- Tests: 171 total.

### Changed

- **First public release: v0.1.0.** Version bumped from 0.0.1; the README install section now
  covers the shipped channels only — GitHub Releases archives, the generated install scripts and
  `cargo install --git` — and the `OWNER` placeholders were replaced with the real account
  (`baroseiro`, `T6.1`). winget, scoop and crates.io are deferred to **v0.1.x** (`T6.4`/`T6.5`).
- **Default flip: charts are off by default** (`charts.enabled: false`). The dashboard opens as the
  instant gauges/bars layout (no chart warm-up); press `c`, pass `--charts` or set
  `charts.enabled: true` for live graphs. History is still collected, so `c` switches instantly.
- **Default flip: the GPU fan anti-stop floor is on by default.** Idle GPUs are held at the 45 %
  floor automatically; disable per session with `f` or permanently with `fan_guard.enabled: false`.
- **The anti-stop floor now only touches cards that can stop their fans** (`fan_guard.only_auto_stop`,
  default true): a fan control whose software range bottoms out above 0 % (most AMD cards — the
  reference RX 7900 XTX reports 15 %) marks the card as "cannot park", and it is never written to.
  Capability is also accepted from observed stopped fans (session-sticky, since a floored fan
  spins). Skipped cards get a one-time status note; the debug overlay reports `N skipped`;
  `only_auto_stop: false` restores flooring every controllable GPU.
- **GPU** panels put every fan right-aligned on the power/clock row (their own row when the panel
  is too narrow for both).
- Short screens: when charts cannot absorb the squeeze (charts off, or table-only layouts), rows
  now compress towards a 4-row minimum instead of being clipped off-screen, and a pressed gauge
  panel drops its least important rows first (memory temp → hot spot → VRAM → fan row → info) so
  load and temperature survive at the smallest sizes.
- The permanent key-hint bar at the bottom is gone: the full key map lives in the `h`/`?` popup
  (which now also lists the scroll keys), the chart window label moved into the header, and the
  bottom line is a status line - the only place messages render (the old second-line toast is
  gone) — highlighted for 12 s after the newest message, muted afterwards, so the fan anti-stop
  state, reload results and errors stay visible.
- `fan_guard.floor_percent` default raised 20 → **45 %**: the anti-stop floor should be clearly
  audible/spinning rather than a barely-moving fan, and 45 % still leaves the driver curve full
  range below the release hysteresis.
- `fan_guard.idle_temp_c` default raised 50 → **65 °C**: idle GPU hot spots commonly sit at
  55–60 °C, so the anti-stop floor engages out of the box instead of waiting for a genuinely
  cold card.
- SPEC §9 documents `--dump` as a public machine surface (timestamps as Unix seconds) and the
  `--once` no-alternate-screen/colour behaviour.
- SPEC sync for the fan floor (T5.8): §1.4 N1 narrowed to "no general sensor writes" (exception:
  the opt-in floor), §3.2 documents `/Sensor?action=Set` via **POST** (HTTP 200 with a `result`
  body — accept and reject must be read from the body; the GET path hangs on errors), §6.4 lists
  `f`, new §7.5 specifies the decision table, §8.2/§8.3 and Appendix C carry `fan_guard`, and
  §12.1/§12.4/§15 R9 cover its tests, QA row and vendor-tool conflict risk.
- GPU panels list **every fan on the card** (`Fan 1 … Fan 2 …`, paired with their controls by
  name; single-fan cards keep the compact `Fan` label; `gpu.<n>.fan` pins one). Fans render
  right-aligned on the power/clock row and fall back to their own row when the panel is too
  narrow.
- `panels.gpu.show` gains `clock` in the defaults (the show-gating in P4 had silently hidden GPU
  clocks, which SPEC §6.3 lists as part of the panel).
- Declared MSRV corrected to Rust **1.89** (was 1.83): the resolved dependency tree
  (ratatui 0.30.2 / uuid 1.89) does not build on older toolchains.
- Spec/plan corrections aligned with the implementation: poll interval floor fixed at 250 ms
  everywhere (was 200 in §8.2 and Appendix C), `serde_ignored` added to the dependency table,
  input events documented as a crossterm input thread, T0.2 stub exit code corrected to 1.
- SPEC §3.3/§3.4 corrected from live-instance verification: hardware nodes are wrapped in a
  machine container node; `Version` is absent on 0.9.6-era servers; id prefixes include the
  hyphenated `gpu-nvidia`/`gpu-amd`/`gpu-intel` and `vram`/`memory` styles.
- Planned layout updated: the binding engine lives in `src/bindings.rs` (SPEC §4.1, PLAN T2.2);
  default `keys.focus_prev` also accepts `backtab` (legacy terminals report Shift+Tab that way)
  and `lhm-tui.example.json` was regenerated.
- SPEC/PLAN sync for P3: `charts.enabled` documented everywhere (§7, §8.2/§8.3, §9, Appendix C),
  GPU `mem_temp` slot row added to §5.2, PLAN T3.5 glyph note corrected to the implemented
  `_ - =` ASCII set.
