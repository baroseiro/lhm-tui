# lhm-tui v0.2.0 — release notes (hand-written body)

<!--
Release body source for the `v0.2.0` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**Voltage and clock indicators.** The dashboard now shows what the whole machine is actually
doing electrically: a voltages table, per-GPU core voltage, and the full set of CPU clocks, core
voltage and package current.

## Highlights

- **Voltages table** — press `v` (or add `"voltage"` to `ui.layout`) for every voltage sensor in
  the tree, sorted by value with LHM min/max. Hidden by default, like the temperatures table.
- **GPU metrics row** — `Power · Clock · Voltage` on every GPU panel, with all fans still
  right-aligned on the same row (`Power 26 W · Clock 210 MHz · Voltage 0.72 V · Fan 1 610 RPM ·
  Fan 2 664 RPM`); the fan block drops to its own row when the panel is too narrow.
- **CPU metrics strip** — replaces the old clock/power line with
  `Power · Clock Average · Clock Effective · Bus Speed · Clock Fabric · Clock Uncore · Clock Memory
  · Voltage Core · Current TDC`, wrapping onto multiple rows when the panel is narrow.
- **No more truncated labels** — info rows keep full labels (`Clock Effective`, `Voltage Core`),
  and min/max columns format voltages (two decimals) and currents (one) properly.
- Also in this release since v0.1.0: the repository is **public**, README now ships a **hero shot +
  15 s demo GIF** (recorded with VHS, reproducible via `docs/demo.tape`), and CI moved to
  `actions/checkout@v6` with pinned runner images.

## Install

1. Download the archive for your platform from the assets below
   (`lhm-tui-x86_64-pc-windows-msvc.zip`, `…-unknown-linux-musl.tar.xz`, `…-apple-darwin.tar.xz`,
   plus `aarch64` variants) and put `lhm-tui` on your `PATH`.
   Or use the generated installer: `lhm-tui-installer.ps1` (Windows) / `lhm-tui-installer.sh`
   (Linux/macOS).
2. Or build from source: `cargo install --git https://github.com/baroseiro/lhm-tui`
3. Requirements: LibreHardwareMonitor **0.9.5+** running as administrator with
   `Options → Remote Web Server → Run` enabled, plus a terminal ≥ 60×20 (80×24 recommended).
4. Verify downloads against `sha256.sum` (each archive also ships a `.sha256` sidecar).

## Upgrading from v0.1.0

No breaking changes. New defaults only: the CPU panel shows the richer metrics strip (toggle rows
with `panels.cpu.show`), GPU panels gained `voltage` in `panels.gpu.show`, and `v` toggles the new
table. `fan_guard` and chart defaults are unchanged.

## Known issues & deferrals

- **winget, scoop and crates.io** install channels remain deferred to **v0.2.x** (PLAN T6.4/T6.5).
- **Manual QA matrix** (`docs/qa-matrix.md`): the reference rig (Windows 11 + Windows Terminal,
  Ryzen 9 5900XT, RTX 3090 + RX 7900 XTX, LHM 0.9.6-era) is verified, including the anti-stop floor
  and the new voltage/clock bindings; remaining rows (legacy conhost, tmux/SSH, Intel iGPU,
  AMD-only/no-GPU machines, LHM 0.9.5, NO_COLOR, 24 h soak) are still open.
- **Writes remain limited to the fan anti-stop floor**; general fan/pump control is planned for
  v0.3.0.
- Some GPUs report `0 MHz` / `0.00 V` for short moments in deep idle (LHM/NVAPI behaviour); the
  dashboard renders the values it gets.

## Compatibility

- LibreHardwareMonitor 0.9.5+ (developed against 0.9.6-era builds; the parser also covers the newer
  numeric `Raw*` payloads).
- Windows 10/11, Linux and macOS clients; MSRV Rust 1.89.
- Checksums: `sha256.sum` in the release assets.

---

Full changelog for this release:
[CHANGELOG.md](https://github.com/baroseiro/lhm-tui/blob/v0.2.0/CHANGELOG.md#020---2026-10-19).
