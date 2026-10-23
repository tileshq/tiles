#!/usr/bin/env bash
# macOS only: opens a kitty window running `tiles run --terminal <terminal>
# codex`, where `codex` is a fake harness that prints a placeholder block for a
# card this script makes, and keeps a screenshot of the window. Proves the
# placeholder and pre-cut routes in a real terminal that takes kitty graphics.
#
#   TILES_KITTY=/path/to/kitty.app/Contents/MacOS/kitty \
#   drive-kitty.sh <run-id> <name> <kitty|warp> [intent.json]
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: drive-kitty.sh <run-id> <name> <kitty|warp> [intent.json]" >&2; exit 2; }
run="$1"; name="$2"; terminal="$3"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
intent="${4:-$root/crates/tiles-engine/tests/intents/column.json}"
kitty="${TILES_KITTY:?set TILES_KITTY to a kitty binary}"
out="$root/target/verify/$run/$name"
mkdir -p "$out/bin"
tiles="$root/target/release/tiles"
TILES_CACHE_DIR="$out/cache" TILES_COLS=110 TILES_CELL_PX=16x32 TILES_DRAW=block "$tiles" card "$intent" > "$out/card.json"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print("\n".join(["Here is the chart:"]+[l for l in d["text"].splitlines() if l.startswith("[tile:")]+["That is the whole reply."]))' "$out/card.json" > "$out/block.txt"
# The fake answers --version, which tiles run asks before the launch (ADR-032).
printf '#!/bin/sh\n[ "$1" = --version ] && { echo "codex-cli 0.163.0"; exit 0; }\ncat "%s"\nsleep 20\n' "$out/block.txt" > "$out/bin/codex"
chmod +x "$out/bin/codex"
printf '#!/bin/bash\nexport PATH=%q:$PATH TILES_CACHE_DIR=%q TILES_CELL_PX=16x32\nexec %q run --terminal %q codex\n' "$out/bin" "$out/cache" "$tiles" "$terminal" > "$out/session.sh"
chmod +x "$out/session.sh"
title="tiles-verify-$run-$name"
sock="$out/kitty.sock"
cleanup() { "$kitty" @ --to "unix:$sock" close-window --match all >/dev/null 2>&1 || true; }
trap cleanup EXIT
"$kitty" --config NONE -o allow_remote_control=yes --listen-on "unix:$sock" -o font_size=13 \
  -o remember_window_size=no -o initial_window_width=1000 -o initial_window_height=760 \
  -o confirm_os_window_close=0 -o background=#1e1e1e -o foreground=#dddddd --title "$title" \
  "$out/session.sh" >/dev/null 2>&1 &
sleep 6
wid="$(osascript -l JavaScript -e "ObjC.import('CoreGraphics'); var l=ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)); var r=''; for (var i=0;i<l.count;i++){var w=l.objectAtIndex(i); var n=w.objectForKey('kCGWindowName'); if(n && ObjC.unwrap(n)=='$title'){r=ObjC.unwrap(w.objectForKey('kCGWindowNumber'));break;}} r")"
[ -n "$wid" ] || { echo "no kitty window titled $title"; exit 1; }
screencapture -x -l "$wid" "$out/window.png"
echo "evidence: $out"
