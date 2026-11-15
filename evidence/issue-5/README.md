# Residual-basin diagnostic checkpoint

This is retained evidence for [Locate the first invalid event in the residual basin failures](https://github.com/atsyplenkov/surfer2-rs/issues/5). It is preliminary diagnostic work, not a repair or a ticket resolution. This branch must not be merged into `master`.

## Read first

- [Fresh replay checkpoint](checkpoint.md): exact source identities, commands, public failures, and limits.
- [Review assessment](review-synthesis.md): accepted findings and corrections to the independent review.
- [Reported basin-2 collapse](reported-collapse.md): original-support calculations, one retiming experiment, emitted-face observations, and the reduction's changed birth ancestry.

The full basin and the 11-vertex reduction both stop the reported three-support triangle with roughly 0.502 m between stored positions. Retiming only that stop reduces the spread and closes the watched face's printed cycle, but both constructions still fail. The full face then contains an `InfinityChord`, and the remaining spread falls below an existing endpoint threshold. Neither a geometric repair nor complete finite output has been demonstrated.

The next investigation needs a witness preserving the full split parent on original supports 594/595 and its hit on support 601. The existing reduction replaces that parent with a synthetic edge.

## Preserved material

`checkpoint.tar.gz` contains 103 files under `evidence/issue-5/`: the inherited diagnostic reductions and traces, their scripts and historical logs, and the fresh session's reports, requests, source snapshots, patch, calculations, targeted logs and controlled-experiment results. `manifest.json` records every member's byte count and SHA-256 plus the archive hash.

The archive does not include compiled binaries, Cargo targets, the disposable control clone, `.git`, or Python caches. Binary hashes in the reports describe executions already performed; source restoration and rebuilding are the replay path. Historical reduction records cite older binaries whose exact source versions were not captured; those observations remain historical. Fresh replays are separately identified in `session-resume/`.

The source bundle inherited from [the replay evidence](../issue-6/README.md) supplies the exact control and Arm A commits and original fixture bits. The diagnostic source is Arm A `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` plus the archived observer patch and two untracked source snapshots. Keeping those as evidence avoids changing the engine files on this branch.

## Restore an isolated diagnostic checkout

Run from the root of this evidence branch, with a new destination directory:

```sh
EVIDENCE=$PWD
RESTORE=$(mktemp -d /tmp/surfer2-issue5-restore.XXXXXX)
git clone --no-checkout "$EVIDENCE/evidence/issue-6/sources/revisions.bundle" "$RESTORE/repo"
git -C "$RESTORE/repo" checkout --detach origin/evidence/arm-a
tar -xzf "$EVIDENCE/evidence/issue-5/checkpoint.tar.gz" -C "$RESTORE/repo"
cd "$RESTORE/repo"
S=$PWD/evidence/issue-5/session-resume
git apply "$S/tracked.diff"
mkdir -p examples
cp "$S/prototype_basin.rs.snapshot" examples/prototype_basin.rs
cp "$S/prototype_diagnostics.rs.snapshot" src/prototype_diagnostics.rs
cargo build --release --locked --example prototype_basin
```

Use `--offline` if dependencies are already cached. The public red-capable loop is:

```sh
env -u SURFER2_PROTOTYPE_EDGES cargo test --release --locked \
  --test projected_polygons projected_basin_ -- --test-threads=1 --nocapture
```

At this checkpoint it executes two tests and exits 101, with `IncompleteFaceCycle` in each. Exact face numbers are observations, not acceptance criteria.

The detailed report's commands use the original isolated binary path `$S/target/release/examples/prototype_basin`. In a restored checkout either build with `CARGO_TARGET_DIR="$S/target"` or substitute `target/release/examples/prototype_basin`. The source-line calculation scripts use checkout-relative paths. The archived result hash files include the original machine paths; use `manifest.json` to verify a relocated archive.

Run replays only in the restored disposable checkout: calculation scripts and redirected logs can overwrite copied observations. Keep this evidence branch and its archive unchanged. The optional `force_event` and `pin_pair` request fields mutate diagnostic state; baseline requests contain neither. The retained retiming requests explicitly record their one intervention.

To restore the control observer, create a separate clone at `origin/evidence/control` and use `session-resume/control-prototype_basin.rs.snapshot` as `examples/prototype_basin.rs`. For the public control tests use the bundled `origin/evidence/control-harness`, which adds only the common test and fixtures.

## Validation and scope

Restoration checks and their outcomes are recorded in `restoration-check.json` and its logs. No production change, debug-profile check, river replay or full-suite run is claimed for this preservation step. The diagnostic ticket remains open pending further evidence and maintainer discussion.
