# lhm-tui v0.4.0 - release notes (hand-written body)

<!--
Release body source for the `v0.4.0` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**Fan and pump control, grouped tables and mouse support.** v0.4.0 turns the
dashboard into a control surface: the new Fans & Controls panel lists every fan
with its current speed and software setting, lets you change it from the
keyboard or mouse wheel, and keeps the anti-stop guard out of the way.

## Highlights

- **Fans & Controls panel** (`f`, hidden by default) - one flat table with the
  device in parentheses (`GPU Fan 1 (NVIDIA GeForce RTX 3090)`), showing the
  **current speed** and **current setting**. LHM's `Default` mode is shown as
  `Auto`; the two can differ, so both are always visible.
- **Control fans and pumps** - focus the panel with `Tab`, select with `↑`/`↓`
  or the mouse wheel, adjust by 5 % with `←`/`→` (or `-`/`+`), and press `0` to
  hand the fan back to Auto/Default. Writes go only to fan/pump `Control`
  sensors through LHM's `/Sensor?action=Set`.
- **Guard handoff** - a manual GPU percentage switches the anti-stop floor off
  for the session (`anti-stop off` on the status line) so the two writers never
  fight; `n` re-enables it. Returning a fan to Auto does not disable the guard.
- **Grouped temperatures and voltages** - both tables insert a device heading
  and sort within each device. GPU rails no longer repeat the card name.
- **Mouse support** - click a panel title to focus it, double-click to toggle
  zoom (same as `Enter`), and use the wheel exactly like `↑`/`↓` (scroll the
  focused table / move the fan selection).
- **CPU memory speed** - row 2 now ends with `Speed`, twice the `Clk Memory`
  sensor in MHz (`Speed 3600 MHz`).

## Install

1. Download the archive for your platform from the assets below
   (`lhm-tui-x86_64-pc-windows-msvc.zip`, `…-unknown-linux-musl.tar.xz`,
   `…-apple-darwin.tar.xz`, plus `aarch64` variants) and put `lhm-tui` on your
   `PATH`.
   Or use the generated installer: `lhm-tui-installer.ps1` (Windows) /
   `lhm-tui-installer.sh` (Linux/macOS).
2. Or build from source: `cargo install --git https://github.com/baroseiro/lhm-tui`
3. Requirements: LibreHardwareMonitor **0.9.5+** running as administrator
   (required for fan controls) with `Options → Remote Web Server → Run`
   enabled, plus a terminal ≥ 60×20 (80×24 recommended).
4. Verify downloads against `sha256.sum` (each archive also ships a `.sha256` sidecar).

## Upgrading from v0.3.0

No breaking config changes, but the default keys moved: **`f` now toggles the
Fans & Controls panel** and the **GPU fan anti-stop floor is on `n`**. New
remappable actions are `toggle_fans`, `fan_control_up`, `fan_control_down` and
`fan_control_auto`. `panels.fans` is enabled by default but stays hidden until
`f` (or adding `"fans"` to `ui.layout`). If you had customised the old
`toggle_fan_guard: ["f"]`, update it to avoid the new default conflict.
Mouse reporting is now enabled; use `Shift`+drag if your terminal needs native
text selection.

## Known issues & deferrals

- **winget, scoop and crates.io** install channels remain deferred to **v0.4.x**
  (PLAN T6.4/T6.5).
- Manual fan writes have **no confirmation modal, safe-range config or
  `--read-only` lock yet** (SPEC §16); LHM must run with the privileges needed
  to control the device. Some SuperIO headers expose a control without write
  support; failures appear on the status line.
- **Manual GPU fan percentages disable the anti-stop floor** for the session so
  the guard cannot override them; `n` re-enables it.
- **Estimated NVIDIA shared total**: LHM does not report a shared-memory total
  for NVIDIA cards, so it is derived from RAM minus VRAM. Treat that percentage
  as an estimate; AMD cards use the total reported by LHM.
- **Manual QA matrix** (`docs/qa-matrix.md`): the reference rig is verified;
  remaining rows (legacy conhost, tmux/SSH, Intel iGPU, AMD-only/no-GPU
  machines, LHM 0.9.5, NO_COLOR, 24 h soak) are still open.
- Some GPUs report `0 MHz` / `0.00 V` for short moments in deep idle
  (LHM/NVAPI behaviour); the dashboard renders the values it gets.

## Compatibility

- LibreHardwareMonitor 0.9.5+ (developed against 0.9.6-era builds; the parser
  also covers the newer numeric `Raw*` payloads).
- Windows 10/11, Linux and macOS clients; MSRV Rust 1.89.
- Checksums: `sha256.sum` in the release assets.

---

Full changelog for this release:
[CHANGELOG.md](https://github.com/baroseiro/lhm-tui/blob/v0.4.0/CHANGELOG.md#040---2026-10-19).
