# Manual QA matrix

Executed before every release (PLAN T5.7, SPEC §12.4). Tick a row only after
running the checklist at the bottom on that combination; file issues for
failures and list deferrals in the release notes.

| # | Environment | LHM | Result | Notes / issue |
| - | ----------- | --- | ------ | ------------- |
| 1 | Windows 11 + Windows Terminal (truecolor) | 0.9.6, no auth | ☐ | |
| 2 | Windows 11 + legacy conhost | 0.9.6, no auth | ☐ | `a` ASCII fallback |
| 3 | WSL2 (mirrored or gateway IP) + Windows Terminal | 0.9.6, no auth | ☐ | remote monitoring |
| 4 | tmux over SSH (256-color) | 0.9.6, auth on | ☐ | Basic auth path |
| 5 | kitty / wezterm (Linux, truecolor) | 0.9.6, no auth | ☐ | |
| 6 | Terminal < 60×20 | — | ☐ | too-small screen |
| 7 | AMD CPU + NVIDIA GPU | 0.9.6 | ☐ | reference rig |
| 8 | Intel CPU + iGPU | 0.9.6 | ☐ | |
| 9 | AMD GPU only | 0.9.6 | ☐ | |
| 10 | Two GPUs, mixed vendor | 0.9.6 | ☐ | side-by-side layout |
| 11 | No GPU (VM) | 0.9.6 | ☐ | graceful presets |
| 12 | LHM 0.9.5 (old tuple) | 0.9.5 | ☐ | version warning |
| 13 | `NO_COLOR=1` / `--no-color` | — | ☐ | plain rendering |
| 14 | Fan anti-stop floor, NVIDIA at idle | 0.9.6, LHM as admin | ☐ | §7.5: `f` spins stopped fans; load spike → auto; quit → auto |

## Checklist per row

1. `lhm-tui --probe` prints version, counts and no errors.
2. `lhm-tui` renders CPU, GPUs, memory; values match the LHM web UI within
   one poll interval.
3. Press `c` (or run with `--charts`): charts fill over ~5 minutes; `g` cycles the window; `c`
   switches back to gauges.
4. `t` shows/hides the temperatures table; `s` sorts it; `j`/`k` scroll it.
5. `enter` zooms; `tab` cycles focus; `b` searches sensors; Enter graphs a
   sensor; `q`/`esc` close overlays without quitting.
6. `p` pauses (header shows `paused`); `r` reloads an edited config without
   losing history; `m` reports the min/max reset result on the status line.
7. Kill/restart LHM mid-session: status goes `stale` then `live` again.
8. Quit restores the terminal (no raw mode, cursor visible, shell usable).
9. `--once` and `--dump -` produce output and exit 0.
10. Fan anti-stop floor (row 14 only): it is on by default — with the GPU idle the fans spin at
    the floor and LHM's UI shows a manual control value; start a GPU load → the guard releases and
    the driver curve takes over; stop the load → floor returns; `f` off / quit → LHM shows
    automatic control again.
