# Commands and checks

All paths below identify the fresh scratch worktrees used in this pass. Existing worktrees and master were not edited. The repository's `justfile` uses release tests and locked dependencies; this bounded diagnostic pass uses the same release profile, without claiming the full suite.

## Restoration and first red loop

Created detached checkouts at Arm A `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` for baseline/intervention, and at control `d1a89fd66795f537b034f220ed4f838bd0fdea3b` for control. The evidence-only branch starts at published `244944daf2f0fc2ba9e21b412e424257a1574800`.

The retained `checkpoint.tar.gz` and `ancestry-preserving.tar.gz` were extracted into the new baseline worktree. The archived `tracked.diff` and the two Rust observer snapshots restored the original observer.

```sh
cd /tmp/surfer2-issue10-baseline
cargo build --release --offline --locked --example prototype_basin
sha256sum target/release/examples/prototype_basin
BINARY="$PWD/target/release/examples/prototype_basin" \
  bash evidence/issue-5/session-resume/ancestry-preserving/red.sh
```

Build exit 0. The rebuilt original observer hash was `1add446a198f4dbb7538bf243ce02db96fc4a7d1c2b216c9dc6f51c76bc8250c`, exactly the archived executable hash. The witness assertion exited 1, reporting the source-601 incomplete face. The earlier first invocation used the preserved original binary and returned the same red result.

Read-only endpoint mapping logs were then added. This changes the executable hash but leaves parsed baseline JSON identical to the archived trace. These are the `observer.diff` sources preserved here. Control uses the same example and observation points except for the baseline-only directed-halfedge-dedup log; its corresponding patch is separate because that policy does not exist in control.

One attempted application of the baseline observer patch to control failed on the dedup logging context; `git apply` applied no hunks. The compatible read-only `control-observer.diff` was then constructed. One initial active-observer build failed because the endpoint IDs were incorrectly treated as optional; the observer was corrected to match the existing field type, and the rebuild and equality check passed. Neither failure changed engine policy.

## Single-motion experiment

The intervention adds only `intervention-only.diff` on top of the baseline observer. It changes the successor velocity before insertion or any subsequent prediction. A log records its one execution. No request uses the inherited `force_event` or `pin_pair` fields.

For each of `baseline`, `intervention`, and `control`:

```sh
cd /tmp/surfer2-issue10-MODE
cargo build --release --offline --locked --example prototype_basin
SURFER2_PROTOTYPE_EDGES=0,1,2,3,6,7 target/release/examples/prototype_basin \
  < /tmp/surfer2-issue10-evidence/evidence/issue-10/representative-request.ndjson \
  > /tmp/surfer2-issue10-evidence/evidence/issue-10/MODE.json \
  2> /tmp/surfer2-issue10-evidence/evidence/issue-10/MODE.log
BINARY="$PWD/target/release/examples/prototype_basin" \
  bash /tmp/surfer2-issue10-evidence/evidence/issue-10/red.sh
```

All builds and observer invocations exited 0. Each `red.sh` invocation exited 1. Repeated JSON and log outputs were identical. Per-run elapsed times and binary hashes are in `replay-verification.json`.

```sh
python3 /tmp/surfer2-issue10-evidence/evidence/issue-10/compare.py
```

Exit 0. Checks fixed witness indices, no direct time/birth overrides, identical initial states and first six events, exactly one changed shared vertex velocity, unchanged origin, exactly one intervention log, and emitted endpoint mapping for all final traced arcs. It writes `comparison.json` and prints `comparison.log`'s contents. Its pass means observations are consistent with the recorded experiment, not that a skeleton is valid.

The common source hash inventories were compared exhaustively over tracked `src/`, Cargo manifests/lock, and the observer snapshots. Only `src/propagator.rs` differs between baseline and intervention. The delta is retained separately; no other policy file differs.

## Active support observation

```sh
cd /tmp/surfer2-issue10-baseline
cargo build --release --offline --locked --example prototype_active
SURFER2_PROTOTYPE_EDGES=0,1,2,3,6,7 target/release/examples/prototype_active \
  < /tmp/surfer2-issue10-evidence/evidence/issue-10/representative-request.ndjson \
  > /tmp/surfer2-issue10-evidence/evidence/issue-10/active.json \
  2> /tmp/surfer2-issue10-evidence/evidence/issue-10/active.log
```

Exit 0. Removing only `active_vertices`, `active_edges`, and `live_triangles` recursively yields exact parsed equality with baseline. The source certificate also rechecks that equality and the listed local support endpoint relationships.

## Independent source calculations and complete replay

```sh
PYTHONDONTWRITEBYTECODE=1 python3 \
  /tmp/surfer2-issue10-evidence/evidence/issue-10/calculations/certify_witness.py \
  --restored /tmp/surfer2-issue10-baseline \
  --traces /tmp/surfer2-issue10-evidence/evidence/issue-10 \
  > /tmp/surfer2-issue10-certificate-verification.json
cmp /tmp/surfer2-issue10-certificate-verification.json \
  /tmp/surfer2-issue10-evidence/evidence/issue-10/calculations/certificate.json
bash /tmp/surfer2-issue10-evidence/evidence/issue-10/replay.sh \
  /tmp/surfer2-issue5-evidence/evidence/issue-6/sources/revisions.bundle
bash -n /tmp/surfer2-issue10-evidence/evidence/issue-10/replay.sh \
  /tmp/surfer2-issue10-evidence/evidence/issue-10/red.sh
```

All exited 0. Certificate reproduction took about 2.1 seconds. Complete source-bundle restoration took about 81 seconds, rebuilt all three observers and the active-state example, and reproduced all retained JSON/log observations. See `restoration.log`.

The staged `git diff --check` initially flagged the required single-space blank context lines inside preserved `.diff` files as trailing whitespace. Those patch bytes were kept unchanged. The check excluding `.diff` artifacts, and checks of all three applied source diffs, passed.

Skipped checks are listed in the main report. No debug, full-basin, river/lake, complete suite, clippy, or external exact-engine result is implied by these checks.
