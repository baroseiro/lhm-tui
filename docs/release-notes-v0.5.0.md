# lhm-tui v0.5.0 — release notes (hand-written body)

<!--
Release body source for the `v0.5.0` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**Direct DLL monitoring.** v0.5.0 adds a second data source: instead of polling
LibreHardwareMonitor's web server, `lhm-tui` can load `LibreHardwareMonitorLib.dll`
directly. The LibreHardwareMonitor application does not need to be running, and
all panels, charts, history, fan controls, `--probe`, `--once` and `--dump` work
exactly as before. HTTP remains the default.

## Highlights

- **New source selector** — `server.source = "http" | "dll"`, `--source`,
  `LHM_TUI_SOURCE`, plus `server.dll_path`, `--dll` and `LHM_TUI_DLL`. The DLL
  path may point at the DLL or its containing folder.
- **Both LHM distributions work** — the classic .NET Framework zip
  (`LibreHardwareMonitor.zip`) uses the embedded .NET Framework bridge; the
  .NET 8/9/10 zip (`LibreHardwareMonitor.NET.10.zip`) automatically falls back
  to the embedded net8.0 bridge with runtime roll-forward.
- **No LHM process or port** — the bridge loads the library, enables every
  hardware category, and emits the same JSON tree as `/data.json`. The existing
  tolerant parser and every binding stay unchanged.
- **Fan writes and reset** — `←`/`→`, `0`/Auto, the NVIDIA anti-stop guard and
  `m` min/max reset use the same `Control` sensors through the bridge, and
  quitting calls `Close()` so driver defaults return.
- **Diagnostics** — the header, debug overlay and `--probe` show the active
  source and resolved DLL path.
- **Safe integration** — the Rust crate keeps `#![forbid(unsafe_code)]`; the
  bridges are child processes, not in-process CLR/FFI hosting.

## Install

Same channels as v0.4.1:

- GitHub Releases archives and installers (Windows x64/arm64, Linux x64/arm64,
  macOS Intel/Apple silicon)
- `cargo install --git https://github.com/baroseiro/lhm-tui`

HTTP configs keep working unchanged. To use the DLL source:

```powershell
lhm-tui --source dll --dll "C:\Path\To\LibreHardwareMonitorLib.dll"
```

or set `"server": { "source": "dll", "dll_path": "..." }`.

## Known issues

- DLL mode is Windows-only; Linux/macOS continue to use HTTP for remote hosts.
- Run `lhm-tui` elevated for the same sensor coverage as LHM. Without
  elevation some CPU MSR/SuperIO sensors are absent or zero.
- The .NET 8/9/10 bridge needs a matching .NET runtime. Source builds include it
  only when the .NET 8+ SDK is available; release binaries embed both bridges.
- Recording and alerts, listed as v0.5 in the roadmap, move to v0.5.x so this
  release stays focused on the data source.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, `cargo +1.89 check --all-targets`, release build.
- Live `--probe`/`--dump`/`--once` with LHM closed against both the .NET
  Framework and .NET 10 LHM libraries.
- All six release archives, installers and checksums are built by the release
  workflow; the Windows x64 artifact is re-downloaded and smoke-tested.
