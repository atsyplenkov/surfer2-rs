# Independent oracles for projected-basin geometry and point contacts

Research date: 2026-11-14. Answers [#7](https://github.com/atsyplenkov/surfer2-rs/issues/7)
on [planning map #3](https://github.com/atsyplenkov/surfer2-rs/issues/3), under the
[existing acceptance specification][spec].

## Answer

1. Use **exact dyadic input arithmetic plus certified square-root enclosures**
   for small support, birth, and co-arrival witnesses. This is a practical
   independent calculation, including for nearly parallel supports. Exact
   predicates, certified constructions, and agreement between floating-point
   calculations are different levels of evidence.
2. Use **analytic polygons**, followed by CGAL comparisons on its documented
   input domain, for complete small skeletons. CGAL 6.1 **excludes holes that
   touch another contour**, including an exterior vertex. Its straight-skeleton
   package is therefore not a direct oracle for basin 1. Basin 2 passes the
   input-domain audit below and is eligible after winding normalization.
3. Analyze basin 1 as an **exact embedded boundary arrangement with local
   interior sectors**, before comparing skeletons. The freshly computed closed
   input domain has first Betti number 2; this is derived from its arrangement,
   **not an asserted cycle rank for the exported skeleton**. A contact-preserving
   skeleton-to-domain argument or independent contact witness is still needed
   before making that graph expectation a test.
4. Validate the public finite coordinate graph, node/trace correspondence,
   branch coverage, embedded geometry, and DCEL separately. A valid DCEL, a
   connected tree, or unchanged counts can each describe the wrong skeleton.
5. Derive numerical budgets from the input bits, conditioning, evaluator
   arithmetic, serialization, and transformations. Resolve ambiguous comparisons
   with stronger evidence; do not use a larger tolerance to merge nodes.

These are research recommendations within the specification. No acceptance
values or engine policies are changed.

## Evidence, provenance, and terminology

**Source facts** below are cited to primary documentation/code. **Derived checks**
are calculations in this note, independent of skeleton output. **Conditional
recommendations** describe how to use future minimized witnesses; they do not
identify a cause of either residual failure.

- Verified before research: issue #7 is assigned to `atsyplenkov`.
- Research branch: `research/projected-basin-oracle-contract`, based on published
  `244944daf2f0fc2ba9e21b412e424257a1574800`. The worktree was clean.
- Read the governing specification, map, ticket, and
  `/tmp/handoff-surfer2-issue-1-projected-basins.md`. The handoff's failure
  observations are **reported evidence**, not rerun here.
- No applicable `AGENTS.md`, glossary, or ADR was found in the research worktree
  or its ancestor directories. The note is the single research deliverable.
- Exact fixtures were read from the main checkout's `.scratch/issues-1-2/`.
  Both basin files are byte-identical to
  `1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c:fixtures/projected/<name>`.
  That local commit is an input source, not this branch's ancestor or an oracle.

| Input | Extract SHA-256 | Ring lengths |
| --- | --- | --- |
| `pl_basins-1.json` | `486185b932322440f0a773496b67c80bc238e453c83253ae9a20d2e91631bf77` | 2900, 4, 4 |
| `pl_basins-2.json` | `1923b3261f191ba581bfca71d9a244dcc08dd769143a2bbce2506743cdb4e77a` | 3358 |

The source GeoPackage SHA-256 recorded in the specification and fixture
provenance is
`ba2731ab17579290ecf9108c34dc31e8b0e8973eab651e00ddd94a7e3c8e02a6`;
the GeoPackage itself was not reread. The fixtures contain `[x_bits, y_bits]`
pairs, with closure/consecutive duplicates removed as documented.

Working vocabulary for the checks:

- **Input coordinate**: the exact real dyadic rational represented by a supplied
  finite binary64 value, not an intended decimal or a rounded map coordinate.
- **Support**: an oriented, infinite moving line. Incidence on its line does
  not establish incidence on an active finite wavefront edge.
- **Boundary occurrence**: a ring/index occurrence of an input vertex. Two
  occurrences can share one coordinate while bounding different interior sectors.
- **Event node**: a space-time incidence with supporting-constraint provenance.
  Spatial proximity alone does not identify it.
- **Finite export graph**: the multigraph formed from the actually exported
  positive-length finite segments, keyed by exact endpoint coordinates.
- **Closed domain**: the closure of the odd-parity input interior, including its
  boundary and contact points. Its topology can differ from that of its open
  interior, and must not be confused with a serialized graph.

The published input façade itself documents odd-parity semantics and ignored
ring winding [R1]. These definitions clarify the checks without prescribing
upstream event-handler or successor semantics, which belong to #4.

## 1. Which references are applicable?

### CGAL: a useful independent engine with an important domain boundary

**Source facts, pinned to CGAL 6.1 commit
`b26b07a1242fb640003c72e9082432fb3a76296b`:**

- The manual defines the skeleton through inward-moving support lines and
  vertex traces, and says the package handles polygon interiors, not general
  planar straight-line figures [C1].
- The creation API requires a CCW exterior, CW holes, and holes that intersect
  neither each other nor the exterior [C2].
- The low-level builder explicitly calls it an error to enter a contour that
  “crosses or touches a hole.” Its allowance for certain self-touching contours
  is not permission for touching separate contours [C3].
- The actual wrapper converts points to the selected skeleton kernel and enters
  exterior/holes separately. `enter_contour(..., aCheckValidity=true)` removes
  consecutive coincident vertices and a repeated closure; that code is not a
  certificate that all documented polygon preconditions hold [C4].

**Applicability:** the two exact shared vertices proved in §2 disqualify basin 1
from this documented straight-skeleton domain. A successful CGAL run on it would
not override the precondition. Moving a hole, cutting a slit, or concatenating
contours changes the representation/problem and requires a separate equivalence
proof; no such transformed result is an oracle for the original fixture.

Basin 2's exact audit establishes a simple, nonzero-area ring without contacts
or crossings. Reversing its order if necessary makes it a documented CGAL input
without changing coordinates. This establishes eligibility, not that a
particular CGAL build has successfully processed it; no CGAL basin run was made.
Use a minimized eligible case first.

### Exact predicates are not exact support constructions

**Source facts:**

- `Exact_predicates_inexact_constructions_kernel` (EPICK) promises exact kernel
  predicates, while its geometric constructions are not generally exact [C5].
- The skeleton traits have filtered event predicates and filtered constructions,
  with conversions back to the selected output kernel [C6].
- The support-normalization source computes a square root. If the number type
  lacks a square-root functor, `inexact_sqrt_implementation` obtains an interval
  square root, converts it to `double`, then constructs the number type from
  that value [C7]. An exact rational type can therefore hold an approximation
  of an irrational length exactly.
- CGAL's `Exact_predicates_exact_constructions_kernel_with_sqrt` provides a
  `FieldWithSqrt` number type, requiring CORE or LEDA [C8]. The skeleton example
  distinguishes EPICK, EPECK, and EPECK-with-sqrt; the package also warns about
  inexact offset constructions [C1].

**Recommendation:** use EPICK as an independent differential reference with
numerical qualifications, not as certified exact coordinates. For a genuinely
algebraic CGAL small-case reference, select the square-root-capable kernel
explicitly at skeleton construction, confirm the build/backend and conversion
path, and preserve algebraic values or certified enclosures before exporting
`double`. Ordinary EPECK, an “exact” executable name, or converting an already
inexact skeleton does not by itself prove exact Euclidean constructions.
The kernel option is source-supported but was not built or executed here.

For weighted comparisons, normalize **physical normal speed**, not the spelling
of “weight.” CGAL's source multiplies normalized line coefficients by `w`, and
its moving-line equation is `w(ax+by+c)-t=0`, implying speed `1/w` [C7].
The manual's adjacent “larger weight ... faster” prose should not substitute for
checking that equation. Calibrate an asymmetric analytic case before translating
another engine's weight convention. No Surfer2 weight mapping is decided here.

### Recommended oracle ladder

| Method | Establishes | Applicability and limit |
| --- | --- | --- |
| Integer/rational predicates on input bits | Equality, orientation, parallelism, segment contacts, parity | Exact for the supplied coordinates; no irrational constructions involved |
| Rational arithmetic with certified root intervals, or exact algebraic arithmetic | Small support positions/velocities, birth residuals, co-arrival/order certificates | Needs source constraints, speed convention, and birth/event provenance |
| Analytic whole polygons | Complete expected branches, junctions, event times, offsets | Best for small independently specified cases; examples in §4 |
| CGAL straight skeleton with explicitly recorded kernel | Independent algorithmic comparison | Only documented polygon domain; excludes basin 1; numerical qualification above |
| Exact segment arrangement, e.g. CGAL `Arrangement_2` with an exact rational kernel | Contacts, embedding, face parity, containment, boundary topology | Handles intersecting segment arrangements [C9]; not a skeleton engine |
| Existing frozen Surfer2 corpus/lake outputs | Historical regression protection | Related implementation, serialization losses, and historical build qualifications; not fresh basin truth |

CGAL arrangement geometry uses rational segment intersections, so a
square-root-capable kernel is unnecessary for that task. The lightweight
integer arrangement audit in Appendix A avoids a native build entirely.

## 2. Basin 1: independently establish the contact topology

### Fresh exact-coordinate audit

Appendix A decodes all input bits, scales them to integers on a common
`2^-34` grid, checks all segment pairs with exact orientations, and constructs
the boundary rotation system. It rejects proper crossings, overlaps,
vertex-on-edge-interior contacts, repeated within-ring vertices, and degenerate
edges. Face walks keep the incident face on their left; parity is propagated
from the exterior across each boundary edge. This follows the planar-arrangement
model documented by CGAL [C9], but does not use CGAL or Rust skeleton code.

The completed audit found:

| Property | Basin 1 | Basin 2 |
| --- | ---: | ---: |
| Unique boundary coordinates `V` | 2906 | 3358 |
| Boundary segments `E` | 2908 | 3358 |
| Boundary graph components | 1 | 1 |
| Arrangement faces, including exterior | 4 | 2 |
| Bounded odd-parity faces | 1 | 1 |
| Bounded even-parity faces | 2 | 0 |
| Closed-domain Euler characteristic | -1 | 1 |

The only cross-ring contacts are these zero-based occurrences:

| Exterior occurrence | Hole occurrence | Exact binary64 coordinate, shown as round-trip decimals |
| --- | --- | --- |
| `(ring 0, vertex 323)` | `(ring 1, vertex 3)` | `(321516.9917565159, 6232018.113677429)` |
| `(ring 0, vertex 1236)` | `(ring 2, vertex 3)` | `(331616.9398583168, 6209694.968318121)` |

The single included face has a boundary walk of length **2908**, revisiting the
contact coordinates. The excluded bounded faces each have a four-edge walk.
There are no other segment intersections except shared endpoints.

At exterior vertex 323, outgoing rays in CCW order go to exterior 322, hole-1
vertex 0, hole-1 vertex 2, exterior 324. The sectors immediately CCW from these
rays have parities `1,0,1,0`. At exterior 1236, the corresponding order is hole-2
vertex 0, hole-2 vertex 2, exterior 1237, exterior 1235, with parities `0,1,0,1`.
Thus each contact has **two distinct local interior sectors at the same point**.
Keeping only a single ring-corner interpretation loses information.

### What that proves, and what it does not

**Derived topological result:** the closed odd domain is the connected boundary
graph plus its one included open face. Its boundary graph has cycle-space
dimension `E - V + 1 = 3`. Filling the included face adds one nonzero independent
boundary relation, leaving first Betti number **2**. Equivalently,
`chi = 2906 - 2908 + 1 = -1`, with one connected component and no closed
two-dimensional component. Basin 2 similarly has `chi=1` and first Betti number
zero. This calculation uses the verified embedding and included face, not
`ring_count - 1`.

The open included face has a connected boundary walk with no separate inner
boundary component: a point-touching exclusion is accessible from the outside
through the omitted contact point. Closing the domain adds that point back.
Confusing the open interior with its closure is precisely how apparently
plausible “tree” and “two-hole” arguments can disagree.

**Not yet established:** that the intended finite skeleton export is a
deformation retract of this closed, non-manifold-at-contact domain, with the
same identifications at time zero. Neither the CGAL domain contract nor a ring
count supplies that argument. Consequently **do not promote 2 to a frozen
export-graph cycle-rank assertion from this note alone**.

The next contact check should record a sector-resolved graph, the exact
coordinate quotient of that graph, and the map from all incident input
constraints to emitted branches. Identifying two distinct vertices in the same
connected graph increases its cycle rank by one; identifying vertices from
different components connects components without that increase. Which case
applies must be proved from the sector-resolved skeleton, not assumed.

A useful independent minimized topology control is the outer square with a
collinear boundary vertex at `(2,0)`,
`[(0,0),(2,0),(4,0),(4,4),(0,4)]`, and the diamond hole
`[(2,0),(1,1),(2,2),(3,1)]`. It has one vertex contact and the same two-sector
local structure. Its arrangement can be certified exactly. Its complete
contact skeleton still requires an analytic wavefront/roof derivation; a CGAL
run with these as separate contours cannot certify it. This is a proposed
future control, not a replacement for either basin or a demonstrated residual
failure.

## 3. Small geometric certificates from exact binary64 inputs

### Reconstruct the equations independently

For an edge from `P_i` to `Q_i`, oriented with the intended moving side on its
left, define

```text
d_i = Q_i - P_i
a_i = -d_iy,  b_i = d_ix
L_i = sqrt(d_ix² + d_iy²)
c_i = a_i P_ix + b_i P_iy

a_i x + b_i y = c_i + s_i L_i t
```

Here `s_i` is physical normal speed; unit-speed basins have `s_i=1`.
All `a_i,b_i,c_i,L_i²` are exact rationals from the coordinate bits.
Using unnormalized coefficients keeps square roots on the right-hand side.
For two supports, with rows `(a_i,b_i)` in `A`,

```text
det A = a_0 b_1 - b_0 a_1
p(0) = A^-1 c
v    = A^-1 (s_0 L_0, s_1 L_1)
p(t) = p(0) + t v.
```

**Exact predicate:** `det A != 0` proves a unique finite intersection velocity
over the reals for finite speeds and nonzero edges. Its magnitude is not a
parallelism predicate. `det A == 0` requires a rank/compatibility analysis of
the moving equations; an isolated-time coincidence of opposing parallel lines
is not an ordinary uniquely located moving vertex.

**Certified construction:** bound each square root outward, then propagate
intervals through the rational linear solve. An interval containing zero is
not an exact equality test. Increase precision for a strict sign; use an exact
algebraic zero test or structural identity for a true tie. MPFR supports
directed rounding, but explicitly does not track whole-computation accuracy
for the caller [N1]. Running it twice at higher precision without interval/error
propagation is evidence of agreement, not a certificate.

Appendix B uses only Python integers, `Fraction`, and `isqrt`. For `q >= 0`,
choose `D=2^160` and
`k=floor(sqrt(floor(q D²)))`; then
`k/D <= sqrt(q) <= (k+1)/D`, checked by exact squaring.
The interval width is a reference-computation precision, not an engine
acceptance tolerance. Python documents exact float-to-rational extraction [N2].

### A certified check on the nine-corner support pair

For edges `4->5` and `6->7` in the specification's nine-corner polygon, Appendix B
computes independently:

```text
det A = 1233784399225613 / 4611686018427387904 != 0
v_x in [19801495.065647, 19801495.065648]
v_y in [-206733.351676, -206733.351675]
```

The printed bounds are rounded outward to six decimal places; the underlying
rational component enclosures each have width below `1e-38`. In particular
`v_x > 10^6` is certified. These are left-normal equations for the listed edge
directions; reversing both orientations reverses velocity and preserves speed.
The four original binary64 points match basin 2 ring-0 vertices
1321, 1322, 1323, 1324 exactly.

This verifies a support-geometry fact already relevant to the reduced control.
It says neither that these supports are active in a residual event nor that
this velocity should replace a particular implementation path. It is not a
candidate-generated golden skeleton.

### Birth replay: distinguish source geometry from stored-state replay

For a proposed birth `(B,t_b)`, independently evaluate:

1. Support residuals `a_i B_x + b_i B_y - c_i - s_i L_i t_b`.
2. Trajectory replay residual `p(0) + t_b v - B`.
3. The anchored form `B + (t-t_b)v`, with both support residuals at later times.

A correct anchor cannot make an inconsistent birth lie on both moving supports.
Conversely, exact evaluation of a serialized binary64 trajectory describes that
rounded trajectory, not necessarily the intended motion from original edges.
Record both comparisons. If source birth time/position are algebraic, retain
their defining constraints and certified intervals instead of treating the
rounded birth fields as exact source truth.

An analytic dyadic edge family provides a finite-fast control without borrowing
any implementation values. Put `H=2^24`. The directions
`(1,0)` and `(-1+H^-2,-2/H)` have rational lengths. Their inward unit normals can
be written

```text
n_0 = (0,1)
n_1 = (2H/(H²+1), -(H²-1)/(H²+1)).
```

Solving `n_i dot v = 1` gives **`v=(H,1)` exactly**. With the plus sign on the
second normal's y component, the same-direction case gives `v=(1/H,1)`.
For any exact birth, `p(0)=B-t_b v` replays it exactly; Appendix B checks the
projected-coordinate example `B=(458000,6248000)`, `t_b=13/8`.
These are algebraic controls, not residual-basin events.

### Co-arrival: certify the plane, not one coordinate

For two affine trajectories relative to one epoch, write
`p(t)-q(t)=r+u t`. If `u != 0`, exact planar co-arrival requires

```text
cross(r,u) = 0
t = -dot(r,u) / dot(u,u)
r + u t = (0,0).
```

Also require the time to be in both trajectories' active intervals. If `u=0`,
distinguish trajectories coincident throughout from trajectories never meeting.
For a diagnostic closest approach the same dot-product formula gives `t_*`,
and `||r+u t_*||²` is the squared miss; a nonzero miss is not a collision.
For an interval-limited lifetime, minimize on that interval, including endpoints.

Appendix B's exact example `r=(1,2), u=(-1,-1)` has x-time 1, y-time 2,
closest time `3/2`, and squared miss `1/2`: an x-only time is not co-arrival.
For three vertices, all three pairwise residuals must vanish at a common
certified time; separate small pairwise minima at different times do not suffice.

An independent three-support construction solves
`[a_i, b_i, -s_i L_i] (x,y,t)^T = c_i` for three rows. It must then pass active
edge-segment membership, lifetime, side, and competing-earlier-event checks.
A zero signed triangle area only proves collinearity. A simultaneous
intersection of infinite supports alone does not license stopping every
incident vertex. The minimized witness must supply the missing active
constraints and lifecycle facts; this note does not infer them from face IDs.

## 4. Analytically complete small skeletons

These expected results follow directly from moving support equations [C1], not
from a Rust snapshot:

| Input, unit speeds | Analytic result | What it exercises |
| --- | --- | --- |
| Square `[0,2]²` | Four corner-to-`(1,1)` arcs; common stop time 1 | A genuine simultaneous junction; no artificial degree-three resolution required |
| Rectangle `[0,6] x [0,2]` | Four corner arcs to `(1,1)` and `(5,1)`, plus the ridge between them at time 1 | A positive-length ridge can have equal endpoint times; deleting zero-duration arcs is wrong |
| Triangle `(0,0),(4,0),(0,3)` | Three corner-to-`(1,1)` arcs, stop time/inradius 1 | Independent exact rational construction |
| Triangle `A=(0,0), B=(1,0), C=(1-H^-2,2/H)` | Three arcs to the incenter; at A the initial velocity is `(H,1)` | Finite-fast motion in an entire nondegenerate dyadic polygon, with analytically determined termination |

For any nondegenerate triangle, the incenter is
`I=(a A+b B+c C)/(a+b+c)`, where `a=|B-C|`, `b=|C-A|`, `c=|A-B|`, and
the inradius is `2 area/(a+b+c)`. Convex inward offsets have exactly these three
vertices until collapse, which supplies both geometry and completeness.
Use certified root intervals for the needle triangle's irrational lengths.

For a convex polygon more generally, independently clip the half-planes
`a_i x+b_i y >= c_i+s_i L_i t` at time `t`. Enumerating feasible changes of
active constraints provides a small-case certificate; it also supplies offset
sections between certified event times. Applying all support half-planes to a
nonconvex polygon instead computes an intersection that need not be its
wavefront, so this construction is restricted to convex controls.

Exact parallel and weighted variants should be derived using explicit physical
speeds. Perturbing a square into a rectangle supplies a controlled split of a
simultaneous event, but neither perturbation nor its limit replaces the exact
degenerate case. A generic `2n-3` arc count is inapplicable to higher-degree
simultaneous junctions, collinear vertices, contacts, and subdivisions.

## 5. Detecting incorrect output despite valid bounded cycles

The following checks are complementary. They operationalize the existing
specification; none alone proves the complete nonconvex skeleton.

### Graph identity and branch preservation

1. Build the finite-export multigraph directly from exact endpoint coordinates
   of the returned segments. Check finite positive lengths, finite ordered times
   `0 <= t_start <= t_end`, component count, and `E-V+C`. Equal endpoint times
   are allowed for positive-length ridges. Do not use rounded bins, proximity
   joins, or node IDs to establish connectivity. Preserve multiplicity for
   audit rather than silently deduplicating arcs.
2. Independently build the public-node-ID graph and the endpoint occurrence
   table. Every occurrence of one public node must agree with its advertised
   coordinate and time; record distinct IDs sharing a coordinate. Compare the
   coordinate quotient explicitly. Exact XY equality alone cannot certify that
   two different space-time events were correctly merged.
3. Track raw kinetic trace, public segment, supporting-constraint pair, incident
   faces, and endpoint event identity. Check that every required positive-length
   trace survives emission/export and that no unexplained multiplicity vanishes.
   Conversely, a raw trace can itself be invalid; this conservation check is
   not proof of its geometric correctness.
4. Require the nine specified corners in the reduced case and independently
   derived branches in analytic cases. At contacts, account for boundary
   occurrences and their sectors, not just the set of unique coordinates.
5. Retain reciprocal twins, next/previous agreement, face ownership, root
   reachability, and full bounded-halfedge coverage. These are additional
   conditions, not substitutes for the finite graph.

For signed zero, report whether a bit-key graph and an exact-numeric-coordinate
graph differ (`+0` and `-0` represent the same real number). This explicit
representation issue is distinct from any positive-distance stitching.

Two different trees can have identical `V`, `E`, degree histograms, and total
length; a missing branch can be offset by an extra branch. A many-to-one
nearest-node match can hide a merge. Compare a bijection of justified junctions
and the edge multiset, including time/support labels where available.

Representation-only degree-two subdivisions can be compared as polylines when
the subdivision is proved to preserve the same geometric segment, time law,
support labels, and incidence. Do not contract a short branch, an event junction,
or a real ridge merely to make graphs isomorphic. Genuine unresolved
degeneracies need an explicit equivalence proof.

### Embedding and containment

Use an exact arrangement of input boundaries and exported segments:

- Split at all exact intersections. Unexpected crossings, interior endpoints
  on other arcs, overlaps, and duplicate arcs need explanation; do not silently
  node them and accept the resulting connectivity.
- Test the **entire segment** against the closed odd domain, not just its
  endpoints, bounding box, or a fixed set of samples. Concave boundaries and
  holes make endpoint-only checks insufficient. Exact intersections partition
  a segment into intervals of constant domain membership.
- With a justified geometric error budget, certify that any outside portions
  lie in the allowed domain-distance band. This requires a bound on the whole
  outside portion, e.g. certified distance intervals/subdivision or exact
  coverage by dilated domain triangles. A midpoint within the band is not a
  certificate for its whole interval.
- If an independently known straight reference arc is contained in the domain,
  certified endpoint errors bound every point of the paired segment by linear
  interpolation. This is useful for analytic cases and avoids sampling.

Containment checks cannot establish completeness: an empty or truncated skeleton
can be perfectly contained.

### Stronger completeness evidence

For a small nonconvex witness, combine an eligible independent engine result
with a geometric/event certificate: all required corner/sector traces; correct
active support incidences and chronological meetings; no earlier admissible
event omitted; and a complete terminal wavefront state. For analytically
enumerable cases this is a finite independent calculation.

For larger outputs, an independent overlay of projected skeleton faces can
check that they cover the closed domain with disjoint interiors and retain all
input constraints. Lift each face by its source-support time function, check
nonnegative heights and compatible heights on seams, and compare offset
sections at certified regular times. This detects omissions that face-cycle
validation misses. For nonconvex input, a self-consistent partition/roof alone
still needs the correct wavefront visibility/chronology to establish the
straight skeleton; sampled offsets alone are also incomplete evidence.

### What the existing frozen references actually buy

At published `244944d`, the GIS provenance identifies lake9706 as upstream
Surfer2 `c42f9a7...` double output and lake10623 as exact-build output; it reports
that the double build failed final DCEL construction for the latter. It also
records 17-digit OBJ output and historical compatibility patches not included
in this repository [R2]. This is valuable historical evidence, not a blanket
accuracy bound for new basin events.

The OBJ parser derives skeleton segments from face edges, drops time-zero
boundary edges/infinite-time edges/zero-spatial-length edges, and deduplicates
identical space-time endpoint pairs [R3]. Consequently it cannot independently
recover every original trace identity or explain multiplicity. `match_segments`
compares geometry after discarding its time fields, while the graph comparator
uses a tolerance-respecting node bijection and edge multisets [R4].

Keep these frozen gates intact. Their tolerances and provenance do not establish
a new residual-witness error budget. A source-derived port and its source
implementation can share assumptions or defects; agreement is strongest when
paired with analytic/algebraic checks. No new frozen values were generated.

## 6. Numerical budgets and metamorphic comparisons

### Derive budgets before judging candidate discrepancies

Use separate ledgers for:

1. **Input:** zero uncertainty when testing the exact supplied bits. Any external
   survey/CRS uncertainty is a different question and cannot excuse an output
   gap for these tests.
2. **Reference:** the certified interval width from rational/root or directed
   interval arithmetic.
3. **Evaluator:** a bound for the actual chosen arithmetic path, including
   stored coefficient error, square-root/normalization error, event-time error,
   multiply/add or FMA behavior, underflow/overflow, and final rounding.
4. **Serialization/transform:** quantified decimal or binary output rounding
   and any representational change caused by transformed inputs.

For the rational two-support matrix above, a useful diagnostic identity is

```text
r = c + (s_i L_i)t - A x_hat
x_* - x_hat = A^-1 r
|x_* - x_hat| <= |A^-1| |r|       (componentwise).
```

Certified intervals make this a rigorous forward-error enclosure. The same
reasoning applies to velocity. A small support residual can correspond to a
large position error when the supports nearly oppose each other. An event-time
error is amplified by motion: a time interval of radius `delta_t` contributes
at most `|v| delta_t`, plus the certified velocity/anchor terms. At the checked
support pair, even `delta_t=10^-6` contributes about 19.8 coordinate units along
x; “small in time” does not mean “small in geometry.”

**Crucial distinction:** computing a residual bound around the candidate and
then accepting whatever bound it produces is circular. Use the residual to
measure/certify the discrepancy, and compare it with a separately justified
budget from representation and evaluator arithmetic. A standard normal-range
round-to-nearest model uses unit roundoff `u=2^-53` and accumulated factors such
as `gamma_k=ku/(1-ku)` for an appropriate `k`-operation path with `ku<1`.
This follows by bounding products of per-operation factors `1+delta`,
`|delta|<=u`, using `(1+u)^k <= 1/(1-ku)`; the expression's dependency structure
determines the applicable paths. Cancellation requires absolute operand-scale
bounds, not a relative bound on a tiny result.
Exceptional ranges and library operations need their own established bounds.
The actual path and conditioning must come from the minimized witness before
putting a number into a regression.

If intervals cannot separate event order, establish an algebraic tie or mark
the comparison unresolved. If position bounds encompass two distinct candidate
junctions, use constraint/time incidence to disambiguate or improve the
reference. A positional tolerance must not decide topology.

For comparison between two independently bounded results, a valid coordinate
allowance is the sum of their certified errors and the mapping error. It is
not automatically an existing `64 ULP`, bounding-box-relative rule, millimetre,
or centimetre threshold. Those constants become justified only through an
applicable analysis, not because previous fixtures happened to pass them.

### Winding, ring starts, and translation

**Unchanged-bit transformations:** cyclically permute a ring, reverse its order,
and permute holes while retaining original point/edge provenance. The physical
unweighted polygon is identical. Require every run to independently satisfy
exact endpoint-sharing and graph validity, then compare its effective embedded
graph under the provenance map and certified coordinate budgets. CGAL's
orientation requirements must be normalized in the reference adapter; do not
feed it the intentionally reversed polygon and call rejection a discrepancy.
For weighted controls, weights must follow geometric edges under reversal.

**Translation:** for `T(x)=x-c`, evaluate every transformed coordinate as an
exact rational first. Check whether it is exactly representable in binary64;
if it is, its mapped input perturbation is zero. Otherwise record
`fl(x-c)-(x-c)` exactly and either bound its effect or use a different diagnostic
translation. Floating-point translation is not universally geometry-preserving.
In particular it can change collinearity, degeneracies, or small features even
if many individual coordinates round-trip.

A certified local-coordinate reference can subtract an exact rational anchor,
solve independently, and add it back with controlled rounding. This is an
oracle technique, not a required production preprocessing step. Compare
translated outputs after the inverse map, including inverse-map rounding and
input perturbations; require exact sharing **within** each result, not
bit-identical endpoints across frames/platforms.

For nonsingular isolated events, interval sensitivity of the support/event
equations can bound transformed-input effects. At an exact contact or
simultaneous event, an arbitrarily small perturbation can change combinatorics.
Do not assume a Lipschitz graph correspondence across such perturbations.
Prefer exact-representable transforms for those controls or supply an explicit
degeneracy analysis. Internal triangulation, queue order, node numbering, and
serialization order need not be invariant.

## 7. What waits for minimized witnesses?

The oracle-selection question is answered. The following evidence belongs to
the next diagnostic/design work, not a speculative repair:

- **For each residual event:** original edge identities and exact endpoint
  bits; physical speeds/orientations; birth definitions and active lifetimes;
  competing support/segment events; serialized trajectory bits; intended
  versus actual stop identity; and the raw-to-public trace map. These enable the
  certificates in §3. Face IDs or rounded log coordinates are insufficient.
- **Contact-to-export topology:** prove the sector-resolved-to-coordinate
  quotient on a minimal one-contact witness, including time-zero branch
  multiplicity and any equal-time ridges. Then determine whether the closed
  domain's independently computed Betti number transfers to the intended
  finite export. This is a precise new proof obligation, not a choice to relax
  connectivity or discard contact constraints.
- **Witness-specific budgets:** derive separate position/time/reference and
  metamorphic allowances from the demonstrated arithmetic and conditioning.
  This is already part of map #3's unspecified numerical policy.
- **Reference execution, if needed:** record exact CGAL revision, kernel,
  arithmetic backend/version, compiler flags, input adapter, support/weight
  convention, and output precision. Start with analytic controls and eligible
  minimized polygons. A large native build is not necessary to decide input
  applicability.

If a separate decision ticket is useful, the genuinely new narrow question is:
**“Which sector identifications at a point-touching hole preserve the intended
finite skeleton, and does its coordinate quotient retain the closed domain's
first Betti number?”** It should consume an independent contact witness and the
diagnostic trace identity evidence. The existing #5 can also own this proof
obligation; no duplicate numerical-repair ticket is suggested.

## 8. Execution record and limits

Executed on Linux with Python 3.14.8:

```bash
gh issue view 7 --repo atsyplenkov/surfer2-rs --json number,title,body,assignees,comments,url,state
gh issue view 3 --repo atsyplenkov/surfer2-rs --json number,title,body,comments,url,state
git rev-parse HEAD origin/master
git show 1b6ed8ea9af51ed9f82f99d56306abc6f6359c7c:fixtures/projected/PROVENANCE.txt
python /tmp/opencode/oracle-input-audit.py \
  /home/ats/Development/surfer2-rs/.scratch/issues-1-2/pl_basins-1.json \
  /home/ats/Development/surfer2-rs/.scratch/issues-1-2/pl_basins-2.json
python /tmp/opencode/oracle-small-calculations.py
```

The last two scripts are reproduced verbatim below, so the temporary paths are
not publication dependencies. Both final runs exited 0. During preparation,
the arithmetic script initially asserted that the y component exceeded `1e6`;
the result showed the large component was x. That calculator assertion was
corrected and rerun; no engine result was involved. A separate exact bit lookup
verified the four support endpoints' basin-2 indices.

Fixture equality was checked by comparing file bytes with `git show` output,
and SHA-256 was computed with Python `hashlib`. CGAL release tag `v6.1` resolves
through annotated tag `63ad16c49dbfda916a88f91c6b0c6d4d0233de7c` to the pinned
commit used throughout. Primary-source files were retrieved at that commit.

No engine or full-basin skeleton tests, debug/release comparisons, native
reference-engine builds, cargo checks, or candidate-output golden generation
were performed. The full-input work was an input-boundary audit only. No dirty
engine experiment was used, so there is no dirty engine-diff hash or execution
profile to attribute to these calculations. The diagnosing-bugs workflow is
used here to define independently red-capable future checks; this research
does not claim to have run the residual bug loop or established a causal
hypothesis.

## Appendix A. Reproducible exact input-arrangement audit

Save this block as `/tmp/opencode/oracle-input-audit.py` and use the command
above, or pass the same hash-verified files from another location.

```python
import hashlib
import json
import struct
import sys
from collections import defaultdict, deque
from fractions import Fraction as Q
from functools import cmp_to_key
from pathlib import Path


def sub(a, b):
    return a[0] - b[0], a[1] - b[1]


def cross(a, b):
    return a[0] * b[1] - a[1] * b[0]


def orient(a, b, c):
    return cross(sub(b, a), sub(c, a))


def on(a, b, p):
    return orient(a, b, p) == 0 and all(
        min(a[k], b[k]) <= p[k] <= max(a[k], b[k]) for k in (0, 1)
    )


def intersections(a, b, c, d):
    if any(max(a[k], b[k]) < min(c[k], d[k]) or
           max(c[k], d[k]) < min(a[k], b[k]) for k in (0, 1)):
        return set()
    o = orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b)
    assert not (o[0] * o[1] < 0 and o[2] * o[3] < 0), "proper crossing"
    return {p for p in (a, b, c, d) if on(a, b, p) and on(c, d, p)}


def angle_cmp(a, b):
    ha = int(a[1] < 0 or (a[1] == 0 and a[0] < 0))
    hb = int(b[1] < 0 or (b[1] == 0 and b[0] < 0))
    if ha != hb:
        return ha - hb
    c = cross(a, b)
    return (c < 0) - (c > 0)


for filename in sys.argv[1:]:
    raw = Path(filename).read_bytes()
    bits = json.loads(raw)
    qrings = [
        [tuple(Q(struct.unpack("<d", struct.pack("<Q", n))[0]) for n in p)
         for p in ring] for ring in bits
    ]
    scale = max(q.denominator for ring in qrings for p in ring for q in p)
    rings = [[tuple(int(q * scale) for q in p) for p in ring] for ring in qrings]
    edges = [(a, ring[(i + 1) % len(ring)], r, i)
             for r, ring in enumerate(rings) for i, a in enumerate(ring)]
    owners = defaultdict(list)
    for r, ring in enumerate(rings):
        assert len(set(ring)) == len(ring), "repeated vertex within a ring"
        for i, p in enumerate(ring):
            owners[p].append((r, i))
    contacts = set()
    for i, (a, b, r, _) in enumerate(edges):
        assert a != b
        for c, d, s, _ in edges[i + 1:]:
            hit = intersections(a, b, c, d)
            assert len(hit) <= 1, "overlap"
            assert hit <= ({a, b} & {c, d}), "vertex on edge interior"
            if hit and r != s:
                contacts.update(hit)
    adj = defaultdict(list)
    for a, b, _, _ in edges:
        adj[a].append(b)
        adj[b].append(a)
    for p in adj:
        adj[p].sort(key=cmp_to_key(lambda a, b: angle_cmp(sub(a, p), sub(b, p))))
    seen = set()
    stack = [next(iter(adj))]
    while stack:
        p = stack.pop()
        if p not in seen:
            seen.add(p)
            stack.extend(adj[p])
    assert len(seen) == len(adj), "audit below assumes a connected boundary graph"
    face_of, walks = {}, []
    for a in adj:
        for b in adj[a]:
            if (a, b) in face_of:
                continue
            first, e, walk = (a, b), (a, b), []
            while e not in face_of:
                face_of[e] = len(walks)
                walk.append(e)
                u, v = e
                e = v, adj[v][(adj[v].index(u) - 1) % len(adj[v])]
            assert e == first
            walks.append(walk)
    area2 = [sum(cross(a, b) for a, b in w) for w in walks]
    outside, = [i for i, area in enumerate(area2) if area < 0]
    dual = defaultdict(list)
    for a, b, _, _ in edges:
        f, g = face_of[a, b], face_of[b, a]
        assert f != g
        dual[f].append(g)
        dual[g].append(f)
    parity, queue = {outside: 0}, deque([outside])
    while queue:
        f = queue.popleft()
        for g in dual[f]:
            if g not in parity:
                parity[g] = 1 - parity[f]
                queue.append(g)
            assert parity[g] == 1 - parity[f]
    assert len(parity) == len(walks)
    v, e, f = len(adj), len(edges), len(walks)
    assert v - e + f == 2
    print(Path(filename).name, hashlib.sha256(raw).hexdigest())
    print("rings", [len(r) for r in rings], "scale", scale)
    print("contact occurrences", sorted(owners[p] for p in contacts))
    print("boundary V E F C", v, e, f, 1)
    print("faces (walk length, parity, bounded)",
          sorted((len(w), parity[i], i != outside) for i, w in enumerate(walks)))
    for p in sorted(contacts):
        print("contact sectors CCW", owners[p],
              [(owners[q], parity[face_of[p, q]]) for q in adj[p]])
    included_faces = sum(parity.values())
    print("closed odd-domain chi", v - e + included_faces)
```

Key output:

```text
basin 1:
contact occurrences [[(0, 323), (1, 3)], [(0, 1236), (2, 3)]]
boundary V E F C 2906 2908 4 1
faces (walk length, parity, bounded) [(4, 0, True), (4, 0, True), (2900, 0, False), (2908, 1, True)]
closed odd-domain chi -1

basin 2:
contact occurrences []
boundary V E F C 3358 3358 2 1
faces (walk length, parity, bounded) [(3358, 0, False), (3358, 1, True)]
closed odd-domain chi 1
```

This deliberately small auditor assumes a connected, endpoint-noded boundary
graph and rejects other arrangements. For disjoint-hole reference polygons use
a general exact arrangement implementation that groups inner/outer boundary
walks into faces. Do not extend this script's Euler count blindly to those inputs.

## Appendix B. Reproducible arithmetic certificates

Save as `/tmp/opencode/oracle-small-calculations.py`; no third-party dependency
or engine import is required.

```python
from fractions import Fraction as Q
from math import isqrt


def sqrt_bounds(q, bits=160):
    assert q >= 0
    den = 1 << bits
    k = isqrt((q.numerator * den * den) // q.denominator)
    lo = Q(k, den)
    hi = lo if lo * lo == q else Q(k + 1, den)
    assert lo * lo <= q <= hi * hi
    return lo, hi


def add(a, b):
    return a[0] + b[0], a[1] + b[1]


def scale(a, k):
    ends = a[0] * k, a[1] * k
    return min(ends), max(ends)


def outward_decimal(interval, digits=6):
    den = 10 ** digits
    lo = (interval[0] * den).__floor__()
    hi = (interval[1] * den).__ceil__()
    def fmt(n):
        return f"{'-' if n < 0 else ''}{abs(n) // den}.{abs(n) % den:0{digits}d}"
    return f"[{fmt(lo)}, {fmt(hi)}]"


# Edges 4->5 and 6->7 of the nine-corner input in the governing spec.
# Convert the binary64 values, not the displayed decimal strings, to rationals.
points = [
    (455661.52568074607, 6248187.787704309),
    (455610.0601245094, 6248188.325017044),
    (455611.02902955154, 6248281.07562989),
    (455662.4934623703, 6248280.5383236855),
]
p = [tuple(Q(x) for x in pt) for pt in points]
lines = []
for j in (0, 2):
    dx, dy = p[j + 1][0] - p[j][0], p[j + 1][1] - p[j][1]
    lines.append((-dy, dx, sqrt_bounds(dx * dx + dy * dy)))
a0, b0, l0 = lines[0]
a1, b1, l1 = lines[1]
det = a0 * b1 - b0 * a1
assert det != 0
vx = scale(add(scale(l0, b1), scale(l1, -b0)), 1 / det)
vy = scale(add(scale(l1, a0), scale(l0, -a1)), 1 / det)
print("input hex", [[x.hex() for x in pt] for pt in points])
print("det", det)
print("left-normal vx", outward_decimal(vx))
print("left-normal vy", outward_decimal(vy))
assert vx[0] > 10**6
assert max(vx[1] - vx[0], vy[1] - vy[0]) < Q(1, 10**38)
print("component enclosure widths < 1e-38")

# Independent rational, near-parallel support family, H=2^24.
# Dyadic directions (1,0), (-1+H^-2,-2/H) have rational lengths.
h = 1 << 24
s, c = Q(2 * h, h * h + 1), Q(h * h - 1, h * h + 1)
assert s * s + c * c == 1
v = ((1 + c) / s, Q(1))
assert v == (Q(h), Q(1))
same_direction_vx = (1 - c) / s
assert same_direction_vx == Q(1, h)
b, tb = (Q(458000), Q(6248000)), Q(13, 8)
p0 = tuple(b[k] - v[k] * tb for k in (0, 1))
assert tuple(p0[k] + v[k] * tb for k in (0, 1)) == b
print("synthetic opposing vx, same-direction vx", v[0], same_direction_vx)
print("exact birth replay residual", (Q(0), Q(0)))

# Same x at t=1 does not imply planar co-arrival.
# Relative displacement r=(1,2), relative velocity u=(-1,-1).
r, u = (Q(1), Q(2)), (Q(-1), Q(-1))
tx, ty = -r[0] / u[0], -r[1] / u[1]
tc = -sum(r[k] * u[k] for k in (0, 1)) / sum(x * x for x in u)
res2 = sum((r[k] + tc * u[k]) ** 2 for k in (0, 1))
assert (tx, ty, tc, res2) == (1, 2, Q(3, 2), Q(1, 2))
print("x-time y-time closest-time squared-miss", tx, ty, tc, res2)
```

Output:

```text
input hex [['0x1.bcfb61a4c0db2p+18', '0x1.7d5bef269bf56p+22'], ['0x1.bcee83d914786p+18', '0x1.7d5bf14cd144ap+22'], ['0x1.bceec1db9ec3ap+18', '0x1.7d5d644d71ec0p+22'], ['0x1.bcfb9f94e3319p+18', '0x1.7d5d62273e530p+22']]
det 1233784399225613/4611686018427387904
left-normal vx [19801495.065647, 19801495.065648]
left-normal vy [-206733.351676, -206733.351675]
component enclosure widths < 1e-38
synthetic opposing vx, same-direction vx 16777216 1/16777216
exact birth replay residual (Fraction(0, 1), Fraction(0, 1))
x-time y-time closest-time squared-miss 1 2 3/2 1/2
```

## Primary sources

All CGAL source links below use the same inspected 6.1 release commit.

- **[C1]** CGAL 6.1, [straight-skeleton manual](https://doc.cgal.org/6.1/Straight_skeleton_2/index.html);
  [pinned manual source](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/doc/Straight_skeleton_2/Straight_skeleton_2.txt).
  Definitions, general-polygon restriction, weighted equation, and exact/inexact
  offset discussion.
- **[C2]** [Creation API preconditions](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/doc/Straight_skeleton_2/CGAL/create_straight_skeleton_2.h#L18-L24).
- **[C3]** [Builder contour contract](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/doc/Straight_skeleton_2/CGAL/Straight_skeleton_builder_2.h#L166-L181).
- **[C4]** [Actual creation wrapper](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/create_straight_skeleton_2.h#L35-L62)
  and [contour-entry implementation](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/Straight_skeleton_builder_2.h#L1396-L1430).
- **[C5]** [EPICK guarantees](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Kernel_23/doc/Kernel_23/CGAL/Exact_predicates_inexact_constructions_kernel.h#L7-L24).
- **[C6]** [Filtered skeleton predicates/constructions](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/Straight_skeleton_builder_traits_2.h#L528-L615)
  and [filter selection/conversion](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/Straight_skeleton_2/Straight_skeleton_builder_traits_2_aux.h#L70-L164).
- **[C7]** [Square-root fallback and normalization](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/constructions/Straight_skeleton_cons_ftC2.h#L90-L205),
  [weighted coefficients](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/constructions/Straight_skeleton_cons_ftC2.h#L254-L279),
  and [moving-line equation](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Straight_skeleton_2/include/CGAL/constructions/Straight_skeleton_cons_ftC2.h#L323-L366).
- **[C8]** [Exact construction kernel with square root](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Kernel_23/doc/Kernel_23/CGAL/Exact_predicates_exact_constructions_kernel_with_sqrt.h#L7-L18).
- **[C9]** CGAL 6.1, [arrangement model, rotations, faces, and intersections](https://doc.cgal.org/6.1/Arrangement_on_surface_2/index.html#aos_ssec-basic-dcel);
  [pinned manual source](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Arrangement_on_surface_2/doc/Arrangement_on_surface_2/Arrangement_on_surface_2.txt)
  and [segment-traits implementation](https://github.com/CGAL/cgal/blob/b26b07a1242fb640003c72e9082432fb3a76296b/Arrangement_on_surface_2/include/CGAL/Arr_segment_traits_2.h).
- **[N1]** GNU MPFR **4.2.2**, [§4.4, directed/correct rounding](https://www.mpfr.org/mpfr-4.2.2/mpfr.html#Rounding)
  and [§5, caller responsibility for accuracy](https://www.mpfr.org/mpfr-4.2.2/mpfr.html#MPFR-Interface).
- **[N2]** Python **3.14**, [`float.as_integer_ratio`](https://docs.python.org/3.14/library/stdtypes.html#float.as_integer_ratio)
  and [release-tag source for rational conversions](https://github.com/python/cpython/blob/v3.14.0/Lib/fractions.py).
- **[R1]** Published port [polygon input semantics](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/src/input.rs#L104-L114).
- **[R2]** Published [frozen GIS provenance](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/fixtures/gis/README.txt#L1-L39).
- **[R3]** Published [OBJ finite-segment extraction](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/tests/support/oracle.rs#L76-L140).
- **[R4]** Published [graph comparison](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/tests/support/graph_match.rs#L68-L139),
  [time-field omission](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/tests/support/graph_match.rs#L224-L233),
  and [existing differential-tolerance heuristic](https://github.com/atsyplenkov/surfer2-rs/blob/244944daf2f0fc2ba9e21b412e424257a1574800/tests/support/graph_match.rs#L753-L784).

[spec]: https://github.com/atsyplenkov/surfer2-rs/issues/1#issuecomment-6873800700
