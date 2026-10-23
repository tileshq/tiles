#!/usr/bin/env bash
# Drives the MCP server the way a harness does: initialize, tools/list, then
# one tools/call with an intent file, over stdio. Keeps every reply and the
# server's stderr as evidence under target/verify/<run>/<name>/.
#
#   drive-mcp.sh <run-id> <name> <intent.json> [VAR=value ...]
#   drive-mcp.sh r1 mcp-block crates/tiles-engine/tests/intents/column.json TILES_DRAW=block
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: drive-mcp.sh <run-id> <name> <intent.json> [VAR=value ...]" >&2; exit 2; }
run="$1"; name="$2"; intent="$3"; shift 3
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
out="$root/target/verify/$run/$name"
mkdir -p "$out"
args="$(python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))))' "$intent")"
{
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"verify-tiles","version":"1"}}}'
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
  echo "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"tiles_chart\",\"arguments\":$args}}"
} > "$out/requests.jsonl"
env TILES_CACHE_DIR="$out/cache" "$@" "$root/target/release/tiles" mcp < "$out/requests.jsonl" > "$out/replies.jsonl" 2> "$out/stderr.txt"
echo "exit=$?" > "$out/exit-code.txt"
python3 - "$out/replies.jsonl" > "$out/summary.txt" <<'PY'
import json, sys
for line in open(sys.argv[1]):
    m = json.loads(line)
    r = m.get("result", {})
    if "protocolVersion" in r:
        print("initialize", r["protocolVersion"], r["serverInfo"]["name"])
    elif "tools" in r:
        print("tools", [t["name"] for t in r["tools"]])
    elif "content" in r:
        print("isError", r["isError"], [c["type"] for c in r["content"]])
        print(r["content"][0]["text"])
    else:
        print("error", m.get("error"))
PY
ls "$out/cache/cards" > "$out/cards.txt" 2>/dev/null || true
echo "evidence: $out"
cat "$out/exit-code.txt"
