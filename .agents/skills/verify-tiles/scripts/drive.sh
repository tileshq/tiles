#!/usr/bin/env bash
# Drives one `tiles render` the way a user types it, in a tmux session this
# run owns, and keeps the evidence under target/verify/<run>/<name>/.
#
#   drive.sh <run-id> <name> [--env NAME=value]... <tiles args...>
#   drive.sh r1 column-light render crates/tiles-engine/tests/intents/column.json -o '$OUT/card.png'
#   drive.sh r1 plan --env TILES_OMNI_URL=https://acme.omniapp.co run --dry-run claude
#
# Each --env sets one variable for the command only.
# '$OUT' in the arguments is replaced by the evidence directory. The script
# prints the evidence directory and the command's exit code, and always kills
# the session it started (and only that one).
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: drive.sh <run-id> <name> [--env NAME=value]... <tiles args...>" >&2; exit 2; }
run="$1"; name="$2"; shift 2
vars=()
while [ "${1:-}" = "--env" ]; do
  [[ "${2:-}" =~ ^[A-Z_][A-Z0-9_]*= ]] || { echo "drive.sh: --env needs NAME=value" >&2; exit 2; }
  vars+=("$2"); shift 2
done
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
out="$root/target/verify/$run/$name"
mkdir -p "$out"
# A name reused in one run must not read the last drive's exit code.
rm -f "$out/exit-code.txt" "$out/transcript.txt"
session="tiles-verify-$run-$name"
cleanup() { tmux kill-session -t "$session" 2>/dev/null || true; }
trap cleanup EXIT
args=()
for a in "$@"; do args+=("${a//\$OUT/$out}"); done
cmd="$root/target/release/tiles"
if [ ${#vars[@]} -gt 0 ]; then printf '%q ' env "${vars[@]}" > "$out/command.txt"; else : > "$out/command.txt"; fi
printf '%q ' "$cmd" "${args[@]}" >> "$out/command.txt"
echo >> "$out/command.txt"
tmux new-session -d -s "$session" -x 160 -y 40 -c "$root"
tmux send-keys -t "$session" "$(cat "$out/command.txt"); echo \"exit=\$?\" > '$out/exit-code.txt'" Enter
for _ in $(seq 1 100); do [ -s "$out/exit-code.txt" ] && break; sleep 0.1; done
tmux capture-pane -p -t "$session" -S - > "$out/transcript.txt"
code="$(cut -d= -f2 < "$out/exit-code.txt" 2>/dev/null || echo timeout)"
( cd "$out" && for f in *.png frames/*.png; do [ -f "$f" ] && echo "$(shasum -a 256 "$f" | cut -c1-16) $(file -b "$f" | cut -d, -f1-2) $f"; done ) > "$out/files.txt" 2>/dev/null || true
echo "evidence: $out"
echo "exit: $code"
