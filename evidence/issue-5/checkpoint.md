# First diagnostic checkpoint: basin 2

Status: **preliminary; no repair, resolution, or claim of the globally first invalid event**. This checkpoint is for discussion before further investigation. All new files made in this session are under this directory. Earlier diagnostics, source files, the main checkout, and the tracker were not modified.

## Revisions and provenance

- The comparison is the existing Arm A worktree at `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` with a pre-existing, uncommitted observer. The tracked diff is saved as `tracked.diff`, SHA-256 `5fecc7540bdd7d8ccba02fd93ca58c34997e795169720d55e8fa4f6547638605`. Its current hash still matches. Inspection found watch-filtered logging and a rewrite of `compute_collapse(...)` through a local variable; the example also supports interventions, but the requests used here do not request any and the resulting intervention lists are empty. This is **not a clean Arm A build**.
- The untracked observer files are copied as `prototype_diagnostics.rs.snapshot` (SHA-256 `15628603766bb34058a571e38ec673433408a93da8f9d3581bdfd2dfec89a4fe`) and `prototype_basin.rs.snapshot` (`903db527fd688287d6035efaace05025957391fd49322c70c5655f1bcd3d943f`).
- The control engine worktree is at `d1a89fd66795f537b034f220ed4f838bd0fdea3b`, with no tracked diff and its pre-existing untracked observer example (`control-prototype_basin.rs.snapshot`, SHA-256 `eebab64e3289ad3bf6657a53508d7ea4698253ff2b7c0972c1488613c46e48a5`). Its example differs from the Arm A example; the latter's optional intervention features were disabled here.
- The clean control public-test harness is bundled commit `d96984f66eabe97ab003c311b764c5c3a7c5a2e7`, i.e. the control engine with Arm A's test and fixtures. It was locally cloned from the retained bundle, SHA-256 `6a1d007a1b65c0220126846c55c91a5db62f3a819a7607eebe734ebc853957cd`. Its `tests/projected_polygons.rs` and the comparison's test have SHA-256 `b15af0682850b71cd288f05c02a5518f7d3c629b30371dafc0d2b938f16c56a5`.
- Basin 1 fixture SHA-256: `486185b932322440f0a773496b67c80bc238e453c83253ae9a20d2e91631bf77`; basin 2 fixture: `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a`. Existing basin-2 early-reduced request file: `1f96d5c0b4aa5c9244f82e3ce8d848de99c259800f55952f1c15091dfc3a6e9d`; existing basin-2 reported-reduced request: `a683ab5c39427c6c961da3d87f66699f834e25f759c9c07fcdd735d661ecec83`. The JSON-line requests produced from them are retained here; see `input-sha256.txt` for their hashes.
- Toolchain: `rustc 1.95.0 (59807616e 2026-04-14)` and `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`. All builds were release profile, `--locked --offline`, with isolated targets under this directory. `binary-sha256.txt` and `control-provenance.txt` record the actual binaries.

## Fresh public failures

From `/tmp/opencode/surfer2-first-invalid`, with `D=$PWD/evidence/issue-5/session-resume`, the following exact command was run twice:

```sh
env -u SURFER2_PROTOTYPE_EDGES CARGO_TARGET_DIR="$D/target" CARGO_TERM_COLOR=never \
  cargo test --release --offline --locked --test projected_polygons \
  projected_basin_ -- --test-threads=1 --nocapture
```

Both runs exited 101, executing exactly two tests. Basin 1 returned `IncompleteFaceCycle`, bounded validation, face 13 with 5 owned halfedges; basin 2 returned the same error class, face 600 with 10 owned halfedges. First run including isolated compilation: 22.988 s; second run: 0.303 s. Logs: `public-basin-run-{1,2}.log`. Face numbers describe this run and are not acceptance criteria.

For the clean control harness, after a local bundle clone and `git checkout --detach origin/evidence/control-harness`, the same Cargo command was run from `control-harness/` with `CARGO_TARGET_DIR="$D/target-control-harness"`. It exited 101 after 22.688 s including build: basin 1 `IncompleteFaceCycle` face 277, basin 2 face 18. Log: `control-public-basin-run-1.log`. Thus both full-basin public failures exist on the control as well as on the instrumented comparison; this result alone says nothing about their respective causes.

## Reduced and full observer replay

The existing example was freshly built in each worktree using `cargo build --release --offline --locked --example prototype_basin`, with isolated targets `target/` and `target-control/`. For example, the representative early reduction was run twice as follows:

```sh
env -u SURFER2_PROTOTYPE_EDGES "$D/target/release/examples/prototype_basin" \
  < "$D/basin-2-early-reduced-request.ndjson" \
  > "$D/basin-2-early-reduced-run-1.json"
python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["status"] == "connected", "finite skeleton is not connected: " + str(d.get("error", d["status"]))' \
  "$D/basin-2-early-reduced-run-1.json"
```

The example returns process status 0 even when construction fails; the separate assertion is the red-capable verdict. Both comparison runs returned `dcel_error` / `IncompleteFaceCycle`, and the assertion exited 1. The runs produced identical JSON and took 0.004 s each. `early-reduced-public-assert.log` records the assertion. `dcel_error` means construction failed; it does **not** prove that a returned finite graph is disconnected.

The 21-vertex reduction preserves original vertex indices 321–327 around the observed incident constraints; other edges can be newly formed by deletion. The existing reducer records deletion-1-minimality only among **unprotected** vertices, with those witness vertices locked. It is not a globally minimal polygon. We did not rerun the reducer or independently certify its topology here.

The separately retained 11-vertex reported-event reduction also returned `dcel_error` twice, with identical JSON and a red connectedness assertion; each observer run took 0.003 s. Its protected range is 594–602. The clean control example returned a connected graph on the early reduction, but a disconnected graph on the reported-event reduction. These graph counts do not independently establish complete geometric output.

The full basin-2 request was also run with the fresh comparison example (`basin-2-full-request.ndjson` to `basin-2-full-run-1.json`): it returned `dcel_error` face 600 in 0.739 s. Its event, birth, and final vertex arrays exactly matched the prior `pl_basins-2-scan.json`. The control example ran the same full request and returned `dcel_error` face 18; see `basin-2-full-control.json` and `control-comparison-summary.log`. The examples are diagnostic stepping drivers, not substitutes for the public tests above.

## What the fresh trace and calculations show

- On the comparison's full basin-2 trace, step 52 at `t=25.496700756094473` is a constraint collapse with original constraints `[0,322,323]` and `[0,324,325]` and observed vertex support pairs `(325,324)`, `(324,322)`, `(322,321)`. The clean control trace also selects the corresponding collapse (step 54, `t=25.496700756094967`).
- The comparison then reports step 53 `SplitOrFlipRefine`, step 54 `VertexMovesOverSpoke`, and step 55 `TriangleCollapse` at `t=25.49672364617444`. At step 55 the observer calculates a maximum pairwise separation of `101.98679768254352` from the stored vertex motions. The control's corresponding full trace does not show those three events in that time window. The early reduction repeats the same comparison-side pattern with separation `101.98679768266881`; its clean-control replay does not show it. This is a concrete divergence associated with the comparison, **not yet a controlled causal isolation of an Arm A change**.
- The observer logs meetings only above `0.005` metres, a selection threshold, not an accuracy bound or proof that no earlier event is invalid. The spread is calculated from the engine's stored trajectories. It flags a co-arrival question; verifying the active vertices, applicable event semantics and independently expected physical state remains necessary.
- The existing `certify.py` was imported without executing its file-writing main block, pointed at the fresh full trace, and run for step 53 (`early`) and step 2770 (`reported`). Its results and output are `basin-2-certificates-fresh.json` and `basin-2-certify-fresh.log`; the calculation took 2.176 s. The code uses exact rational binary64 input coordinates and square-root enclosures on a `2^-192` grid, but uses trace-provided initial support identity/direction to select and orient the lines. Its implementation was not independently audited in this session.
- Under that calculation's oriented source-support assumptions, hypothetical co-arrival of shared supports 322/324 with support 321 is enclosed at `25.49670075580307111584..25.49670075580307111585`, and with support 325 at `25.49670075580306584297..25.49670075580306584298`, with positive difference `5.27286..5.27287 × 10^-15` seconds. This is a statement about **source-support line intersections**. It does not establish that both candidate events are active or admissible in the actual region, that event order must follow from these numbers alone, or that step 52 is the globally earliest invalid decision.
- At the later reported full-basin `TriangleCollapse` (step 2770, `t=649.3054124144818`), the same source-support calculation encloses the three-line reference time at `649.30541366328843704446..649.30541366328843704447` and the observed-minus-reference time at approximately `-1.24880664369608 × 10^-6` seconds. The stored fast position differs from that calculated reference point by roughly `(-0.5016762, 0.0042353)` metres. This is a local support calculation, conditional on the chosen supports and their orientation; it does not by itself prove an admissible physical event or identify the cause of the final face error.

## Pending and limits

No causal link has been established from the early 101.99 m observation to the full basin's face-600 error, nor from the reported 0.5 m observation to that error. The reductions' synthetic edges may introduce other failures. Earlier events and birth/support invariants have not been exhaustively checked, so even the first observer-selected large spread is not a demonstrated first invalid event. The control's different behavior does not prove which comparison change causes it. Basin 1's reduced case, point-contact topology, the river, the `Never`/`Degenerate` paths, arc identity, deduplication, and finite-export completeness were not newly investigated here. No corpus, oracle, river, debug-profile, or full-suite check was run.

The diagnostic work is paused for approval before additional tracing or intervention. The planning ticket must remain open pending maintainer discussion and further evidence.
