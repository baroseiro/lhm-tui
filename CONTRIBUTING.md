# Contributing

Thanks for looking at `lhm-tui`. This project is intentionally small: one binary, one
dependency-light crate, a frozen design. The rules below keep it that way.

## Ground rules

- **[SPEC.md](SPEC.md) is the design source of truth.** A change that alters behaviour, a public
  surface (CLI flag, config field, keybinding, exit code, `--dump` format) or the architecture
  starts as a SPEC edit — then [PLAN.md](PLAN.md), then code.
- **Everything public ships documented in the same PR.** Public-surface changes update
  `SPEC.md` (§8.2/§8.3 and friends), `lhm-tui.example.json`, `schema/lhm-tui.schema.json` and
  `README.md` together. The example config is generated from the defaults:
  `cargo run -- --print-config`, then put the `"$schema"` line back.
- **`CHANGELOG.md` is mandatory.** Every semantic change adds a bullet under `## [Unreleased]` in
  the same commit ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/) categories, one bullet
  per logical change, PLAN task id appended when one applies). A missing entry is a review blocker.
- **No GPL code or assets.** Some projects (`nvtop`, GPL-3.0-or-later) are design inspiration only;
  never copy code, themes or assets from them. See SPEC §7.4.
- **No `unsafe`** (`#![forbid(unsafe_code)]`), no `unwrap()` outside tests, no panics on bad
  input — config validation collects *all* errors and exits with code 2.

## Development loop

Prerequisites: [Rust](https://rustup.rs) — `rust-toolchain.toml` pins the MSRV (**1.89**); an
optional local [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)
with `Options → Remote Web Server → Run` for live checks.

```console
cargo fmt                                        # format
cargo clippy --all-targets -- -D warnings        # lint (CI fails on any warning)
cargo test                                       # unit + integration + TUI snapshots
cargo run -- --probe                             # connectivity/sensor-binding diagnosis
cargo run                                        # the TUI (needs LHM on localhost:8085)
```

Verification order per PR (PLAN §5): `fmt` → `clippy` → `test` → the targeted test files for what
you touched → manual smoke (`--probe`, then the TUI).

### Tests you will most likely touch

| Area | Where |
| ---- | ----- |
| Parsing / LHM payload quirks | `tests/parse.rs`, `tests/fixtures/*.json` (new heuristic ⇒ new fixture) |
| Sensor bindings / presets | `tests/bindings.rs`, `tests/parse.rs` |
| Config validation & schema | `src/config.rs` tests, `tests/schema.rs` |
| Rendering (snapshots) | `tests/frames.rs` / `tests/widgets.rs` + `insta` |
| HTTP behaviour | `tests/mock_server.rs` (scripted mock — CI never talks to a real LHM) |

Snapshot tests use [`insta`](https://insta.rs). When the rendering change is intended, review and
accept the diff with `cargo insta review` (or `INSTA_UPDATE=always cargo test`) — never hand-edit
`.snap` files without reading the diff.

### Live checks (optional, local only)

```console
cargo test --test live -- --ignored --nocapture    # needs LHM on localhost:8085
```

The ignored live tests are read-only except `live_set_control_rejects_unknown_sensor`, which only
probes a **non-existent** sensor id (no hardware writes). The real fan anti-stop smoke test is a
manual checklist item in [docs/qa-matrix.md](docs/qa-matrix.md).

### Docs captures (screenshots/GIF)

The README hero shot and demo GIF are recorded from a real session with
[VHS](https://github.com/charmbracelet/vhs):

```console
vhs docs/demo.tape        # writes docs/preview.png + docs/demo.gif
```

Needs `vhs`, `ttyd` and `ffmpeg` on `PATH`, a running LHM on `localhost:8085`, and `lhm-tui` on
`PATH`. The tape briefly floors an idle zero-RPM GPU at 45 % — that is the anti-stop feature doing
its job; it is released again when the demo quits. Refresh both files when the layout changes
noticeably (the TestBackend snapshots in `tests/` remain the rendering source of truth).

## Pull requests

1. Small, focused commits with conventional-ish prefixes (`feat:`, `fix:`, `change:`, `docs:`).
2. Include the changelog bullet and the docs/schema/example updates for anything public.
3. Describe *why* in the PR body; link the PLAN task (`T4.2`) when the change implements one.
4. Fix what CI reports: `fmt --check`, `clippy -D warnings`, `cargo test --all-targets`,
   release build — on Windows, Linux and macOS.

Bug reports are most useful with `lhm-tui --probe` output (redact IPs/usernames), the LHM version
and your terminal/OS; the issue template asks for exactly that.

## License

By contributing you agree that your contributions are licensed under the [MIT license](LICENSE).
