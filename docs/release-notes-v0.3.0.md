# lhm-tui v0.3.0 - release notes (hand-written body)

<!--
Release body source for the `v0.3.0` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**A dashboard polish pass.** v0.3.0 tightens the panels introduced in v0.2.0:
GPU shared memory, a dedicated fan row, labelled per-core bars, shorter metric
labels and percentage memory readouts.

## Highlights

- **Shared GPU memory** - a `Shared` gauge sits directly under `VRAM` with the
  same `used / total (pct %)` template (`Shared 731 / 104796 MB (1%)`). When the
  card reports no shared total (NVIDIA), the total is estimated as system RAM
  total minus this card's VRAM, so the row still shows a usable percentage.
- **Fans get their own row** - GPU panels keep `Pwr · Clk · Volt` on the metrics
  row and render every fan on a dedicated line underneath (`Fan 1 608 RPM ·
  Fan 2 665 RPM`), at every terminal width.
- **Per-core bars are labelled** - the CPU bar row carries a muted `Per-Core`
  label and one column of spacing between cores, so the individual bars are
  easier to tell apart.
- **Short metric labels** - `Pwr`, `Clk` and `Volt` on the GPU row; `Pwr`,
  `TDC`, `Volt Core`, `Clk Avg`, `Clk Eff`, `Clk Fabric`, `Clk Uncore` and
  `Clk Memory` on the CPU strip.
- **Fixed two-row CPU template** - `Pwr · TDC · Volt Core` with right-aligned
  `Clk Avg · Clk Eff`, then `Bus Speed · Clk Fabric · Clk Uncore · Clk Memory`.
- **Memory readouts** - `Virtual` is a gauge like `Used`; used/virtual memory
  and GPU VRAM show `used / total (pct %)` (`26.9 / 127.9 GB (21%)`, VRAM keeps
  MB: `599 / 24576 MB (2%)`).
- **Voltages table** - GPU rails carry the card name (`GPU Core Voltage
  (NVIDIA GeForce RTX 3090)`); per-core rails stay in the zoomed detail view
  (`enter`) and out of the docked table.

## Install

1. Download the archive for your platform from the assets below
   (`lhm-tui-x86_64-pc-windows-msvc.zip`, `…-unknown-linux-musl.tar.xz`,
   `…-apple-darwin.tar.xz`, plus `aarch64` variants) and put `lhm-tui` on your
   `PATH`.
   Or use the generated installer: `lhm-tui-installer.ps1` (Windows) /
   `lhm-tui-installer.sh` (Linux/macOS).
2. Or build from source: `cargo install --git https://github.com/baroseiro/lhm-tui`
3. Requirements: LibreHardwareMonitor **0.9.5+** running as administrator with
   `Options → Remote Web Server → Run` enabled, plus a terminal ≥ 60×20
   (80×24 recommended).
4. Verify downloads against `sha256.sum` (each archive also ships a `.sha256` sidecar).

## Upgrading from v0.2.0

No breaking changes and no new keybindings. GPU panels gain the `Shared` row and
move fans to their own line; the CPU strip uses the fixed two-row template. The
only new config surface is the optional `gpu.<n>.shared_used` /
`gpu.<n>.shared_total` sensor-override slots. Chart, `fan_guard` and polling
defaults are unchanged.

## Known issues & deferrals

- **winget, scoop and crates.io** install channels remain deferred to **v0.3.x**
  (PLAN T6.4/T6.5).
- **Estimated NVIDIA shared total**: LHM does not report a shared-memory total
  for NVIDIA cards, so it is derived from RAM minus VRAM. Treat that percentage
  as an estimate; AMD cards use the total reported by LHM.
- **Manual QA matrix** (`docs/qa-matrix.md`): the reference rig (Windows 11 +
  Windows Terminal, Ryzen 9 5900XT, RTX 3090 + RX 7900 XTX, LHM 0.9.6-era) is
  verified; remaining rows (legacy conhost, tmux/SSH, Intel iGPU, AMD-only/no-GPU
  machines, LHM 0.9.5, NO_COLOR, 24 h soak) are still open.
- **Writes remain limited to the fan anti-stop floor**; general fan/pump control
  is planned for v0.4.0.
- Some GPUs report `0 MHz` / `0.00 V` for short moments in deep idle
  (LHM/NVAPI behaviour); the dashboard renders the values it gets.

## Compatibility

- LibreHardwareMonitor 0.9.5+ (developed against 0.9.6-era builds; the parser
  also covers the newer numeric `Raw*` payloads).
- Windows 10/11, Linux and macOS clients; MSRV Rust 1.89.
- Checksums: `sha256.sum` in the release assets.

---

Full changelog for this release:
[CHANGELOG.md](https://github.com/baroseiro/lhm-tui/blob/v0.3.0/CHANGELOG.md#030---2026-10-19).
