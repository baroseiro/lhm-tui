#!/usr/bin/env bash
# lhm-tui soak test: run the TUI for N minutes, sampling RSS/CPU every minute.
# Usage:  ./scripts/soak.sh [minutes] [url]
set -u

minutes="${1:-60}"
url="${2:-http://127.0.0.1:8085}"
csv="soak.csv"
binary="$(dirname "$0")/../target/release/lhm-tui"

if [ ! -x "$binary" ]; then
    echo "build first: cargo build --release ($binary not found)" >&2
    exit 1
fi

echo "minute,rss_kb,cpu_s" > "$csv"
"$binary" --url "$url" --log "${TMPDIR:-/tmp}/lhm-tui-soak.log" &
pid=$!
echo "soaking for $minutes minute(s); pid $pid"

for minute in $(seq 1 "$minutes"); do
    sleep 60
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "process exited early"
        break
    fi
    rss=$(ps -o rss= -p "$pid" | tr -d ' ')
    cpu=$(ps -o time= -p "$pid" | tr -d ' ')
    echo "$minute,$rss,$cpu" >> "$csv"
    echo "minute $minute: rss ${rss} KB, cpu ${cpu}"
done

kill "$pid" 2>/dev/null
echo "sample log: $csv (watch for flat RSS and slow-growing cpu time)"
