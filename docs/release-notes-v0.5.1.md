# lhm-tui v0.5.1 — release notes (hand-written body)

<!--
Release body source for the `v0.5.1` GitHub Release (the workflow creates the
release with the CHANGELOG section; `gh release edit --notes-file` sets this
text). Keep the "known issues" list honest.
-->

**CPU temperatures in DLL mode.** v0.5.1 fixes the case where DLL mode showed
`0.0 °C` for CPU temperature (and 0 W / 0 MHz for package power and clocks).
Windows blocks the PawnIO/MSR driver access those sensors need without
administrator rights. The bridge now requests elevation through the normal
Windows UAC prompt and proxies hardware access to an elevated child over a
local named pipe, so the TUI itself stays non-elevated and remains in the same
terminal.

## Highlights

- **Automatic UAC elevation** — start DLL mode from a normal terminal and
  approve the prompt once; CPU temperature, package power and clocks work.
  Already-elevated sessions skip the prompt.
- **Clear limited mode** — `--probe` prints the elevation state and a warning;
  the TUI shows a one-time status-line message; `--once`/`--dump` write the
  warning to stderr; the debug overlay reports `dll elevation`.
- **Opt out** — set `LHM_TUI_NO_ELEVATE=1` to skip the prompt (GPU monitoring
  still works; CPU MSR sensors may read 0).
- **No double prompts** — after an elevation error the client does not fall
  back to the other bridge flavor, so a declined UAC prompt is reported once.
- **CI fix** — non-Windows `clippy -D warnings` passes again.

## Install

Same channels as v0.5.0: GitHub Releases archives/installers and
`cargo install --git https://github.com/baroseiro/lhm-tui`.

Existing configs need no changes. If you previously launched the TUI from an
elevated terminal, behavior is unchanged. If you prefer no UAC prompt, set
`LHM_TUI_NO_ELEVATE=1` and use HTTP mode for full CPU sensor coverage.

## Known issues

- DLL mode is Windows-only.
- UAC must be available; locked-down systems that block elevation can use
  `LHM_TUI_NO_ELEVATE=1` for GPU-only data or run the TUI elevated.
- If UAC is approved with a different administrator account, the named-pipe
  connection may be denied; use the same account or run the TUI elevated.
- Recording and alerts remain scheduled for v0.5.x.

## Verification

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, `cargo +1.89 check --all-targets`, release build.
- Manual non-elevated proxy test (named pipe) and `--probe` warnings with both
  the .NET Framework and .NET 10 LHM libraries; UAC approval uses the standard
  Windows `runas` path.
- All six release archives, installers and checksums are built by the release
  workflow; the Windows x64 artifact is re-downloaded and smoke-tested.
