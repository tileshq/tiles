#!/usr/bin/env bash
# macOS only: opens a kitty window running `tiles run --terminal kitty codex`,
# where `codex` is a fake harness that makes its card through `tiles card`
# after the wrapper starts (as the MCP server would, with the frame count
# `tiles run` hands it), waits 1.2 s, then types the block a row every 60 ms on
# a #0a0a0a conversation with a "Build" line under the typed row, as OpenCode
# does. Keeps 12 screenshots from the block's first row on, and one after.
#
#   TILES_KITTY=/path/to/kitty.app/Contents/MacOS/kitty \
#   drive-kitty-stream.sh <run-id> <name> <intent.json>
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: drive-kitty-stream.sh <run-id> <name> <intent.json>" >&2; exit 2; }
run="$1"; name="$2"; intent="$3"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
kitty="${TILES_KITTY:?set TILES_KITTY to a kitty binary}"
tiles="$root/target/release/tiles"
out="$root/target/verify/$run/$name"
rm -rf "$out"; mkdir -p "$out/bin" "$out/shots"
# The fake answers --version, which tiles run asks before the launch (ADR-032).
cat > "$out/bin/codex" <<SH
#!/bin/bash
[ "\$1" = --version ] && { echo "codex-cli 0.163.0"; exit 0; }
bg=\$'\e[48;2;10;10;10m'
printf '\e[2J\e[H'
for i in \$(seq 1 3); do printf '%s  %-90s\e[0m\r\n' "\$bg" "conversation line \$i"; done
sleep 1
"$tiles" card "$intent" > "$out/card.json"
sleep 1.2
printf '%s  %-90s\e[0m\r\n' "\$bg" "Here is the chart:"
touch "$out/started"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print("\n".join(l for l in d["text"].splitlines() if l.startswith("[tile:")))' "$out/card.json" | while IFS= read -r line; do
  printf '%s  %s\e[K\e[0m\r\n' "\$bg" "\$line"
  printf '%s  %-90s\e[0m\r' "\$bg" "▣  Build · gpt-5"
  sleep 0.06
done
printf '%s  %-90s\e[0m\r\n' "\$bg" "That is the whole reply."
sleep 30
SH
chmod +x "$out/bin/codex"
printf '#!/bin/bash\nunset HERDR_BIN_PATH HERDR_ENV HERDR_PANE_ID HERDR_SESSION HERDR_SOCKET_PATH HERDR_TAB_ID HERDR_WORKSPACE_ID TERM_PROGRAM TERM_PROGRAM_VERSION\nexport PATH=%q:$PATH TILES_CACHE_DIR=%q\nexec %q run --terminal kitty codex\n' "$out/bin" "$out/cache" "$tiles" > "$out/session.sh"
chmod +x "$out/session.sh"
title="tiles-verify-$run-$name"
sock="$out/kitty.sock"
cleanup() { "$kitty" @ --to "unix:$sock" close-window --match all >/dev/null 2>&1 || true; }
trap cleanup EXIT
"$kitty" --config NONE -o allow_remote_control=yes --listen-on "unix:$sock" -o font_size=13 \
  -o remember_window_size=no -o initial_window_width=1400 -o initial_window_height=760 \
  -o confirm_os_window_close=0 -o background=#1e1e1e -o foreground=#dddddd --title "$title" \
  "$out/session.sh" >/dev/null 2>&1 &
wid=""
for _ in $(seq 1 20); do
  sleep 0.3
  wid="$(osascript -l JavaScript -e "ObjC.import('CoreGraphics'); var l=ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)); var r=''; for (var i=0;i<l.count;i++){var w=l.objectAtIndex(i); var n=w.objectForKey('kCGWindowName'); if(n && ObjC.unwrap(n)=='$title'){r=ObjC.unwrap(w.objectForKey('kCGWindowNumber'));break;}} r")"
  [ -n "$wid" ] && break
done
[ -n "$wid" ] || { echo "no kitty window titled $title"; exit 1; }
for _ in $(seq 1 100); do [ -e "$out/started" ] && break; sleep 0.05; done
for i in $(seq -w 0 11); do screencapture -x -o -l "$wid" "$out/shots/$i.png"; done
sleep 2
screencapture -x -o -l "$wid" "$out/shots/final.png"
echo "evidence: $out"
