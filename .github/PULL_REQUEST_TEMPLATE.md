## What & why

<!-- One or two sentences. Link the PLAN task (T4.2) when the PR implements one. -->

## Checklist

- [ ] `cargo fmt` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] `cargo test` passes (snapshots reviewed via `cargo insta review` if rendering changed)
- [ ] `CHANGELOG.md` has a bullet under `[Unreleased]` (mandatory for semantic changes)
- [ ] Public surface (CLI flag, config field, keybinding, exit code, `--dump`) updates SPEC §8.2/§8.3,
      `lhm-tui.example.json`, `schema/lhm-tui.schema.json` and `README.md` in this PR
- [ ] New parsing heuristic ⇒ new fixture in `tests/fixtures/`; new widget ⇒ `TestBackend` snapshot
- [ ] No `unsafe`, no `unwrap()` outside tests, no copied GPL code/assets (SPEC §7.4)

## Testing

<!-- How did you verify? Mock-server tests, snapshots, `--probe` against real LHM, manual smoke… -->
