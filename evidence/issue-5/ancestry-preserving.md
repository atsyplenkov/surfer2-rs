# Basin-2 witness with original split ancestry

**Outcome:** a 12-vertex diagnostic polygon preserves the full-input local split ancestry and exposes a supported comparison-specific scheduling mechanism. This is a local finding, not a production repair or a proof of the globally first invalid event. The earlier diagnostic evidence was preserved by the main session at [the retained diagnostics commit](https://github.com/atsyplenkov/surfer2-rs/commit/77614bd1ae3c312b046aaae03ceb2b7d9c496fd0) and linked from [Locate the first invalid event in the residual basin failures](https://github.com/atsyplenkov/surfer2-rs/issues/5#issuecomment-6891553805). The supporting directory is preserved in `ancestry-preserving.tar.gz` on the retained `evidence/issue-5-diagnostics` branch. The archive keeps the original handback report unchanged; this readable copy adds its publication location.

## Witness and reproduction

Starting from the earlier 11-vertex reduction, restoring original vertex **603** yields the 12 original vertex indices:

```text
594, 595, 596, 597, 598, 599, 600, 601, 602, 603, 1143, 3151
```

`representative.py` tried only the original 11-point candidate and this one, in two observer calls and 0.071 s wall time. It checked all vertex bits against the full basin input and checked simplicity, nonzero area with preserved orientation, proper crossings, duplicate vertices, and collinear overlaps using exact integer predicates over the binary64 dyadic inputs. The witness has no holes. Edges `[603,1143]`, `[1143,3151]`, and `[3151,594]` are synthetic, so the polygon is not a complete reproduction of the original region. The local birth ancestry and active triangle described below use original edges, not those connectors. No global minimality claim is made.

From this directory the red-capable check is:

```sh
./red.sh
```

It uses the previously built, instrumented comparison observer. It exits **1** in 0.063 s with `IncompleteFaceCycle`, source edge `[0,601,602]` (observed face 5). The check asserts construction and a connected coordinate graph, not full geometric completeness. The helper accepts `BINARY=/path/to/a/compatible/prototype_basin` for another build. For the clean control observer:

```sh
BINARY="$PWD/../target-control/release/examples/prototype_basin" ./red.sh
```

This also exits **1**, in 0.047 s, because it returns two coordinate-graph components (19 edges, 21 nodes, rank 0). Thus control is not a correct result. The diagnostic example returns JSON with process exit 0; `red.sh` supplies the actual failing assertion. Logs are `red-comparison.log` and `red-control.log`. This pass did not rerun the full public tests; the already-retained checkpoint contains their red results on both full basins.

## Matched ancestry and finite-hit evidence

The witness comparison's steps 4, 6, and 7 correspond to full-basin steps 2195, 2769, and 2770. `parent_match.py` verifies equal event kinds, times and original constraint IDs; for each participating original support pair it verifies exact stored origin/velocity bits and equal recorded birth and birth time. `witness_reference.py` also matches the three final-vertex stored origin/velocity bits, birth bits/times, event time and spread between the witness and full input **within each revision**. Vertex and triangle IDs are local and differ. This is local-state correspondence, not equality of whole propagation histories.

At the full-input and witness split at `t=649.3052873722798`:

- The split parent is an initial vertex on original supports 594 and 595. It hits original edge 601, whose target endpoints use original support pairs 601/602 and 601/596. The latter endpoint was itself born in the earlier original-edge split at `t=371.0291729650514`; its parent uses 597/596 and its hit edge is 601. The observed witness events through the reported stop use original support identities; see `ancestry-chain.log`.
- The observed finite-segment fraction is approximately `0.499998857089`; the observed normal distance is approximately `4.23e-10` m. Independently evaluating original source-support lines with exact dyadic inputs and enclosed square roots gives hit time `649.30528733131290608687..649.30528733131290608688` and segment fraction `0.49999885705603042819..0.49999885705603042820`. The observed event is about 40.97 ns later. The certified fraction is strictly inside the source-defined segment **conditional on the selected supports being the active endpoints**. The observer shows those endpoints in the queued triangle; this does not independently certify every earlier event or the full active region.
- The split creates a finite fast successor on original supports 595/601. The comparison's stored trajectory misses its recorded birth by **0.02075466918 m**. In the clean control the corresponding successor misses its recorded birth by **0.03364081385 m**. This replay inconsistency occurs in both revisions; it is distinct from the later comparison-specific 0.502 m separation.

The earlier 11-vertex reduction did **not** preserve this ancestry: its fast child's parent used synthetic edge `[602,1143]`. Restoring vertex 603 gives the 12-vertex witness's target endpoint the original 602/603 support and recovers the full-input split parent on 594/595. The two reductions are not interchangeable for ancestry claims.

## Supported local scheduling mechanism

At the comparison's final three-constraint triangle, the actual watch log reports these scalar edge times:

| Original support | Scalar x-quotient time |
| --- | ---: |
| 596 | `649.3054124144818` |
| 595 | `649.3054136530183` |
| 601 | `649.3054136530184` |

The existing three-constraint scheduler selects the minimum and emits `TriangleCollapse`, stopping all three finite vertices. In the comparison, that leaves their recorded positions **0.50169532046 m** apart. The stored fast vertex's x velocity is approximately `405055.655` m/time-unit: the approximately `1.2385e-6` time-unit lead of the selected 596 quotient over the other two is sufficient to explain the observed half-metre displacement. The independent three-source-line meeting is enclosed at `649.30541366328843704446..649.30541366328843704447`, subject to the support-orientation and active-region assumptions.

`scalar_times.py` independently recomputes the x and y quotients from the recorded dyadic motions. For support 596 on the comparison, x gives `649.3054124144818` and y gives `649.3054136927502`; their exact-dyadic difference is approximately `-1.27826846e-6`. The x denominator is only `0.0168653641921`, so a small x-position difference changes the scalar time substantially. The same source `wavefront/edge.rs` on both revisions uses the x quotient unless the edge is exactly vertical; its two copies have the same SHA-256. The watch log confirms the comparison scheduler's chosen scalar values. For control, the independently recomputed raw 596 x quotient is `649.3054136718612`, and the observed chosen time is the 595 quotient `649.3054136530183`. The control's final recorded spread is **2.72877738e-7 m**, not 0.502 m.

The relevant slow vertex on supports 596/601 has the same stored origin on both revisions, but different stored velocities:

```text
comparison: (-0.9915316883380821, 1.0084083568463786)
control:    (-0.9915316883707419, 1.0084083573892713)
```

At the comparison event time the exact-dyadic difference between the two stored x motions is approximately `2.12061614e-8` m. Dividing a displacement of that size by the relative x speed above gives a time sensitivity of approximately `1.26e-6`. `motion_formulas.py` reconstructs both source formulas from the traced original support data: the comparison's stored velocity exactly matches the normal formula; the control's exactly matches time-one minus time-zero subtraction. Their relative difference is `3.84574914e-10`, above Arm A's `1e-10` projected-frame disagreement rule. The source files and hashes are listed in `external-inputs.sha256`.

This ties the **observed local differential** to the comparison's motion-selection policy interacting with x-only timing and the all-three stop. It does not show that the normal formula is inherently wrong, that reverting Arm A would repair the basin, or that this is the only difference responsible for a full-input outcome. The formula reconstruction matches the recorded bits and source rule; no isolated source-change intervention was performed in this pass.

A second local inconsistency precedes the reported birth: comparison witness step 5 / full step 2196 stops a three-constraint triangle at `371.02917302656596` with a recorded spread of `0.00722315483` m. It uses original supports 597, 600, and 601. This and the inherited birth replay miss rule out treating the later scalar as a complete explanation of all local geometry. No claim is made that this is the first invalid step globally.

## From stop to failed face

`witness-reference.log` records the three kinetic arcs ending at the reported stop. On original source face 601 the slow trace ends near `(463811.7052512257, 6272413.626816563)` and the fast trace ends near `(463811.2035737932, 6272413.631053105)`. The watched emission records those endpoints on face 5, source 601, which owns nine halfedges and has no root; see `comparison-watched.log` and `face-evidence.log`. The corresponding full comparison's source-601 face also fails. This connects the observed separation to the affected face's emitted boundary; it does not independently establish the unique cause of the entire face error or exported completeness.

The previous checkpoint's explicitly artificial retiming experiment changed the same full-input triangle time, moved the first validation failure, and produced a target-face cycle containing an `InfinityChord` across an earlier gap. It remains a sensitivity experiment, not a proposed fix or proof of valid finite output. No new intervention was performed in this pass.

## Arithmetic bounds and remaining proof

The source-line calculations use the prior `certify.py` rational intervals over exact input dyadics, with directions selected from the traced initial supports. At the reported comparison stop, direct exact-dyadic evaluation of the stored origin, velocity and time differs from the logged fast x position by about `5.4e-9` m; this bounds only that final expression's rounding for this sample. It is far smaller than the recorded 0.502 m spread. It is **not** a general physical or topological error budget, and it does not validate birth positions, earlier event order, or all upstream arithmetic.

The remaining blocker to a complete causal and repair decision is to establish a justified error budget and active-wavefront lifecycle for the relevant predecessor and earlier 7.2 mm stop, then assess how a candidate policy handles the coupled motion, scheduling, stop identity, emitted arcs and finite export. The original source-line segment calculation assumes the observer's selected supports are active; the full region has not been independently replayed as exact finite wavefront topology. Nor has a one-change source counterfactual isolated the policy from every other revision difference. A connected control result cannot be assumed: this witness returns two components there and the full control still fails.

## Commands and provenance

All new files in this pass are under this directory. No existing source or diagnostic was edited, and no git, network, or tracker operation was performed. The representative run uses Arm A commit `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` with the previously captured observer diff `5fecc7540bdd7d8ccba02fd93ca58c34997e795169720d55e8fa4f6547638605`; the control is `d1a89fd66795f537b034f220ed4f838bd0fdea3b`. The main session checked the observer source hash before this pass. `external-inputs.sha256` records input, source, prior full-trace, and binary hashes; `manifest.sha256` records the new files.

From `/tmp/opencode/surfer2-first-invalid`, with `S=$PWD/evidence/issue-5/session-resume` and `D=$S/ancestry-preserving`, the commands run were:

```sh
env -u SURFER2_PROTOTYPE_EDGES PYTHONDONTWRITEBYTECODE=1 python3 "$D/representative.py"
env SURFER2_PROTOTYPE_EDGES=0,1,2,3,6,7 "$S/target/release/examples/prototype_basin" \
  < "$D/representative-request.ndjson" > "$D/comparison-watched.json" 2> "$D/comparison-watched.log"
env -u SURFER2_PROTOTYPE_EDGES "$S/target-control/release/examples/prototype_basin" \
  < "$D/representative-request.ndjson" > "$D/control.json" 2> "$D/control.log"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/trace_summary.py"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/parent_match.py"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/witness_reference.py"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/scalar_times.py"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/motion_formulas.py"
"$D/red.sh"
env BINARY="$S/target-control/release/examples/prototype_basin" "$D/red.sh"
```

The observer replays took 0.007 s and 0.003 s; the interval/reference script took 2.825 s. The watched comparison JSON equals the first representative trace after parsing, with no interventions. The source input hash is `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a`, the retained 12-point request hash is `ebf5cd6fc61b364ac237314b274e7923962d7739db420322df766a4085728db5`, and the observer binaries are `1add446a198f4dbb7538bf243ce02db96fc4a7d1c2b216c9dc6f51c76bc8250c` (comparison) and `c035c6402cbd9a9dcd07a487a35b6317372c6f0fb15a9e02132cdb9258097f05` (control). These are prebuilt local artifacts; rebuild a compatible observer from the retained diagnostics source and use `BINARY=...` when replaying elsewhere.
