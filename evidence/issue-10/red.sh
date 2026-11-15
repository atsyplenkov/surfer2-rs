#!/usr/bin/env bash
# Throwaway red-capable observer check for the 12-vertex basin-2 witness.
# Override BINARY to test another compatible observer build. This checks
# connectedness/construction only; it does not prove geometric completeness.
set -euo pipefail
D="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
S="$(dirname -- "$D")"
BINARY="${BINARY:-$S/target/release/examples/prototype_basin}"
env -u SURFER2_PROTOTYPE_EDGES "$BINARY" < "$D/representative-request.ndjson" |
  python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["status"] == "connected", "expected connected finite skeleton; observed " + str(d.get("error", d["status"])); print("PASS: connected coordinate graph", d.get("graph"))'
