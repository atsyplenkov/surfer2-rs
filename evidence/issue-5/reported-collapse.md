# Reported basin-2 collapse: bounded diagnostic

**Status:** preliminary, for maintainer review. This investigates the reported support neighborhood, not the globally first invalid event or every cause of either basin failure. No source, existing diagnostic, tracker issue, or production policy was changed. All new files are in this directory.

## Result in brief

On the instrumented Arm A comparison, a three-constraint `TriangleCollapse` stops three finite vertices at `t=649.3054124144818` even though their recorded positions span **0.50169532046 m**. The triangle has the same three original supports, 595, 596, and 601, in the original full basin and the 11-vertex diagnostic reduction. The failed bounded face in both unmodified runs is owned by original edge `[0,601,602]`; the separated endpoints occur on its emitted halfedges.

This is a demonstrated local conflict between the recorded stop and the three-way co-arrival contract. It is **not** yet a proof that all earlier geometry is valid, a complete causal account of the public failure, or a repair design. In particular, the fast successor already fails to replay its own recorded birth, so a standalone scalar-time explanation is insufficient.

One explicitly labelled retiming experiment changed only the time of that queued event. In both full and reduced replays the three stored positions then span about **4.160 mm**, and the watched target face's printed `next`/`prev` links form a complete cycle. Both constructions still fail bounded validation, now on other faces. The full target face's new cycle includes an `InfinityChord` across a separate approximately 7.2 mm gap. The retimed spread also falls below the existing 5 mm metre-scale junction threshold; canonicalization may explain the changed connectivity. This experiment does **not** establish correct geometry, complete exported linework, or a fix.

## Provenance and controls

The source was the existing Arm A worktree at `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` plus the pre-existing observer diff, SHA-256 `5fecc7540bdd7d8ccba02fd93ca58c34997e795169720d55e8fa4f6547638605`. The diff hash was unchanged after this work. The example source hash is `903db527fd688287d6035efaace05025957391fd49322c70c5655f1bcd3d943f`; `src/prototype_diagnostics.rs` is `15628603766bb34058a571e38ec673433408a93da8f9d3581bdfd2dfec89a4fe`; the already-built release example is `1add446a198f4dbb7538bf243ce02db96fc4a7d1c2b216c9dc6f51c76bc8250c`. Its source includes optional intervention hooks; baseline requests did not enable them and returned `"interventions": []`.

The original basin-2 fixture SHA-256 is `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a`. Full and reduced baseline request hashes are `a83443e4af0ac21f5c915d16251f17a457fe833c5946f2794d652c12776b9504` and `7961cae6093b04c078e997def39903f784d420cb0e6696af43adcf9d9e6612e1`. All reduced coordinates match their stated original binary64 vertices; its indices 594 through 602 are retained consecutively. The source-line comparison checked that the stored initial point, direction, normal, and weight of supports 595, 596, and 601 agree between the reduced and full runs. See `provenance.txt` and `results-sha256.txt` for hashes of the inputs, scripts, and results. Rust/Cargo versions and the clean-control public replay are recorded in the preceding `../checkpoint.md`.

The watched baseline JSONs equal the previously reproduced full and reduced baseline JSONs byte-for-byte after parsing. Their example status is `dcel_error`; this is a construction failure, not proof that a returned graph is disconnected. `full-watched.log` and `reduced-watched.log` contain targeted engine logs and face emission. The top-level shell wrapper for those two successful runs and their successful equality assertions finished with exit 1 **afterward**, because its final `tail -1` command rejected multiple operands. That reporting mistake did not alter the observer outputs.

## Ancestry and independently computed line checks

The numbers below are conditional on the trace's identified and oriented source supports. The existing `certify.py` functions use exact dyadic inputs, rational interval arithmetic, and enclosed square roots; their implementation and the active-region admissibility of every earlier event have not been independently certified. Calculations against infinite moving source lines are distinct from proving that a finite wavefront event is enabled.

- **Full-input fast successor.** Step 2769 is a `SplitOrFlipRefine` followed by creation of the successor on original supports 595 and 601. Its split parent has original supports **595 and 594**, and the hit edge is original support 601. The watch log reports `tparam=0.4999988570893514`, inside the stored finite edge. The recorded split is at `649.3052873722798`; source-line arrival for the parent support pair against 601 is enclosed at `649.30528733131290608687..649.30528733131290608688`, a difference of about 40.97 ns. This comparison is local and does not by itself certify the split topology.
- The fast successor is finite (`infinite_speed=None`), labelled `Degenerate` by the split-successor motion classifier, and has velocity approximately `(405055.6550, -3419.3763)` metres per time unit. Its stored trajectory at birth differs from its **recorded** birth by approximately `(0.020753922, -0.000174981)` m. Independent source-support intersection at that time is approximately `(0.016593929, -0.000140123)` m from the recorded birth in the opposite direction; the stored trajectory is approximately `(0.004159993, -0.000034858)` m from that source intersection. The small signed support residuals and the much larger along-intersection displacement illustrate the conditioning problem. These comparisons do not authorize simply pinning motion to the recorded birth: doing so could move it farther from the source-line intersection.
- The slower successor on supports 601/596 was created by the earlier full-input split at `371.0291729650514`; its source-line parent arrival is enclosed near `371.029172961810969`. At the reported stop its stored position differs from its source-line intersection by approximately `(1.10e-11, -1.79e-10)` m. The third vertex, initially on supports 596/595, differs from its source-line intersection by approximately `(-2.105e-8, 5.874e-8)` m at the stop. These are local comparisons, not a full preceding-event proof.
- **Important reduction limit.** The 11-vertex reduction preserves the final 595/596/601 support triple, the scalar stop time, and the 0.501695 m spread, but **does not preserve the full birth ancestry**. Its fast successor is born at `649.3054058825634` when a parent using original support 601 and the synthetic edge `[602,1143]` hits support 595. Its recorded-birth replay miss is approximately 1.621 cm. Its parent is not the full-input parent on 595/594. `predecessors.json` explicitly identifies which reduced edges are synthetic.

`source-comparison.json` records per-vertex motion errors, birth replay, and signed line residuals. `predecessors.json` records the two selected split predecessors in each replay, their source IDs, and source-line arrival intervals. In both runs the successor support records and triangle contents are from the observer; the independent arithmetic checks the consequences of those identities.

## Selection, stop, and emitted face

For the full triangle the watch log records scalar edge times:

```text
support 601: 649.3054136530184
support 596: 649.3054124144818  <- selected minimum
support 595: 649.3054136530183
```

The three-constraint branch in `compute_collapse` selects the minimum available scalar and labels the result `TriangleCollapse`. `constraint_collapse_time` calls `compute_edge_collapse_f64`, whose nonvertical path uses an x-coordinate quotient. The three finite vertices are then stopped at the selected time. This code path and the recorded separation establish why the all-three stop cannot be justified solely by selecting the minimum of the three scalar estimates. They do not establish that changing just the scalar rule would correct earlier motion, support selection, or any other case.

At the baseline stop the full face-601 trace has halfedges ending near `(463811.7052512257, 6272413.62681912)` and `(463811.2035737932, 6272413.631053105)`. The watched raw and expanded arcs show those endpoints as stopped traces of the slow and fast vertices; face 600 (source 601) has `root=None` and owns 10 halfedges. The reduced trace has the same stop-position separation and source 601 on its failing face 5, though its fast arc starts at the different reduced split birth.

There is also an earlier three-constraint stop on source supports 597, 600, and 601: full step 2196 at `371.02917302656596`, with recorded spread `0.007223154832221385` m. Under the same source-line assumptions, their three-line meeting is enclosed near `371.029172963128721`. This is another local observation, not a globally first-event claim. `prior-triangle-summary.log` records the vertices and calculation. Its gap is relevant to the retimed face below.

## Controlled retiming, not a repair

The existing example's `force_event` hook was used to replace the time of the already-selected 595/596/601 `TriangleCollapse` with `649.3054136632884`, the binary64 representation close to the source-line reference time. It did not request a different event type, support, velocity, or position. The hook reports exactly one intervention in each run; all printed events before the selected event compare equal to baseline. Because the queue and later propagation run after an artificial time change, this is an experiment about dependence on that event, not evidence for an admissible production policy.

| Replay | Baseline | After retiming |
| --- | --- | --- |
| Reduced | spread `0.50169532046` m; source-601 face 5 has no root; construction fails there | spread `0.00416013229` m; face 5 has root and all 7 printed halfedges form one reciprocal cycle; construction fails at face 6, halfedge 39 unreachable |
| Full | same spread; source-601 face 600 has no root and owns 10 halfedges | same reduced spread; face 600 has root and all 11 printed halfedges form one reciprocal cycle; construction fails at face 939, source `[0,927,928]` |

`audit_face_cycles.py` follows the printed next links and checks reciprocal prev links of the watched face only; `face-cycles-summary.log` records the counts. It does **not** validate the entire DCEL or prove correct geometric ownership. In the retimed full run its 11th halfedge is `InfinityChord` 20067, which joins the earlier approximately 7.2 mm separation from the 371.029173 event. The event's new 4.16 mm spread is below the existing metre-scale 5 mm `same_stop` threshold; this may contribute to endpoint canonicalization. An `InfinityChord` is a topological closure and is not independent evidence of a real finite skeleton branch. The public constructor still returns an error in both interventions, so exported finite-segment connectedness is not established.

This experiment supplies a causal **sensitivity**: perturbing this one queued time changes the watched face's emitted cycle and moves the first reported validation failure. It does not prove that retiming is the unique or correct cause of the baseline error, that the reference event is physically admissible, or that a cycle closed by canonicalization/chords represents complete geometry.

## Reproduction commands

From `/tmp/opencode/surfer2-first-invalid`:

```sh
S="$PWD/evidence/issue-5/session-resume"
D="$S/reported-collapse"
# Fresh non-intervened watched runs; these produce the same parsed JSON as the
# previous checkpoint's full and reported-reduced baselines.
env SURFER2_PROTOTYPE_EDGES=1,2,7 \
  "$S/target/release/examples/prototype_basin" \
  < "$S/basin-2-reported-reduced-request.ndjson" > "$D/reduced-watched.json" 2> "$D/reduced-watched.log"
env SURFER2_PROTOTYPE_EDGES=595,596,601 \
  "$S/target/release/examples/prototype_basin" \
  < "$S/basin-2-full-request.ndjson" > "$D/full-watched.json" 2> "$D/full-watched.log"

# Independent source-line and predecessor calculations.
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/source_comparison.py" > "$D/source-comparison.log"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/predecessors.py" > "$D/predecessors.log"

# The retained requests add only force_event={constraints: the three original
# edge identities, time: 649.3054136632884}; inspect their hashes above.
env SURFER2_PROTOTYPE_EDGES=1,2,7 \
  "$S/target/release/examples/prototype_basin" \
  < "$D/reduced-retime-request.ndjson" > "$D/reduced-retime.json" 2> "$D/reduced-retime.log"
env SURFER2_PROTOTYPE_EDGES=595,596,601 \
  "$S/target/release/examples/prototype_basin" \
  < "$D/full-retime-request.ndjson" > "$D/full-retime.json" 2> "$D/full-retime.log"
env PYTHONDONTWRITEBYTECODE=1 python3 "$D/audit_face_cycles.py"
```

The shell's `time` measured the full watched run at 0.838 s, retimed reduced run at 0.006 s, and retimed full run at 0.861 s. Source comparison took 1.861 s and predecessor checks 1.377 s. Both observer runs returned process exit 0; their JSON `status` recorded construction failure. `retime-summary.txt` records the interventions and first changed events.

## Next question and untested scope

Can a smaller basin-2 witness retain the **full-input split parent on original supports 594/595, the hit on original support 601, the active-edge arrangement, the final 595/596/601 stop, and the face-601 failure** simultaneously? The present 11-vertex reduction does not satisfy the first condition. Such a witness could distinguish inherited parent motion, event-time prediction, three-way stop semantics, and face/export effects without treating a synthetic predecessor as equivalent.

Further work must validate active finite-edge topology and numerical error budgets, trace prior local events as needed, and compare control against the same justified witness. No assertion is made here about the global first invalid event, all basin mechanisms, the river, point-contact topology, or the original full acceptance criteria. The ticket remains open.
