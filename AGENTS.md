# AGENTS.md

## What this repo is

- **Docs-only, pre-implementation.** The repo holds only markdown docs — `README.md` (public),
  `SPEC.md` (frozen design, v1.0), `PLAN.md` (execution plan), `AGENTS.md`, `CHANGELOG.md`. Verify
  with a directory listing / `git ls-files` before assuming code exists. No crate, CI, schema or
  example config yet.
- **Git history is live and intentional:** one `initial commit` on `main`, tracking
  `origin` = `https://github.com/baroseiro/lhm-tui.git`. Commit forward; don't re-init, rewrite or
  orphan that history. PR/commit expectations are in PLAN §0 (one PR per task, CHANGELOG updates).
- **Reading order before any work:** `SPEC.md` in full, then `PLAN.md`. Plan phases P0–P6 map 1:1
  to SPEC §11 milestones M0–M6; tasks (`T<phase>.<n>`) are in dependency order, and `SPEC.md` /
  `PLAN.md` must stay consistent — update both together.
- **Roadmap files**: the P0–P6 scaffolding is in place (LICENSE, `lhm-tui.example.json`,
  `schema/`, `docs/`, `.github/`). UI captures (`docs/preview.png`, `docs/demo.gif`) are generated
  from `docs/demo.tape` with VHS — regenerate them when the layout changes noticeably; never "fix"
  a doc by deleting references to files that exist.

## Sources of truth & edit order

Precedence when things conflict: **executable config/scripts > `SPEC.md` > `PLAN.md` > `README.md`.**

1. Design/protocol change → edit `SPEC.md` first, then `PLAN.md` (adjust affected tasks), then code/README.
2. Any change to the public surface (CLI flag, config field, keybinding, exit code) requires updating
   `SPEC.md` §8.2+§8.3, `lhm-tui.example.json`, `schema/lhm-tui.schema.json` and `README.md` in the
   same change (PLAN §1 "Docs" rule).
3. `SPEC.md` §8.2's annotated config is **JSONC for documentation only**. The actual
   `lhm-tui.example.json` must be strict JSON — no comments, no trailing commas (SPEC §8.1).
   Don't copy the comments, and don't "clean up" the SPEC example.
4. `OWNER` placeholders were replaced with **`baroseiro`** in `README.md` and in the schema `$id`
   (T6.1); the README screenshot/animation placeholders remain (comments mark what to add post-tag).
   Don't reintroduce `OWNER`.
5. ASCII mockups in `SPEC.md` §6.1 / `README.md` are illustrative. TestBackend snapshots (T5.6)
   are the rendering source of truth.

## Change tracking — `CHANGELOG.md` is mandatory

- **Every semantic change adds a bullet under `## [Unreleased]` in `CHANGELOG.md`, in the same
  commit/PR as the change** — docs edits (SPEC/PLAN/README/AGENTS) included; only cosmetic edits
  (whitespace, typo-only) are exempt.
- Format is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/): categories `Added`, `Changed`,
  `Deprecated`, `Removed`, `Fixed`, `Security` — create the subsection in that order when missing.
  One bullet per logical change, worded for users; append the PLAN task id when the change
  implements one (e.g. "Scaffold crate + CI (T0.1)").
- **Never edit a released version's section.** Releases move `[Unreleased]` entries into a new
  version section (T6.2/T6.6); new work always lands in `[Unreleased]`.
- A missing changelog entry is a review blocker — treat it like a failing test.

## Constraints agents default to violating

- **LHM parsing leniency is intentional — do not "fix" it.** `Raw*` fields may be JSON numbers
  *or* unit-suffixed strings (changed after LHM 0.9.6), and bare `NaN`/`Infinity` break strict
  parsers: see SPEC §3.4 quirks Q1–Q4 and keep the sanitizer + dual deserializer as specified.
- **Never parse node `id` as identity** — it is request-scoped; use `SensorId`/`HardwareId` (Q3).
- Hard rules from PLAN §1: `#![forbid(unsafe_code)]`, MSRV 1.89 / edition 2021, no `unwrap()`
  outside tests, config validation collects all errors and exits 2 — never panics.
- **Stdout belongs to the TUI.** Only `--probe`, `--once`, `--dump`, `--print-config`,
  `--init-config` may write to stdout; all logging goes to a file (SPEC §10.2). Exit codes are
  fixed: 0 ok · 1 runtime · 2 config · 3 connect · 4 auth (SPEC §9).
- The UI task must never await HTTP; polling, backoff and `watch` channel behavior is specified in
  SPEC §4.2. Don't restructure the threading model ad hoc.
- The LHM host is Windows-only, but the TUI itself must build and run on Linux/macOS (remote
  monitoring is a feature, not an accident) — don't add Windows-only code paths outside detection.
- **External TUI references are inspiration only** (nvtop for multi-GPU charts — SPEC §7.4).
  nvtop is GPL-3.0-or-later: never copy its code, assets or theme files. btop is Apache-2.0
  (attribution required if ever reused). Verify a project's license before using anything from it.

## Testing & CI expectations

- Fixture corpus `tests/fixtures/*.json` is the parsing contract (version matrix: LHM 0.9.5 /
  0.9.6 formatted `Raw*` / master numeric `Raw*` / NaN / 2-GPU / huge). New heuristic ⇒ new fixture;
  new widget ⇒ `TestBackend` + `insta` snapshot (SPEC §12).
- CI never talks to a real LHM: integration tests use a mock server (SPEC §12.2). `--dump -` output
  has a golden test — treat it as public API.
- No CI exists yet; it lands first in T0.1 (matrix: windows + ubuntu + macos, running
  `fmt --check`, `clippy -D warnings`, `test`, `build --release`). Manual QA matrix and soak tests
  (SPEC §12.4, PLAN T5.7) are release gates, not CI jobs.
- Per-PR verification order is PLAN §5: `fmt → clippy → test → targeted test files → manual smoke`.

## Commands

- Nothing is runnable yet (no `Cargo.toml`). After T0.1 scaffolds the crate: `cargo run` (against a
  real LHM at `http://localhost:8085`), `cargo run -- --probe`, `cargo test`,
  `cargo clippy --all-targets -- -D warnings`.
- Prefer `--probe` as the first diagnostic for anything connection/parsing related; it prints LHM
  version, sensor counts and the sensor-binding report (SPEC §9).
