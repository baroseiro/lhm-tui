<div align="center">

# lhm-tui

**A btop-style terminal dashboard for LibreHardwareMonitor — CPU, GPU, temps, load and RAM, live in your terminal.**

[![CI](https://github.com/OWNER/lhm-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/OWNER/lhm-tui/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/OWNER/lhm-tui?include_prereleases&sort=semver)](https://github.com/OWNER/lhm-tui/releases/latest)
[![crates.io](https://img.shields.io/crates/v/lhm-tui.svg)](https://crates.io/crates/lhm-tui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-windows%20%7C%20linux%20%7C%20macos-informational)

</div>

<!-- TODO before publishing: replace OWNER in every URL above with your GitHub user/org, then delete this comment and the two screenshot comments below. -->

<!-- TODO: add docs/preview.png once M2 renders presets. Suggested size: 1600×900 PNG of a real session. -->
<!-- TODO: add docs/demo.gif once charts land (M3). Record ~15s with ScreenToGif or terminalizer. -->

`lhm-tui` talks to LibreHardwareMonitor's built-in **Remote Web Server** over plain HTTP and renders
live, color-thresholded panels in your terminal:

- **CPU** — temperature & total load graphs, per-core utilization bars, clocks, package power
- **GPU** — one panel per GPU (NVIDIA/AMD/Intel, multi-GPU supported): core load, **VRAM usage**,
  hotspot temperature, power draw and fan speed
- **Memory** — used/available gauges with history, page file and virtual memory
- **Temperatures** — sortable table of every thermal sensor in the tree, colored by thresholds
- Plus presets for **fans**, **power**, **storage** and **network**

It's a single static binary, needs **no admin rights**, and can monitor `localhost` — or any
Windows machine on your network / over SSH — through a small JSON config file.

> **Project status:** v0.1.0 in development. The full design — architecture, config schema, chart
> engine, milestones — lives in [SPEC.md](SPEC.md).

---

## Requirements

| Component     | Requirement                                                                                   |
| ------------- | ---------------------------------------------------------------------------------------------- |
| Data source   | [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) **0.9.5+** running as administrator, with **Options → Remote Web Server → Run** enabled |
| TUI client    | Windows 10/11, Linux or macOS. Windows Terminal recommended on Windows                          |
| Terminal size | ≥ 80×24 recommended (minimum 60×20)                                                              |

## Quick start

1. **Enable the LHM web server** — in LibreHardwareMonitor:
   `Options → Remote Web Server → Run` ✓ (leave the port at `8085`).
   Optionally make sure the sensors you want are ticked under `File → Hardware`.
2. **Install `lhm-tui`** (pick one):

   ```powershell
   # Windows — winget
   winget install OWNER.lhm-tui

   # Windows — scoop
   scoop install lhm-tui

   # Any platform — from source / crates.io
   cargo install lhm-tui
   ```
3. **Run it:**

   ```console
   $ lhm-tui
   ```

That's it — with no config file, `lhm-tui` polls `http://127.0.0.1:8085/data.json` every second.

> Monitoring a different machine? `lhm-tui --url http://192.168.1.42:8085` or set it once in the
> config file (see below).

## Usage

```console
lhm-tui --help

  -c, --config <PATH>     Config file (JSON)
      --url <URL>         LibreHardwareMonitor base URL   [env: LHM_TUI_URL]
      --username <USER>   Basic auth user                 [env: LHM_TUI_USERNAME]
      --password <PASS>   Basic auth password             [env: LHM_TUI_PASSWORD]
      --poll <DURATION>   Poll interval, e.g. 1s, 500ms
      --window <DURATION> Initial chart window, e.g. 5m
      --theme <NAME|PATH> Built-in theme or theme file
      --ascii             ASCII-only rendering
      --no-color          Disable colors (also honors NO_COLOR)
      --probe             Check connectivity, print LHM version & sensor counts, exit
      --once              Render one snapshot to stdout and exit
      --dump <PATH|->     Dump the normalized snapshot as JSON, exit
      --init-config       Write a default config file, exit
      --print-config      Print the effective merged config, exit
```

`--probe` is handy as a health check; `--once` / `--dump` make it scriptable.

### Keybindings

| Keys                  | Action                                  |
| --------------------- | --------------------------------------- |
| `q` / `Esc` / `Ctrl+C`| Quit                                    |
| `?`                   | Help overlay                            |
| `p` / `Space`         | Pause / resume polling                  |
| `g` / `G`             | Cycle chart window (1m · 5m · 15m)      |
| `Tab` / `Shift+Tab`   | Focus next / previous panel             |
| `Enter`               | Zoom focused panel fullscreen           |
| `b`                   | Sensor browser                          |
| `s`                   | Change table sort                       |
| `r`                   | Reload config from disk                 |
| `m`                   | Reset LHM min/max counters              |
| `a` / `d`             | ASCII mode / debug overlay              |
| `t` / `c`             | Toggle temperatures table / charts      |

All keys are remappable in the config file.

## Configuration

`lhm-tui` looks for `lhm-tui.json` in the working directory, then in your platform config dir
(`%APPDATA%\lhm-tui\` on Windows, `~/.config/lhm-tui/` elsewhere). Generate a starting point with
`lhm-tui --init-config`. A fully commented example ships as
[`lhm-tui.example.json`](lhm-tui.example.json); the complete reference is in
[SPEC.md §8](SPEC.md#8-configuration).

```json
{
  "$schema": "./schema/lhm-tui.schema.json",
  "server": {
    "url": "http://127.0.0.1:8085",
    "poll_interval_ms": 1000
  },
  "ui": {
    "layout": ["cpu", "gpu", "memory", "thermal"]
  },
  "charts": {
    "window": "5m",
    "window_cycle": ["1m", "5m", "15m"]
  },
  "theme": {
    "cpu": "#7aa2f7",
    "gpu": "#bb9af7",
    "memory": "#9ece6a",
    "warn": "#e0af68",
    "crit": "#f7768e",
    "thresholds": {
      "temperature": { "warn": 80, "crit": 95 }
    }
  }
}
```

Common things people change:

| I want to…                          | Config                                                                 |
| ----------------------------------- | ---------------------------------------------------------------------- |
| Monitor another machine             | `server.url` or `--url`                                                |
| Use the web server's login          | `server.username` + `server.password_env` (read from an env var)       |
| Poll faster / slower                | `server.poll_interval_ms` (250–60000)                                  |
| Reorder / hide panels               | `ui.layout`, `panels.<name>.enabled`                                   |
| Pin a chart to a specific sensor    | `panels.sensor_overrides` with a `SensorId` (e.g. `/amdcpu/0/temperature/2`) |
| Build a custom panel                | `panels.custom` with `graph` / `gauge` / `table` widgets                |
| Change colors or warning thresholds | `theme`                                                                 |
| Remap keys                          | `keys`                                                                  |
| Turn charts off                     | `charts.enabled: false` (or run with `--no-charts`, or press `c`)       |
| Show the temperatures table         | Press `t`, or add `"thermal"` to `ui.layout`                            |
| Switch theme                        | `theme.name`: `default`, `gruvbox`, `nord` or `mono`                    |
| Custom theme file                   | `lhm-tui --theme path/to/theme.json` (same shape as the `theme` block)  |
| Strip all colors                    | `NO_COLOR=1 lhm-tui` or `--no-color`                                    |

CLI flags beat environment variables, which beat the config file.

## Remote monitoring & WSL2

LHM's web server binds where you tell it to. To watch a PC from another machine:

1. In LHM: `Options → Settings → Remote Web Server` → set the bind address to all interfaces
   (or the LAN IP) and **restart the server** when prompted.
2. Allow inbound TCP `8085` in Windows Defender Firewall.
3. Point `lhm-tui` at it: `lhm-tui --url http://192.168.1.42:8085`.

**WSL2 users:** `localhost` only reaches Windows if WSL is in
[mirrored networking mode](https://learn.microsoft.com/windows/wsl/networking#mirrored-mode-networking);
otherwise use the gateway IP (`ip route | grep default`) or the LAN IP.

## Troubleshooting

| Symptom                                  | Fix                                                                                       |
| ---------------------------------------- | ----------------------------------------------------------------------------------------- |
| `Cannot reach LHM at http://…:8085`      | Is LHM running **as administrator**? Is `Options → Remote Web Server → Run` checked? Restart the server after changing port/bind/auth. |
| `Authentication failed`                  | Credentials must match the username/password set in the LHM web server settings           |
| Panels but values look stale / missing   | Tick the hardware under `File → Hardware` in LHM; some sensors need admin rights           |
| Nothing but `—` on an NVIDIA laptop GPU   | Some mobile dGPUs only expose sensors while the GPU is active                              |
| Braille graphs look broken                | Press `a` for ASCII mode or set `charts.marker` to `halfblock` in the config               |
| Wrong sensor bound to a chart             | `lhm-tui --probe` prints the binding report; pin the sensor via `panels.sensor_overrides`  |

Debug logging: `lhm-tui --log lhm-tui.log -v -v` (never writes into the TUI itself).

## Roadmap

- **v0.1.0** — read-only dashboard: presets, live braille charts, JSON config, themes *(current)*
- **v0.2.0** — fan/pump control from the TUI (`Sensor?action=Set`) with confirmation dialogs
- **v0.3.0** — CSV/JSONL recording, threshold alerts, chart history backfill via LHM's `/metrics`
- **v0.4.0** — multi-host tabs, user theme files
- Later — template-based custom panels, plugin scripting

See [SPEC.md](SPEC.md) for the full design and milestone acceptance criteria.

## Building from source

```console
git clone https://github.com/OWNER/lhm-tui
cd lhm-tui
cargo build --release        # binary in target/release/lhm-tui
cargo test                   # unit + integration + TUI snapshot tests
```

MSRV: Rust **1.89** (2021 edition). Cross-compiles cleanly to Linux/macOS for remote monitoring.

## Contributing

Issues and PRs are welcome! Please:

- Open an issue first for anything larger than a bug fix — the design decisions live in SPEC.md.
- Run `cargo fmt` and `cargo clippy -- -D warnings` before pushing; CI runs both plus tests on
  Windows, Linux and macOS.
- Include the output of `lhm-tui --probe` (redact IPs/usernames) and your LHM version for bug reports.

## License

[MIT](LICENSE) © OWNER

*Not affiliated with the LibreHardwareMonitor project. "LibreHardwareMonitor" is the work of its
respective authors; `lhm-tui` merely consumes its public HTTP API.*
