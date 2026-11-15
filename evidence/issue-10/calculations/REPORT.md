# Source and local-lifecycle checks for the basin-2 witness

This is diagnostic evidence, not a repair, a numerical tolerance, or a
resolution of the decision. The calculation covers the 12-vertex witness and
its locally relevant chain only. Coordinate distances are in input metres;
times and differences below use the engine's wavefront-time units.

## Reproduce

Start with the immutable [prior evidence and restoration instructions](https://github.com/atsyplenkov/surfer2-rs/blob/e3cfa2a65315f01e1f0082a36bd1f03adb858600/evidence/issue-5/README.md).
The restored directory must contain the bundled Arm A checkout, prior archived
scripts/traces and the bit-encoded original fixture. The issue-10 evidence
directory must contain `baseline.json`, `intervention.json`, `control.json`
and `active.json`. From any location:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evidence/issue-10/calculations/certify_witness.py \
  --restored /path/to/restored/repo \
  --traces /path/to/evidence/issue-10 > /tmp/issue10-certificate.json
cmp /tmp/issue10-certificate.json evidence/issue-10/calculations/certificate.json
```

Run here with `--restored /tmp/surfer2-issue10-baseline` and
`--traces /tmp/surfer2-issue10-evidence/evidence/issue-10`; exit 0, about
2 seconds. The first representative calculation reused the archived
`certify.py` without editing it; its output is `representative.log`. The
script and full reproducible intervals are `certify_witness.py` and
`certificate.json`.

Input and tool SHA-256:

| Input | SHA-256 |
|---|---|
| Original basin-2 bit fixture | `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a` |
| 12-vertex request | `ebf5cd6fc61b364ac237314b274e7923962d7739db420322df766a4085728db5` |
| Archived source certifier | `5266087d98b394da729fb39714431fead8ef29b661330ceb3f276deac97c5a19` |
| Archived full trace for original support orientation | `41dd6999ce51788555792db405e05839508551d79c4b00f63c0c02757960e4c2` |
| Baseline trace | `f5a3b5c369a6c902e8fe3954380920b33f61f0be68384e147c1ea5e57a0e40b7` |
| Intervention trace | `9bca8dc3d4f5d68236b4b49adda3439a5ab3980c1c3b54a996d80e3bef3121f2` |
| Control trace | `dbef3b7c7ac230252d46df0eb869cf97e1d241006b9df521569d27d6dc8f54fe` |
| Baseline with pre-event active-set snapshots | `5d4313face0ccbd490e0d349ce91cb451489c19cb094eb14b04897eb2fff701f` |
| This script | `e624444c00f46da15c0f09e46e2b89456469f625dbc5611c5de59eb2bdd36e9c` |
| This certificate | `926ead1d43fb4ed6137ca064d2c01b89aa8a586fc5573451772187b2888d77f7` |

The restored baseline source was Arm A
`1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c` plus the archived observer
overlay, not a clean Arm A tree. The controlled intervention and fresh observer
are identified in the enclosing report. Their effects are *observed*, not
assumptions in the source calculation.

## What is exact, enclosed, observed, or assumed

The witness copies original vertex bits for indices 594–603, 1143, 3151.
Exact integer predicates recheck distinctness, simple polygon boundaries
(including collinear overlap), nonzero area and original winding. Edges
603–1143, 1143–3151 and 3151–594 are synthetic; none is one of the local
source supports used below. The fixture has one ring and no holes. These
predicates do not prove propagation.

For each original unit-weight support, the archived certifier checks that its
stored direction exactly matches one orientation of the original binary64
endpoint difference, using exact dyadics. A line through source point `p`
with oriented direction `d=(dx,dy)` is advanced by
`(-dy,dx)·x = (-dy,dx)·p + sqrt(dx²+dy²)*t`. All differences, determinants,
and solves use rational arithmetic. Each square root is enclosed by integer
squaring on a `2^-192` grid; displayed interval endpoints are rounded
outwards. Nonzero determinants, the split fractions strictly between zero and
one, and the ordering inequalities are asserted.

The separate `active.json` observer records pre-event active edges, their
endpoint identities and incident triangles. Removing only the three added
fields `active_edges`, `active_vertices`, and `live_triangles` recursively
gives exact parsed equality with `baseline.json`; the certificate rechecks
this. The snapshots establish **what the simulated lifecycle recorded**. They
do not independently prove that an exact source-defined wavefront followed the
same lifecycle; the source time/point claims below are conditional on those
supports remaining physically active. Synthetic closure may affect other
events. The trace contains no independent exact replay of every possible
competing wavefront interaction.

The certificate separates:
- source-line enclosures at a specified time;
- exact-dyadic evaluation of the stored origin, velocity and event time;
- the recorded binary64 position and a separate Python non-fused replay;
- birth or stop coordinates and source-support residuals.

A logged selection filter, printed spread, or last-expression rounding error is
not a whole-event error budget.

## Earliest demonstrated mismatch in this witness chain

In all three traces, the first processed witness event is a
`ConstraintCollapse` of **active support 598**, with endpoints on
599/598 and 598/597. The pre-event snapshot identifies both endpoint vertices
and their incident triangle. On the source model, these three original lines
meet at

```text
[185.51628728706785946041, 185.51628728706785946042].
```

The observed event is 185.51628709685298, about
`1.902148821041e-7` time units earlier. The two observed endpoint
positions are about 390.224 nm apart. Their exact stored motions differ from
the two source positions by only 2.35–5.56 nm in x and -5.49/+4.21 nm in y;
exact stored evaluation versus each logged coordinate differs by at most
0.10 nm in this sample. This shows a source-co-arrival mismatch much larger
than that final evaluation rounding, under the stated active-support
assumption. It is the earliest demonstrated source-geometry mismatch in this
**recorded witness chain**, not a globally first invalid basin event. No
general acceptable tolerance follows from those numbers.

The created 599/597 successor's recorded birth is approximately 5.54 nm off
source support 599 and **385.971 nm** off support 597 at the recorded time.
The two supports are nearly parallel; their source intersection at that time
differs from the recorded birth by approximately
`(+1.176705383 m, -0.009915295 m)`. This is an ill-conditioned
construction: a small normal residual and a large intersection displacement
describe the same observation. They do not establish that the source-defined
successor was physically active.

The observer reports a 4.075 nm birth-replay miss for this successor. Exact
dyadic `origin + birth_time*velocity - birth` is approximately
`(-96.349 nm,-0.889 nm)`; in this sample the non-fused floating evaluation
rounds away about 92.274 nm in x. The stored replay miss, rounded replay, and
source-support residual are distinct quantities. Later, at the observed
next event, this successor's stored motion differs from its source support
intersection by about `(-1.176705480 m,+0.009915294 m)`; the final
evaluation rounding there is about 110 nm in x. Again, no assertion is made
that this difference alone caused all later behavior.

The source model puts the next 597/599/600 meeting about
`8.27206862790342e-6` time units after the first. Immediately before the
observed next event, the snapshot has the 599 edge endpoints 600/599 and
599/597. The source-defined 597/596/599 meeting would be only
`[2.42451809e-12,2.42451810e-12]` after that meeting. At the latter
meeting, remaining support 597 has source endpoint difference about 15 µm in
x and the same endpoint ordering as initially. Once 599 retires, its later
support-line meeting is not automatically a physical event. This is a
locally checked and tightly spaced ordering, conditional on the observed
active supports, not a proof of all competing events.

## The earlier split and 7.223 mm stop

At steps 2–4, the snapshots show support 597 endpoints 600/597 and
597/596, support 600 endpoints 601/600 and 600/597, and support 601 endpoints
602/601 and 601/600. The observed split's parent is 597/596 and its target is
601; the target endpoints are in the recorded event triangle. At step 5, the
snapshot has the three support edges 597, 600 and 601 as the stopped
triangle. These checks establish the **simulated** local adjacency and
lifecycle.

From the original lines, parent 597/596 hits support 601 at

```text
t = [371.02917296181096895215, 371.02917296181096895216]
p = ([464087.62496075457448820533, 464087.62496075457448820534],
     [6272133.01073118806821392040, 6272133.01073118806821392041]).
```

Its fraction from 601/600 toward 601/602 is strictly within
`[0.00000146593426536185,0.00000146593426536186]`. In that model it
is approximately 0.150 mm from the 601/600 endpoint, and the source endpoints
on 597 are also ordered as in the input. The source 601/600/602 meeting is
much later; that check excludes this one endpoint collapse, not all unrelated
events. The source 597/600/601 co-arrival is

```text
[371.02917296312872066359, 371.02917296312872066360],
```

or `[1.31775171143e-9,1.31775171144e-9]` after that split. This source
model therefore permits the split and a short-lived 597/601 child, conditional
on the checked support lifecycle.

The engine instead records the split at 371.0291729650514: about
`3.24040405994e-9` after the source split and
`1.92265234850e-9` **after** the source co-arrival. Extrapolating the
source 597/601 intersection to the recorded split puts it about 0.219 mm
past the source 601/600 endpoint in x. Therefore it would already have passed
its source-model lifetime; do not treat it as a physically active source
child at that recorded time.

The 597/601 successor's exact stored birth replay differs from its recorded
birth by approximately `(+0.368949946 mm,-0.003108721 mm)`. Source
intersection minus recorded birth is about
`(+0.368949032 mm,-0.003109003 mm)`. The separate non-fused replay's
rounding error is about 1.82 nm in x, far smaller than this miss. The replay
calculation does not prove a universal error budget or prescribe how to
construct the successor.

The following observed triangle stop is at 371.02917302656596, about
`6.343723599762e-8` after the source co-arrival, and its three logged
vertex positions are 7.2231548 mm apart at maximum. At this sampled time, the
largest coordinate discrepancy between stored motion and source support
motion across the three is about 33 nm; exact stored evaluation versus the
logged coordinates is at most 2.70 nm. Consequently the 7.223 mm is not
explained by the last position-evaluation rounding. The observer's three
vertices are not coincident when all are stopped; the source reference
explains the scale conditional on active supports. It does not certify the
entire event history.

## The later split, and why the control is a counterexample

The baseline's step 6 has source parent 594/595 and target 601 with simulated
active endpoints 602/601 and 601/596. Source-model hit time is
`[649.30528733131290608687,649.30528733131290608688]`; fraction from
602/601 to 601/596 is `[0.49999885705603042819,0.49999885705603042820]`,
strictly inside that source-defined edge under the active-support condition.
The source 595/596/601 meeting is
`[649.30541366328843704446,649.30541366328843704447]`.

The observed split is approximately `+4.0966943391e-8` time units from
the source hit in baseline, `-4.6960844326e-8` under the isolated
intervention, and `-9.3319267204e-8` in control. The 595/601 child's
reported birth-replay misses are respectively about 20.75 mm, 14.86 mm, and
33.64 mm. The intervention affects event prediction, so do not reuse the
baseline split time for it. By the final stop, the observed spreads are about
0.502 m in baseline and 0.000273 mm in intervention and control. The parent
report covers their emitted arcs, identities and face outcomes; these scalar
spreads alone do not establish correct output.

Control shares the original first observed event but takes a different local
predecessor path: its 596/601 successor arises through a later
constraint-collapse path, not the baseline's 371.029 split. It remains a
counterexample, not a source of ground-truth geometry or evidence of matching
whole ancestry.

## Implications and limits for discussion

- A finite child must be consistent both with its recorded birth and its two
  time-advanced source supports, with an error account appropriate to each
  construction's conditioning. A small rounded birth replay alone does not
  establish support consistency.
- A two-endpoint or three-vertex stop needs justified co-arrival and event
  lifecycle; checking one scalar quotient or choosing a distance because a
  face closes does not establish it. Near-simultaneous source events require
  an ordering rule that can express uncertainty.
- Topological identity of a common event endpoint is separate from coordinate
  proximity; the enclosing experiment must examine both incident arcs and
  face geometry.
- The active snapshots validate local simulated edge identities, not an
  independent exact wavefront replay. A stronger claim that every source
  event above was physically active would require bounding or replaying all
  locally competing events and ancestry over the intervals in question.
  This report intentionally does not require that broader proof.
- No production source, fixture, frozen oracle, threshold or PR was changed.
  No full-basin, debug, river, or whole-suite verification was performed by
  this calculation pass.
