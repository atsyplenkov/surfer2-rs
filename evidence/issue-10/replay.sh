#!/usr/bin/env bash
# THROWAWAY replay. Creates only new scratch checkouts; never edits the caller's tree.
set -euo pipefail
D="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE="${1:?Usage: bash replay.sh /absolute/path/to/revisions.bundle}"
ROOT="$(mktemp -d /tmp/surfer2-issue10-replay.XXXXXX)"
printf 'Replay directory: %s\n' "$ROOT"
for MODE in baseline intervention control; do
  git clone --quiet --no-checkout "$BUNDLE" "$ROOT/$MODE"
  REV=1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c
  PATCH=observer.diff
  if [[ "$MODE" == control ]]; then
    REV=d1a89fd66795f537b034f220ed4f838bd0fdea3b
    PATCH=control-observer.diff
  fi
  git -C "$ROOT/$MODE" checkout --quiet --detach "$REV"
  git -C "$ROOT/$MODE" apply "$D/$PATCH"
  if [[ "$MODE" == intervention ]]; then
    git -C "$ROOT/$MODE" apply "$D/intervention-only.diff"
  fi
  mkdir -p "$ROOT/$MODE/examples"
  cp "$D/sources/prototype_basin.rs" "$ROOT/$MODE/examples/"
  cp "$D/sources/prototype_diagnostics.rs" "$ROOT/$MODE/src/"
  (cd "$ROOT/$MODE" && cargo build --release --offline --locked --example prototype_basin) > "$ROOT/$MODE-build.log" 2>&1
  SURFER2_PROTOTYPE_EDGES=0,1,2,3,6,7 "$ROOT/$MODE/target/release/examples/prototype_basin" \
    < "$D/representative-request.ndjson" > "$ROOT/$MODE.json" 2> "$ROOT/$MODE.log"
  python3 - "$D/$MODE.json" "$ROOT/$MODE.json" <<'PY'
import json, sys
with open(sys.argv[1]) as f: expected = json.load(f)
with open(sys.argv[2]) as f: observed = json.load(f)
assert observed == expected
print(sys.argv[2], 'matches retained trace')
PY
  cmp "$D/$MODE.log" "$ROOT/$MODE.log"
  if BINARY="$ROOT/$MODE/target/release/examples/prototype_basin" bash "$D/red.sh" > "$ROOT/$MODE-red.log" 2>&1; then
    printf 'Unexpected green witness: %s\n' "$MODE" >&2
    exit 1
  else
    STATUS=$?
    [[ "$STATUS" == 1 ]] || exit "$STATUS"
    printf '%s witness remains red, as recorded\n' "$MODE"
  fi
done
cp "$D/sources/prototype_active.rs" "$ROOT/baseline/examples/"
(cd "$ROOT/baseline" && cargo build --release --offline --locked --example prototype_active) > "$ROOT/active-build.log" 2>&1
SURFER2_PROTOTYPE_EDGES=0,1,2,3,6,7 "$ROOT/baseline/target/release/examples/prototype_active" \
  < "$D/representative-request.ndjson" > "$ROOT/active.json" 2> "$ROOT/active.log"
cmp "$D/active.json" "$ROOT/active.json"
cmp "$D/active.log" "$ROOT/active.log"
printf 'All retained observations reproduced. This is not a successful skeleton.\n'
