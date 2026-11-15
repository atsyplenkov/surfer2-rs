# Review of the first diagnostic checkpoint

Status: preliminary diagnostic evidence, awaiting maintainer discussion. No ticket resolution or production repair.

The replay record and exact commands are in `checkpoint.md`. This note records the main agent's assessment of the separate review, including rejected overstatements.

## Supported findings

The fresh public tests reproduce both full-basin construction failures on the control and the instrumented Arm A comparison. The comparison loop takes 0.303 seconds after compilation. The observer has no requested interventions in these runs. Inspection of the existing library changes found logging, not a repair.

The basin-2 early reduction fails construction on the comparison and returns a connected coordinate graph on the control. This establishes a differential witness, not complete geometric correctness of the control result. The full comparison trace contains a roughly 102-metre three-vertex separation in the early chain, and the reduction repeats that observation. The reduction's eventual DCEL error belongs to another constraint neighborhood, so the current predicate does not establish that this early chain causes that error.

The reported basin-2 reduction retains the original support neighborhood 594–602 and removes the early 321–327 neighborhood. It still fails. Thus the reported failure can occur without that early neighborhood in this altered polygon. This does not prove that the early chain has no effect in the full original basin.

The independent arithmetic at the reported full-basin event places the three-source-support meeting about 1.24880664 microseconds after the observed stop. The stored fast position differs from that reference point by about (-0.5016762, 0.0042353) metres. This is a strong local inconsistency to investigate, conditional on support identity, orientation, active-region membership and event semantics. It is not yet a complete causal explanation of the public error.

The certificate script evaluates four selected events. It does not certify all preceding events or establish a global first invalid step. Its event-order calculations use original moving support lines; the connection to active competing events and predecessor numerical errors needs checking.

## Review claims not adopted

- Different final face numbers or source constraints alone do not invalidate a reduction. Face numbers are observations, not acceptance criteria. The relevant requirement is preservation of the mechanism being investigated, original witness supports and appropriate region/contact topology.
- A reduced control result need not have the same error class as a full control result to be useful. Other full-input failures can disappear during reduction. Cross-revision causal claims must nevertheless account for this change.
- The claimed sub-ULP event gaps are incorrect. Fresh Python `math.ulp` calculations give `3.552713678800501e-15` seconds at the relevant times; gaps of approximately `4.44608e-15` and `5.27287e-15` are about 1.25146 and 1.48418 ULPs respectively. These numbers alone do not prove that exact predicates or co-timed handling are required.
- Removing an early neighborhood while retaining a later failure does not prove complete causal independence in the original polygon.
- The review's assertion that nothing remains unverified is too broad. Engine replay, full ancestry, admissibility and end-to-end causality have different evidence requirements.

## Proposed next bounded investigation

Trace backward through basin 2's reported 11-vertex witness and matching full-input support neighborhood. Verify original support identities and active ancestry before the stop; compare independently computed co-arrival with the stored parent trajectories and the scalar times used to schedule the collapse. Distinguish an already-invalid parent motion, incorrect scalar event prediction, and an invalid all-three-vertices stop. Follow the resulting kinetic arcs and endpoints only far enough to establish or reject their connection to the public construction failure. Keep the separate early-chain witness as a counterexample, without treating it as the cause of the reported failure.

Basin 1 and its contact topology, river witnesses, no-event successors and final export completeness remain unfinished. No production repair, debug/full-suite validation, commit or publication was performed in this checkpoint.
