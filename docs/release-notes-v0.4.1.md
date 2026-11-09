# lhm-tui v0.4.1 - release notes (hand-written body)

<!--
Release body source for the `v0.4.1` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**CPU and PCH fan readouts plus a safer fan guard.** v0.4.1 adds the SuperIO/LPC
fan speeds to the CPU metrics strip and makes the anti-stop floor opt-in and
NVIDIA-only, so a default install never writes to a GPU fan controller.

## Highlights

- **CPU Fan / PCH Fan** - the CPU strip row 1 now shows `CPU Fan` and `PCH Fan`
  after `Volt Core`. Both bind from LPC/SuperIO hardware because they are not
  attached to the CPU node; missing fans are skipped. New override slots
  `cpu.fan_cpu` / `cpu.fan_pch` and a `fan` slot in `panels.cpu.show`.
- **Fan guard is opt-in** - `fan_guard.enabled` defaults to `false`. Enable it
  with `n` or `"fan_guard": { "enabled": true }` when an NVIDIA zero-RPM curve
  parks the fans while idle.
- **NVIDIA-only guard** - AMD/Intel fan controls are never written to, even when
  the guard is enabled, so swapping cards or PCIe slots cannot arm the floor on
  an AMD card.
- **Docs/schema fixes** - the JSON Schema now reflects the actual
  `fan_guard.enabled` default, and the Windows config directory is documented as
  `%APPDATA%\lhm-tui\config\`.

## Install

Use the same install channels as v0.4.0:

- GitHub Releases archives and installers (Windows x64/arm64, Linux x64/arm64,
  macOS Intel/Apple silicon)
- `cargo install --git https://github.com/baroseiro/lhm-tui`

Existing configs keep working; no migration is required. If you previously
relied on the floor being armed by default, set `"fan_guard": { "enabled": true }`
or press `n`.

## Known issues

- The guard is NVIDIA-only by design; AMD/Intel cards are never touched.
- Windows on ARM and Linux ARM builds have no separate release archive beyond
  the existing v0.4.0 matrix.
- Recording/alerting remains planned for v0.5.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `cargo +1.89 check --all-targets`, release build.
- Live probe against the reference LHM machine; captures regenerated with the
  CPU/PCH fan readouts visible.
- All six release archives are built and smoke-tested by the release workflow.
