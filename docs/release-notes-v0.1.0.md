# lhm-tui v0.1.0 — release notes (hand-written body)

<!--
Paste this into the GitHub Release that cargo-dist creates for `v0.1.0`
(the workflow generates a body from CHANGELOG.md; replace/prepend it with this
text). Keep the checklist items honest — tick the QA-matrix rows that were
actually executed and list what is still pending.
-->

**A btop-style terminal dashboard for LibreHardwareMonitor** — CPU, every GPU, memory and all
temperatures, live over LHM's web API. One static binary, no admin rights on the client, works on
localhost or across the network.

## Highlights

- **Panels**: CPU (temp/load gauges, per-core bars, clocks, power), one panel per GPU (multi-GPU,
  NVIDIA/AMD/Intel; VRAM, hot spot, memory-junction temperature, power, and every fan on the card),
  memory, temperatures table, plus fans/power/storage/network presets and fully custom panels.
- **Charts on demand** (`c` / `--charts`): braille/halfblock/ASCII graphs with min/max envelopes,
  autoscale hysteresis and window cycling. Off by default for an instant startup.
- **GPU fan anti-stop floor** (on by default, `f` toggles): holds zero-RPM-capable GPUs at a 45 %
  floor while idle and hands them back to the driver curve under load. Cards whose fan curve can
  never stop (most AMD models) are detected and left untouched.
- **Config**: one JSON file (or CLI flags) for layout, themes, thresholds, chart windows, sensor
  pins and key remaps — with a JSON Schema for editor completion and validation that reports every
  error at once.
- **Scriptable**: `--probe` (connectivity + sensor-binding report), `--once` (one frame to stdout),
  `--dump` (stable normalized JSON), fixed exit codes (0/1/2/3/4).
- **Remote monitoring**: LHM runs on Windows; the TUI runs anywhere — WSL2, SSH, Linux, macOS.

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

## First run

- No config needed: `lhm-tui` polls `http://127.0.0.1:8085/data.json` every second.
- `lhm-tui --probe` prints LHM version, sensor counts and the sensor-binding report — the first
  diagnostic for anything connection or parsing related.
- Config discovery, defaults and the full reference: `SPEC.md` §8 and `lhm-tui.example.json`
  (`lhm-tui --print-config` shows the merged config, `--init-config` writes a starting file).

## Known issues & deferrals

- **winget, scoop and crates.io** install channels are deferred to **v0.1.x** (PLAN T6.4/T6.5);
  this release ships via GitHub Releases and `cargo install --git`.
- **README screenshots/GIF** are a post-release docs task (needs an interactive capture session).
- **Manual QA matrix** (`docs/qa-matrix.md`): rows executed so far —
  <!-- fill in: Windows Terminal; 2-GPU AMD+NVIDIA reference box; fan-floor smoke; NO_COLOR; … -->
  remaining rows (WSL2, tmux/SSH, other GPU vendors, LHM 0.9.5, 24 h soak) are tracked for v0.1.x.
- **Writes are limited to the fan anti-stop floor** (GPU fan `Control` sensors only, `SetDefault()`
  when released). General fan/pump control with confirmation dialogs is planned for v0.2.
- Nodes with unusual sensor types are reported as skipped in the `d` debug overlay; please attach
  `--probe` output when reporting a binding mismatch.

## Compatibility

- LibreHardwareMonitor 0.9.5+ (developed and verified against 0.9.6-era builds); the web API
  tolerant parser also covers the newer numeric `Raw*` format.
- Windows 10/11 (Windows Terminal recommended), Linux and macOS clients; MSRV Rust 1.89.
- Checksums: `sha256.sum` in the release assets.
