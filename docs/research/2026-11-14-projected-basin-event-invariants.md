# Upstream successor and collapse-event invariants

2026-11-14 · Research for [ticket #4](https://github.com/atsyplenkov/surfer2-rs/issues/4), under [map #3](https://github.com/atsyplenkov/surfer2-rs/issues/3).

## Answer

The upstream contract couples **support incidence, birth replay, velocity, and event time**. A finite successor must follow the intersection of its two moving supports and replay its supplied birth. An ordinary edge collapse means its two endpoints meet; stopping a whole finite triangle requires all three vertices to meet. Upstream's three-constraint shortcut asserts agreement of all edge-event specifications, rather than selecting the earliest disagreeing prediction. Genuine infinite-speed vertices are a separate, same-time lifecycle associated with exact parallel configurations. Neither large finite speed nor poor conditioning establishes that lifecycle. [U-support], [U-motion], [U-life], [U-edge], [U-collapse], [U-triangle-handler]

Split and constraint-collapse children share the same upstream motion constructor, but inherit different support pairs and ancestry links. There is no upstream angle-threshold/`Never` override table for split leftovers in the inspected path. This supplies checks for a local witness, not a basin-versus-river ancestry classifier. The direct-normal velocity formula is algebraically consistent with the upstream moving-support model; reported regressions from combined substitutions do not disprove that identity. [U-split], [U-constraint-handler], [U-motion]; independent derivation below.

## Evidence boundary and revisions

- **Upstream:** `cgalab/surfer2@c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`, the revision declared by the [Rust README][R-readme]. All `U-*` citations below pin this revision.
- **Published Rust base:** `244944daf2f0fc2ba9e21b412e424257a1574800`. All `R-*` web citations pin this revision.
- **Local control:** `d1a89fd66795f537b034f220ed4f838bd0fdea3b`.
- **Local comparison, Arm A:** `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c`. It is an experiment, not a selected repair. Unpublished-source evidence is identified in the appendix; no unavailable GitHub source URLs are implied.
- **Numerical dependency inspected:** CGAL 5.0, peeled commit `bec70a6d52d8aacb0b3d82a7b4edc3caa899184b`, including its bundled CORE headers. Upstream requests `CGAL COMPONENTS Core` **without a version lock**. This dependency revision documents the relevant contracts and implementation branches; it is not evidence of the version used in any historical or local Surfer2 executable. [U-deps]
- **Reported observations:** `/tmp/handoff-surfer2-issue-1-projected-basins.md`, SHA-256 `0cade0502eae4d842a377245841287fd143e7f7b850df363b06a5164bd4ec9e6`. Its birth miss, premature three-vertex stop, `Never` successor, river counterexamples, and patch regressions were not freshly reproduced here.

Assignment to `atsyplenkov` was verified before research. Applicable worktree/ancestor `AGENTS.md` files and repository glossaries/ADRs were searched for; none were found. The `research`, `diagnosing-bugs`, `domain-modeling`, `gh`, and `commit` skills were consulted. The diagnosis/reproduction phases were deliberately not entered: this ticket asks for source-contract research, and the assigned work excludes engine tests and repair experiments. Definitions remain in this single note.

## Established facts, assumptions, and unresolved questions

| Topic | Established source fact | Assumption or limitation | What still needs a local witness |
|---|---|---|---|
| Finite birth | `make_vertex` asserts `p_at(time) == pos`; `p_at(t) = pos_zero + velocity*t`. [U-motion], [U-life] | The chosen supports and supplied event position must be mutually consistent. Replaying an incorrect birth alone is insufficient. | Which of support selection, birth time/position, origin, or velocity first becomes inconsistent? |
| Moving supports | A support translates by its weighted unit normal; nonstraight velocity is intersection at time 1 minus the time-zero intersection. [U-support], [U-motion] | Normalization and intersection constructions participate in the contract, not just orientation predicates. | Do both support residuals and their time derivatives vanish for the suspect trajectory? |
| Fast versus infinite | Only a straight support sector enters the opposing/unequal-weight infinite-speed classification. [U-motion] | In the exact kernel, “straight” is exact collinearity. A double build evaluates the same predicates in floating arithmetic. | Is the represented support pair actually parallel, merely ill-conditioned, or numerically corrupted? |
| Edge event | The endpoint relative velocity is assumed collinear with the edge; the quotient may use any nonorthogonal projection. [U-edge] | Both trajectories must remain on their common moving support. | Does the selected scalar event time give a planar meeting, and where was collinearity lost? |
| Triangle event | Three constrained edges must have equal collapse specifications; the handler asserts equal positions for all three finite vertices. [U-collapse], [U-triangle-handler] | This shortcut assumes a valid kinetic triangle with coherent motion; an area-zero event alone is weaker. | Is the observed disagreement rounding in a valid state or evidence of an earlier invalid state? |
| Simultaneity | Queue order is component, time, event type, then descending secondary key where applicable. Pending topology changes are processed before the next event. [U-events], [U-queue], [U-propagate] | Equality is equality in `NT`; two numerically close times are not automatically simultaneous. | What exact event identities and order are required at the first disputed time? |
| `Never` | It can mean no collapse for the current trajectories, or an event that should be preempted by another topology change; caches are invalidated when state changes. [U-one-constraint], [U-no-event], [U-cache] | It is not a universal instruction to delete or freeze a successor. | Was `Never` valid, miscomputed, or stale? Which pending event should have invalidated it? |
| Child ancestry | Split copies retain the hit edge's supporting-line object and face; a constraint collapse combines the two surviving outer edges. [U-edge-split], [U-split], [U-constraint-handler] | Both use the same birth/motion requirements. Provenance is useful evidence, not an established classifier. | Do paired basin and river witnesses differ in actual support/endpoint ancestry or only in geometry and scheduling? |

## 1. Vocabulary that matters here

- A **support** is the oriented, weighted infinite line backing a propagating wavefront edge. A **wavefront edge** is the current finite piece between kinetic endpoints; splitting that piece does not create a new underlying moving line. [U-support], [U-edge-split]
- A **spoke** is an unconstrained triangulation edge; its collapse is not the same as retirement of an input-derived wavefront constraint. The upstream spoke handler can reuse the second half of the constraint-collapse machinery. [U-events], [U-spoke-handler], [U-constraint-handler]
- A **birth** is `(pos_start, time_start)`. The **trajectory origin** is `pos_zero`, extrapolated to absolute time zero, not necessarily the birth position. [U-life]
- Upstream `is_degenerate` describes a trace whose stop position equals its start; for a finite vertex, upstream also asserts equal start/stop times in that case. Rust `SuccessorMotion::Degenerate` instead describes a poorly conditioned/non-finite support-intersection calculation. These are different concepts. An “infinite vertex” of an unbounded triangulation is also different from a vertex with **infinite speed**. [U-life], [U-unbounded], [R-split-policy]

## 2. Birth, support, origin, and velocity

### Source contract

`WavefrontSupportingLine` constructs the left unit normal from the oriented line direction and multiplies it by `weight`; `line_at(t)` translates the original support by `t * normal`. Thus its weight is the support's normal speed. [U-support]

For a nonparallel pair, `make_vertex` stores the intersection of the two **time-zero** lines as `pos_zero`. `compute_velocity` intersects their time-one translations and subtracts `pos_zero`. For coincident time-zero lines (`ALL`), it back-projects the supplied birth using the straight-sector velocity. For distinct parallel lines (`NONE`), it stores the birth as origin and asserts that this case corresponds to an infinite-speed vertex. Finally, it asserts birth replay. This is not permission to supply arbitrary disjoint, same-direction supports: valid ancestry and the event geometry are preconditions. The straight finite-motion branch requires equal normals; unequal normals there produce an explicit unimplemented-path failure. [U-motion]

Initial finite vertices additionally assert membership on both original supports. Later finite support incidence follows algebraically from the same two moving supports and affine motion; the inspected successor constructor does not separately assert both incidences at every time. Finite `stop(t)` computes its own trajectory point. Only the infinite-speed overload accepts an externally supplied stopping point, and it asserts `time_stop == time_start`. [U-life]

### Independent derivation

This is a mathematical derivation of the source model, not a proposed floating-point implementation.

Let `n_i` be a unit normal, `c_i = n_i · a_i` the time-zero support offset, and `w_i` its weight. The two moving supports satisfy

```text
n_1 · p(t) = c_1 + w_1 t
n_2 · p(t) = c_2 + w_2 t.
```

For nonzero determinant of the two normal rows, write `p(t) = p_0 + t v`. Then

```text
N p_0 = c
N v   = w
p_b   = p_0 + t_b v.
```

Consequently:

1. `p(1) - p(0)` and solving `N v = w` are identical in exact arithmetic.
2. Intersecting the lines through `w_1 n_1` and `w_2 n_2`, in the original support directions, solves exactly that velocity system.
3. Translating both supports by a common vector changes `p_0` by that vector and leaves `v` unchanged.
4. Reanchoring with `p_0 = p_b - t_b v` enforces birth replay algebraically, but preserves support incidence only if `p_b` lies on both supports at `t_b`.

If a reconstructed trajectory has errors `δp_0` and `δv`, its residual on support `i` is `n_i · δp_0 + t n_i · δv`. Forcing one birth point does not eliminate a wrong support-normal velocity. Conversely, an improved velocity paired with an inconsistent origin can still miss the birth. In binary64, even back-projection followed by replay need not be bit-exact. These observations explain why origin, velocity, birth, and support checks belong together; they do not select an arithmetic policy.

### A finite near-parallel example

Choose `q = 1/20,000,000`, unit weights, and time-zero supports through the origin with unit normals

```text
n_1 = (0, 1)
n_2 = (-2q/(1+q²), -(1-q²)/(1+q²)).
```

The second normal is exactly unit length because `4q² + (1-q²)² = (1+q²)²`. Their determinant is `2q/(1+q²)`, nonzero and about `10^-7`. The velocity equations give

```text
v_y = 1
-2q v_x - (1-q²) = 1+q²
v_x = -1/q = -20,000,000.
```

This is an exactly finite velocity of magnitude greater than `10^6`, despite an opposing, extremely ill-conditioned pair. At `q = 0`, the equations instead demand both `v_y = 1` and `-v_y = 1`: there is no finite solution. The limiting singular configuration and every nonzero member of this family must not be conflated. This example is independent of the basin fixtures and supplies no claim about their correct event times.

### Exact parallel and weighted cases

Upstream first tests the orientation of the two support directions. Only `STRAIGHT` enters these cases: [U-motion]

| Geometry at a valid contact | Upstream interpretation |
|---|---|
| Nonparallel directions, however small the nonzero angle | Ordinary finite-intersection construction; no speed cutoff. |
| Same-direction parallel supports, equal weights, coincident where the vertex exists | Common-normal finite motion. Distinct equally moving lines do not acquire an intersection merely because their directions agree. |
| Opposing parallel supports that have collided | `InfiniteSpeedType::OPPOSING`; a same-time event resolves the collapsed strip/segment configuration. |
| Same-direction parallel supports of different weights that have become collinear | `InfiniteSpeedType::WEIGHTED`; the faster incident edge wins in the configured upstream convention. |

Infinite-speed vertices store a null velocity as a representation device, and their position polynomials are set to zero. They are detected before ordinary bounded/unbounded event prediction and scheduled at `time_now`. They must not be interpreted as stationary finite vertices allowed to persist for later times. The opposing handler may flip away spokes, consume an incident edge, or collapse a fully constrained face. The weighted handler retires a slower incident edge; candidate selection only considers constraints incident to a weighted infinite-speed vertex. [U-motion], [U-inf-predict], [U-inf-handler], [U-weight-policy]

The inspected CGAL line-intersection implementation has an important numerical caveat: zero determinant distinguishes coincident/distinct parallel lines, but non-finite numerators or constructed coordinates can also return “no intersection.” Thus a failed floating-point construction is not proof of exact parallelism. Upstream's constructor assertions express its expected valid state; the double dependency path does not establish that every returned `NONE` satisfies it. [C-intersection], [U-motion]

## 3. Edge collapse, whole-triangle collapse, and time

### What an edge event establishes

Upstream classifies endpoint velocity orientation as follows: right turn → `PAST`; collinear → `ALWAYS` if the time-zero positions coincide, otherwise `NEVER`; left turn → a collapse quotient, asserted to be now or in the future. The source explicitly treats coincident-but-separating endpoints as not collapsing. These rules presume oriented endpoints on their common translating support. [U-edge]

Its derivation states that `v_0 - v_1` is collinear with the edge. With `d = p_1(0) - p_0(0)`,

```text
t = (d · d) / ((v_0 - v_1) · d)
  = d_x / (v_0x - v_1x)       if the support is not vertical
  = d_y / (v_0y - v_1y)       otherwise.
```

The x quotient is therefore a consequence of planar support consistency, not a separate definition of a valid meeting. If numerical trajectories leave the common support, equal x coordinates no longer establish an edge collapse. A planar closest-approach time likewise does not by itself re-establish support incidence or event ancestry. The finite constraint handler stops both endpoints on their own trajectories and asserts that their positions coincide. [U-edge], [U-constraint-handler]

### When all three may stop

- **Three constraints, finite vertices:** upstream obtains one edge collapse and asserts equality with each of the other two, including event type/time; it then emits `TRIANGLE_COLLAPSE`. There is no minimum-of-disagreeing-times or “none found, use now” branch. [U-collapse]
- **Two constraints:** equal finite constraint events become a triangle collapse; otherwise it chooses the earlier available edge event. The pinned source contains a duplicated `c1.type() == NEVER` test at lines 721/723; this is visible source awkwardness, not evidence for a negative-area fallback. Its `std::min` and `Never` ordering still handle a finite `c1` with `c2 == Never`. [U-collapse], [U-events]
- **Zero constraints:** at an area-polynomial root, three zero-length sides imply triangle collapse, one implies spoke collapse, and no zero side implies a flip across the longest spoke. Collinearity alone does not imply a point collapse. [U-generic]
- **Spoke-collapse propagation:** the opposite finite vertex is stopped with the pair only if it is at the collapse position. Otherwise the surviving supports form a successor; there is a separate fully constrained infinite-speed case. [U-spoke-handler]

The ordinary triangle handler stops all three finite vertices and asserts equal trajectory and stop positions. Infinite-speed faces go through distinct handlers. Thus “three constraints” is a shortcut justified by the model's invariants, not a license to stop three inconsistent trajectories independently. [U-triangle-handler], [U-inf-predict], [U-inf-handler]

### Simultaneous events and `Never`

`CollapseSpec` sorts `NEVER` after every real event, then real events by component, time, enum type, and descending secondary key when needed. At equal time the relevant type order is opposing-infinite, triangle, constraint, spoke, split/refine, weighted-infinite, flip, convex-hull departure, invalid. Relevant edge indices and triangle IDs do not break equality in this comparator. A claim that upstream defines one unique total order between otherwise equal triangle events would go beyond this source. [U-events]

Propagation handles one queue event, processes drops and recomputations, then examines the next event. Same-time events can therefore arise from a preceding event's changed topology; they are not simply a frozen batch. The refinement handler can replace a potential split with a spoke or flip at the **same time** and return it to the queue so priorities are respected. [U-propagate], [U-queue], [U-refine]

For a one-constraint triangle, upstream distinguishes a constraint collapse from the opposite vertex hitting its moving supporting line. It computes that line-hit time directly. Future hits are candidates; hits exactly now are screened using area-polynomial degree/sign and, for quadratics, the derivative; past hits are suppressed. `ALWAYS` on the line produces a now-time refinement, while `NEVER` on the line remains no event. If the constrained endpoints are parallel, `ALWAYS` collapses now; otherwise only the appropriate linear-area case is investigated for split/flip. [U-one-constraint], [U-line-hit]

For positive-leading-coefficient quadratics with distinct roots, the second root corresponds to returning from negative area and is rejected as the next collapse. A valid/nonnegative triangle at the current time is an explicitly documented precondition. A double root is accepted as an edge collapse but rejected as a mere tangential line-hit in the relevant acceptance branch. [U-accept], [U-generic-time]

`Never` is a cached prediction about the current state, not permanent immunity from events. Vertex/constraint changes invalidate predictions, modified faces are queued for recomputation, and the simulation finishes when no real event remains. Debug `event_that_will_not_happen` can install an `INVALID_EVENT` sentinel at a generic root to expose a missing preempting topology change; a normal build may return `NEVER` instead. There is no general upstream rule “ill-conditioned child + `Never` → collapse now.” [U-cache], [U-modified], [U-no-event], [U-finish], [U-split]

## 4. Split leftovers and constraint-collapse children

The upstream differences concern **which constraints survive and how the face traces connect**, not different finite-motion laws:

| Operation | Support and ancestry construction | Subsequent lifecycle |
|---|---|---|
| Split | Stop the reflex parent; duplicate the hit edge into two pieces sharing its immutable support and skeleton face. Form children `(nea, ea)` and `(eb, neb)`, each combining one hit-edge copy with one parent incident edge. Link the siblings tail-to-tail along the split face and link the parent's two outgoing sides to the respective children. [U-edge-split], [U-split], [U-links] | Rewire both incident fans, install the two split constraints in the neighboring faces, mark affected faces modified, and drop the original triangle. Their events are then recomputed. No explicit split-leftover angle/constraint-count override appears in this handler. [U-split], [U-modified], [U-queue] |
| Constraint collapse | Stop the two endpoints at the common event point. Form one child from `va.incident(0)` and `vb.incident(1)`, the surviving **outer** edges; link each surviving parent side to that child. Connect the parents head-to-head on the disappearing constraint side. [U-constraint-handler] | Transfer applicable constraints, replace endpoints in affected fans, reconnect neighboring triangles, retire the collapsed edge and triangle, and recompute affected predictions. [U-constraint-handler] |
| Spoke collapse | Stop the meeting pair; process each side. A side that is not a complete point collapse can reuse the constraint-collapse second half even though the collapsed spoke has no wavefront edge. [U-spoke-handler] | Completion depends on the opposite vertex and surviving topology, including explicit infinite-speed handling. |

For both child constructors, `from_split` only relaxes the requirement that both selected incident edge endpoints have already stopped. It does **not** bypass the intersection classification or birth-replay assertion. Replacement of a vertex's incident edge with a split copy asserts identity of the underlying supporting-line object. [U-motion], [U-life]

This suggests precise evidence to retain in the existing first-invalid-event investigation: event kind/time; ordered parent and child identities; incident endpoint slots; original support/face identities and split-copy lineage; weights and oriented directions; birth/origin/velocity; the two support residuals; triangle constraints and opposite vertex; and cache invalidation/recomputation history. This is a list of witness facts, not a production API design.

The upstream evidence does not establish that “constraint-collapse child” means “remove the corridor,” or that a `Degenerate` support pair is invalid. A finite child may legitimately live for a positive interval; an exactly infinite-speed child must be resolved at its birth time. A valid same-time finite child can also have zero duration because another genuine event follows immediately. Distance to an opposite vertex is not an ancestry invariant in these handlers. [U-life], [U-motion], [U-split], [U-constraint-handler], [U-inf-handler]

## 5. Numerical dependencies and exactness assumptions

Surfer2 defaults `NT_USE_DOUBLE` to `OFF`. Its active default kernel alias is `Exact_predicates_exact_constructions_kernel_with_sqrt`; the double branch actually selects `Simple_cartesian<double>`. The alternative exact-predicate/inexact-construction alias is inside `#if 0`, so attributing filtered exact predicates to the enabled double build would be wrong. [U-build], [U-kernel]

At the inspected CGAL revision, the exact-with-sqrt kernel aliases `Simple_cartesian<CORE::Expr>` when CORE is selected, or the LEDA real-number variant when LEDA is selected. Its official contract promises exact predicates **and constructions**, with a scalar field supporting square roots. CORE's CGAL traits declare exact arithmetic and dispatch square root, sign, and comparison to CORE. CORE expression comparisons obtain the sign of a difference; square root creates an expression node. Surfer2's `NT::getZero/getOne`, `Rep()`, and sign diagnostics additionally reveal its CORE-specific expectations. [C-kernel], [C-kernel-doc], [C-traits], [C-expr], [U-kernel], [U-heap-check]

Exactness matters beyond a static `orient2d` call: support normalization takes a square root, intersection constructs origins and velocities, determinant coefficients feed root selection, and equality/ordering governs coincident events and endpoint meetings. Upstream directly compares polynomial coefficients and the quadratic discriminant to zero and constructs roots using `CGAL::sqrt`. It also checks nonnegative kinetic triangle orientation and contains diagnostics comparing CORE's ordinary and exact sign paths. These are source contracts and defensive checks, not proof that every build/dependency combination is correct. [U-support], [U-motion], [U-quadratic], [U-polynomial], [U-valid], [U-heap-check]

The Rust port exposes a robust static orientation helper, but the inspected supporting-line determinant, velocity-orientation test, angle threshold, and event polynomial computations operate directly on `f64`. The existence of that helper does not confer exactness on these separate decisions. [R-geometry], [R-support], [R-edge], [R-vertex], [R-poly]

No upstream executable or dependency build was run here. In particular, this note does not establish that `NT_USE_DOUBLE` handles either original basin, identify the CGAL version behind frozen oracle artifacts, or establish applicability of other skeleton engines. Oracle/input-domain methodology belongs to [ticket #7](https://github.com/atsyplenkov/surfer2-rs/issues/7).

## 6. Concrete Rust divergences from code inspection

These are differences demonstrated by source, not a ranking of causes of the basin failures. Except where stated, the cited Rust files have identical Git blob IDs at the published base, local control, and Arm A.

| ID | Demonstrated difference | Consequence or limit of the finding |
|---|---|---|
| D1 | Control `intersection_velocity` returns `None` for non-finite velocity **or speed above `1e6`**. `wavefront_vertex_motion` can then return zero velocity for opposing directions or a common-normal fallback for same-direction ones. Upstream ordinary finite construction has no such speed cap. [R-support], [U-motion] | A finite fast pair can enter a missing-intersection fallback. Arm A changes this policy; see the local evidence appendix. The algebraic example above refutes speed magnitude as a singularity criterion. |
| D2 | `vertex_angle_from_lines` calls directions straight when `abs(cross) <= 1e-14*scale`. Independently, split `classify_successor_motion` uses sine thresholds `1e-4` and `1e-6`; a finite opposing intersection below `1e-6` can become `NonIntersecting`, and a computed zero velocity also receives that label. This classifier still computes velocity by time-one minus time-zero intersections in Arm A. [R-vertex], [R-split-policy] | Neither `Degenerate` nor `NonIntersecting` is upstream's exact geometric classification. Weights are omitted from the conditioning thresholds; same-direction unequal-weight infinite-speed semantics are handled separately by the earlier infinite-speed branch. |
| D3 | Rust stores the event birth in `pos_start` separately from the origin/velocity and has no matching birth-replay assertion in these child constructors. `position_at` returns the stored stop at and after stop time, whereas upstream kinetic `p_at` evaluates the affine trajectory and asserts no query later than stopping. [R-vertex], [R-child], [R-split-child], [U-motion], [U-life] | A stored birth/stop can disagree with the extrapolated trajectory. A witness must inspect the affine motion itself, not only the returned stopped position. |
| D4 | In `compute_edge_collapse_f64`, `EPS = 0.0` and the outer branch is `o < EPS`; its nested `o < 0.0` immediately returns. The following collinear block is therefore unreachable for finite `o`. Upstream uses `o != LEFT_TURN`. Rust also infers “vertical” from both x deltas being zero, rather than the supporting line. [R-edge], [U-edge] | Coincident equal-velocity endpoints can return `Never` instead of `Always`. This is a sharp control-flow discrepancy, independent of precision; basin reachability is unproved. The vertical quotient also depends on stored motion consistency. |
| D5 | A finite three-constraint Rust triangle uses the minimum available edge time, or `time_now` if none is finite. Two constraints use a relative `1e-15` time-tie threshold, and can emit triangle collapse from negative signed area when no edge event was found. The triangle handler stops all three and uses `same_stop`/child handling rather than the upstream three-way position assertion. [R-collapse], [R-triangle-handler], [U-collapse], [U-triangle-handler] | Earliest-pair arrival and negative area do not independently establish whole-triangle co-arrival. This matches a question raised by the handoff, not proof of its earliest cause. |
| D6 | Rust's split-only leftover table can zero a child's velocity and force a same-time constraint collapse for a two-constraint `NonIntersecting` child, or a two-constraint `Degenerate` child whose natural event is `Never`. Three-constraint `NonIntersecting` cases can emit an output-only spoke. One-constraint cases are `Unsupported`. [R-split-policy], [R-leftover] | This policy has no equivalent in the inspected upstream split handler. Constraint-collapse children never call this table; describing them using a retrospectively computed `Degenerate` label does not show they took this path. [R-child], [U-split] |
| D7 | Rust constraint-child support selection tries the preferred incident slot, then the other live non-collapsing edge; it may fall back to saved triangle-side supports or create zero motion without a support pair. Upstream takes the exact ordered outer edges and asserts stopped/equal parents. [R-child], [U-constraint-handler] | A support-provenance witness must record the actual branch and endpoint slots. The existence of fallback code does not establish that it is taken in either basin. |
| D8 | Rust's one-constraint split/flip fallback calls `det.next_root_from(now)` rather than upstream's direct supporting-line hit plus sign checks. That helper can select the second quadratic root when the first is past, or an increasing linear root at `now`. Rust also has an explicit recent-on-segment `1e-4 s` catch-up path. [R-one-constraint], [R-poly], [U-line-hit], [U-accept] | These are event-selection differences, not proof of reachability under upstream's nonnegative-area precondition. Whether a residual `Never` or late event passes through them requires a witness. The control's determinant stabilization does not change these root-selection routines. |
| D9 | Generic Rust collapse classification treats lengths `<= 1e-9*(1+max_length)` as zero and promotes two zero sides to three; upstream tests exact zero squared lengths. Queue key priority otherwise retains the source enum ordering and numerical equality, without a near-time merge. [R-generic], [R-event], [U-generic], [U-events] | Small-side classification, three-way co-arrival, and queue simultaneity are separate decisions; improving one calculation can change downstream classifications/order. |
| D10 | For a weighted infinite-speed face, Rust picks the maximum-weight constraint among all triangle constraints. Upstream first excludes constraints not incident to a weighted infinite-speed vertex. [R-inf], [U-inf-predict] | This is a concrete weighted-path difference worth retaining in regression reasoning; no relevance to the reported unit-weight basin failure is established here. |

### D4: a minimal code-level counterexample

Take `p_0 = p_1 = (0,0)`, `v_0 = v_1 = (0,1)`, and `now = 0`. These are coincident endpoint motions on the upward-moving support `y = t`.

Upstream sees collinear velocities and zero squared origin distance, hence `ALWAYS`. In the inspected Rust function, `o = 0` fails `o < 0`; both x deltas vanish, so it chooses y; the y relative velocity is also zero, so it returns `NEVER`. This conclusion follows by substitution into the two source functions, without running or adding an engine test. [U-edge], [R-edge]

Rust's one-constraint predictor has a later `leftover_constraint_already_a_point` workaround for equal velocities and equal current positions, which can mask this discrepancy on that path. The shared edge helper itself still cannot emit `Always` via its intended collinear branch, and other callers do not acquire that workaround automatically. [R-one-constraint], [R-edge-time]

## 7. Decisions still requiring local evidence

The source-backed reference question is answered. The following are precise inputs to the already-open [first-invalid-event investigation (#5)](https://github.com/atsyplenkov/surfer2-rs/issues/5), not repair decisions:

1. **First violated invariant:** at the earliest disputed birth, do the selected supports and event point/time agree independently? If yes, is the first error in origin construction, velocity construction, replay, or subsequent scheduling? If no, which parent event or support-selection branch supplied the inconsistent tuple?
2. **Whole-triangle retirement:** do the three edge predictions describe one genuine co-arrival, or is a pair event being promoted to a whole-triangle stop? Record the third trajectory and support incidences before any stored stop overrides them.
3. **Meaning of `Never`:** which exact branch produced it—parallel endpoints, supporting-line no-hit, area/root rejection, convex/preempted event, or cache reuse? Did a required neighboring update occur? Separately record any Rust leftover override.
4. **Provenance claim:** can basin and river children be paired by actual construction kind, ordered surviving supports, split lineage, and event context? Does a proposed distinction survive that pairing? Neither the handoff nor upstream source has demonstrated a separating ancestry predicate.
5. **Numerical-policy choice:** after the witness identifies the invalid decision, which representation/predicate policy preserves birth, supports, co-arrival, and ordering together? The equivalence of velocity formulas supplies a consistency relation, not an endorsement of Arm A's thresholds or a global replacement.

One **new, independently sharp ticket candidate** emerged: “Establish the intended `ALWAYS` classification for coincident collinear edge endpoints and audit its callers.” D4 gives an exact local contract discrepancy and a minimal algebraic input. The decision is whether this warrants a separate witness/regression task or belongs within #5; it is not yet an explanation for a basin failure. No tracker changes were made.

Remaining fog: the first causal defect in each full basin, whether residuals share a cause, whether any reported symptom is candidate-induced, and whether a provenance-based separation exists. The handoff's failed broad substitutions and distance cutoffs are observations about combined executions, not proofs against the direct-normal identity or for an ancestry classifier.

## Appendix: provenance, local-only source evidence, and checks

### Reproduction of the source inspection

The research branch starts from published `244944d`; the main checkout and Grok experiments were not modified. Upstream was cloned separately at `/tmp/opencode/surfer2-upstream-invariants`, detached at the pinned commit, and remained clean. No source patches were applied in either inspected engine.

Principal commands:

```sh
gh issue view 4 --repo atsyplenkov/surfer2-rs --json number,title,assignees,body,url,comments
gh issue view 3 --repo atsyplenkov/surfer2-rs --json number,title,assignees,body,url,comments
gh api repos/atsyplenkov/surfer2-rs/issues/comments/6873800700 --jq .body
git clone --no-checkout https://github.com/cgalab/surfer2.git /tmp/opencode/surfer2-upstream-invariants
git -C /tmp/opencode/surfer2-upstream-invariants checkout --detach c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa
git ls-remote https://github.com/CGAL/cgal.git 'refs/tags/v5.0*'
git diff 244944daf2f0fc2ba9e21b412e424257a1574800 d1a89fd66795f537b034f220ed4f838bd0fdea3b -- src/numeric/polynomial.rs
git diff d1a89fd66795f537b034f220ed4f838bd0fdea3b 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c -- src/wavefront/supporting_line.rs
git show 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c:src/wavefront/supporting_line.rs
```

Git blob comparisons verified that `src/wavefront/{vertex,edge,split_leftover}.rs`, `src/triangulation/collapse.rs`, `src/propagator.rs`, `src/event/{collapse,queue}.rs`, and `src/geometry.rs` are identical at all three Rust revisions. `supporting_line.rs` is identical at published base/control and changes in Arm A. `numeric/polynomial.rs` differs between base and control only in determinant coefficient construction and an added test: the control subtracts the first point and velocity before expanding the determinant. Its root-selection routines cited above remain unchanged; control and Arm A are identical for that file.

No basin fixture was processed, so there is no newly executed input hash/profile/result to report. No dirty experimental diff was used. Artifact preservation and fixture/replay hashes belong to [ticket #6](https://github.com/atsyplenkov/surfer2-rs/issues/6). Engine builds, tests, Clippy, and performance runs were skipped for this documentation-only task. Checks for this deliverable are Markdown source/reference inspection and `git diff --check`.

### Arm A motion policy: local pinned evidence

Source: `git show 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c:src/wavefront/supporting_line.rs`, Git blob `42ad7ffe426692b17621900b5d6d1fd6b1e7800a`, whole-file SHA-256 `4e6d4b58d2083b92ab7c47457378644f3c3bfa6c3cbf5b31c5e0682405552d76`. The control/published blob is `bba25d0071001f4d5568fee811ed808e2f46411e`.

Code inspection establishes:

- Lines 88–97 reanchor on `pos - time*vel` only when resulting speed exceeds `SUBTRACTION_SPEED_LIMIT` and that origin is finite.
- Lines 114–124 define `SUBTRACTION_SPEED_LIMIT = 1e6`, `SUBTRACTION_FRAME = 1e4`, and `SUBTRACTION_DISAGREEMENT = 1e-10`.
- Lines 132–155 choose the subtraction result for moderate velocities in small frames, or for sufficiently agreeing formulas in larger frames; otherwise a finite normal-derived velocity is preferred. If the normal result is unavailable, a finite moderate subtraction result can still be used.
- Lines 157–162 return `a.point` when the original intersection origin is non-finite.
- Lines 187–188 compute direct-normal velocity using `intersect_lines(a.normal, a.direction, b.normal, b.direction)`.

Selected exact source excerpts:

```rust
if vel.x.hypot(vel.y) > SUBTRACTION_SPEED_LIMIT {
    let born = Point2::new(pos.x - time * vel.x, pos.y - time * vel.y);
    if born.is_finite() {
        return (born, vel);
    }
}
```

```rust
match (subtracted, normal) {
    (Some(sub), Some(nor)) if subtraction_is_trustworthy(p0, sub, nor) => Some((p0, sub)),
    (_, Some(nor)) => Some((finite_origin(p0, a), nor)),
    (Some(sub), None) if sub.x.hypot(sub.y) <= SUBTRACTION_SPEED_LIMIT => Some((p0, sub)),
    _ => None,
}
```

These guards are source facts. Their names/comments do not prove a numerical error bound, that an intersection farther away necessarily needs reanchoring, or that `a.point` lies on the other support. Arm A's motion changes do not alter the split conditioning classifier, child support-selection paths, or collapse rules catalogued above. This note neither blesses that policy nor attributes the handoff's megametre birth miss to a confirmed earliest cause.

## Pinned primary sources

### Surfer2 at `c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa`

The motion and lifecycle anchors are [U-support], [U-motion], [U-life], and [U-links]. Collapse prediction and handling are anchored by [U-edge], [U-collapse], [U-line-hit], [U-accept], [U-generic], [U-split], and [U-constraint-handler]. Scheduling and numerical configuration are anchored by [U-events], [U-queue], [U-propagate], [U-kernel], [U-polynomial], [U-build], and [U-deps]. Inline citations select the relevant source ranges.

[U-support]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontSupportingLine.h#L27-L61
[U-motion]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontVertex.cpp#L25-L179
[U-life]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontVertex.h#L43-L207
[U-links]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontVertex.h#L250-L312
[U-edge]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontEdge.cpp#L36-L154
[U-edge-split]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/WavefrontEdge.cpp#L183-L205
[U-collapse]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L664-L734
[U-triangle-handler]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1655-L1704
[U-constraint-handler]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1706-L1833
[U-spoke-handler]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1533-L1653
[U-split]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L2194-L2327
[U-refine]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L2329-L2454
[U-inf-predict]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L68-L137
[U-inf-handler]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1835-L2189
[U-weight-policy]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.h#L24-L30
[U-unbounded]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.h#L398-L416
[U-events]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/CollapseSpec.h#L65-L307
[U-queue]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/EventQueue.cpp#L100-L185
[U-propagate]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/SkeletonStructure.cpp#L58-L99
[U-finish]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/SkeletonStructure.h#L30-L44
[U-no-event]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L139-L161
[U-line-hit]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L204-L370
[U-generic-time]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L372-L509
[U-generic]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L523-L661
[U-accept]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L738-L817
[U-one-constraint]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.cpp#L819-L886
[U-cache]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangle.h#L125-L172
[U-modified]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1232-L1272
[U-valid]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/KineticTriangulation.cpp#L1172-L1229
[U-kernel]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/cgaltools.h#L22-L76
[U-polynomial]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/cgaltools.h#L170-L218
[U-quadratic]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/cgaltools.cpp#L20-L74
[U-heap-check]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/src/EventQueue.cpp#L199-L228
[U-build]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/CMakeLists.txt#L68-L112
[U-deps]: https://github.com/cgalab/surfer2/blob/c42f9a7f969cd661d060481cd3b8bac2bdfb8dfa/surf/CMakeLists.txt#L10-L20

### CGAL/CORE at `bec70a6d52d8aacb0b3d82a7b4edc3caa899184b`

[C-kernel] and [C-kernel-doc] define the exact-with-sqrt kernel; [C-traits] and [C-expr] supply CORE arithmetic semantics; [C-intersection] implements the relevant line-intersection classification.

[C-kernel]: https://github.com/CGAL/cgal/blob/bec70a6d52d8aacb0b3d82a7b4edc3caa899184b/Kernel_23/include/CGAL/Exact_predicates_exact_constructions_kernel_with_sqrt.h
[C-kernel-doc]: https://github.com/CGAL/cgal/blob/bec70a6d52d8aacb0b3d82a7b4edc3caa899184b/Kernel_23/doc/Kernel_23/CGAL/Exact_predicates_exact_constructions_kernel_with_sqrt.h
[C-traits]: https://github.com/CGAL/cgal/blob/bec70a6d52d8aacb0b3d82a7b4edc3caa899184b/Number_types/include/CGAL/CORE_Expr.h
[C-expr]: https://github.com/CGAL/cgal/blob/bec70a6d52d8aacb0b3d82a7b4edc3caa899184b/CGAL_Core/include/CGAL/CORE/Expr.h
[C-intersection]: https://github.com/CGAL/cgal/blob/bec70a6d52d8aacb0b3d82a7b4edc3caa899184b/Intersections_2/include/CGAL/Intersections_2/Line_2_Line_2.h

### Published Rust at `244944daf2f0fc2ba9e21b412e424257a1574800`

The comparison centers on [R-support], [R-vertex], [R-edge], [R-split-policy], [R-child], [R-leftover], [R-collapse], [R-one-constraint], and [R-poly]. The revision/blob checks above delimit where these published citations also describe the local control and Arm A.

[R-readme]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/README.md#L16
[R-support]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/wavefront/supporting_line.rs#L63-L113
[R-vertex]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/wavefront/vertex.rs#L30-L153
[R-edge]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/wavefront/edge.rs#L56-L121
[R-split-policy]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/wavefront/split_leftover.rs#L12-L116
[R-child]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/propagator.rs#L521-L644
[R-split-child]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/propagator.rs#L779-L912
[R-leftover]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/propagator.rs#L914-L1033
[R-triangle-handler]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/propagator.rs#L314-L382
[R-collapse]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/triangulation/collapse.rs#L97-L166
[R-edge-time]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/triangulation/collapse.rs#L390-L408
[R-one-constraint]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/triangulation/collapse.rs#L168-L378
[R-generic]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/triangulation/collapse.rs#L470-L544
[R-inf]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/triangulation/collapse.rs#L25-L77
[R-poly]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/numeric/polynomial.rs#L5-L165
[R-event]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/event/collapse.rs#L23-L218
[R-geometry]: https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/geometry.rs#L91-L114
