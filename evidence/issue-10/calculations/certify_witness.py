#!/usr/bin/env python3
"""Reproduce bounded source geometry; this is diagnostic code, not a repair.

Run with --restored pointing to the issue-5 restored checkout and --traces to
the issue-10 evidence directory. Python 3 and the archived certify.py suffice.
All interval decimal endpoints are outward rounded; event trace values are
binary64 observations, not a certified replay of the entire wavefront.
"""
import argparse
import hashlib
import importlib.util
import json
from fractions import Fraction as Q
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--restored", type=Path, required=True)
ap.add_argument("--traces", type=Path, required=True)
args = ap.parse_args()
root, traces = args.restored.resolve(), args.traces.resolve()
base = root / "evidence/issue-5/session-resume"
archive = base / "ancestry-preserving"
fixture = root / "fixtures/projected/pl_basins-2.json"
cert_path = root / "evidence/issue-5/certify.py"
full_path = base / "reported-collapse/full-watched.json"
request_path = archive / "representative-request.ndjson"
spec = importlib.util.spec_from_file_location("archived_certifier", cert_path)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
full = json.loads(full_path.read_text())
request = json.loads(request_path.read_text())
fixture_data = json.loads(fixture.read_text())
ids = request["indices"][0]
assert ids == list(range(594, 604)) + [1143, 3151]
assert len(fixture_data) == 1 and len(fixture_data[0]) == 3358
assert request["rings"][0] == [fixture_data[0][i] for i in ids]
# Recheck distinct vertices, winding and simplicity using the input dyadics on
# a common integer grid. This is an input predicate, not a wavefront replay.
qpoints=[m.decode(p) for p in fixture_data[0]]
den=max(x.denominator for p in qpoints for x in p)
assert all(den % x.denominator == 0 for p in qpoints for x in p)
grid=[tuple(x.numerator*(den//x.denominator) for x in p) for p in qpoints]
poly=[grid[i] for i in ids]
def cross(a,b,c):
    return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
def area(points):
    return sum(a[0]*b[1]-a[1]*b[0] for a,b in zip(points,points[1:]+points[:1]))
def on(a,b,p):
    return cross(a,b,p)==0 and all(min(a[k],b[k])<=p[k]<=max(a[k],b[k]) for k in (0,1))
assert len(set(poly))==len(poly) and area(poly)*area(grid)>0
segments=list(zip(poly,poly[1:]+poly[:1]))
for i,(a,b) in enumerate(segments):
    for c,d in segments[i+1:]:
        if any(max(a[k],b[k])<min(c[k],d[k]) or max(c[k],d[k])<min(a[k],b[k]) for k in (0,1)):
            continue
        o=(cross(a,b,c),cross(a,b,d),cross(c,d,a),cross(c,d,b))
        assert not (o[0]*o[1]<0 and o[2]*o[3]<0)
        hits={p for p in (a,b,c,d) if on(a,b,p) and on(c,d,p)}
        assert len(hits)<=1 and hits<=({a,b}&{c,d})
relevant=set(range(594,604))
for v in full["initial"]:
    for sup in v["supports"]:
        if sup and sup["input_edge"] in relevant:
            assert sup["weight"]==1.0
lines = m.setup(2, full)  # exact dyadic endpoint/direction equality is asserted
def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
def shown(x):
    return m.interval(x).shown(20)
def vec_shown(v):
    return [shown(x) for x in v]
def pos(pair, t):
    o, v = m.motion(lines, pair)
    return [o[k] + t*v[k] for k in (0, 1)]
def vec_sub(a, b):
    return [a[k] - b[k] for k in (0, 1)]
def arrival(pair, third):
    return m.arrival(lines, pair, third)
def fraction_on_x(p, a, b):
    return (p[0] - a[0]) / (b[0] - a[0])
def supportpair(v):
    return [ids[x["input_edge"]] for x in v["supports"]]
def exact_position(v, t):
    return [Q(v["origin"][k]) + Q(t)*Q(v["velocity"][k]) for k in (0, 1)]
def birth_case(v):
    t = Q(v["birth_time"])
    rec = [Q(x) for x in v["birth"]]
    exact = exact_position(v, t)
    # CPython evaluates these two float operations separately, without FMA.
    # This quantifies this replay's evaluation rounding; compare the trace's
    # recorded birth_miss separately, and do not claim a general error budget.
    rounded = [v["origin"][k] + v["birth_time"]*v["velocity"][k] for k in (0,1)]
    pp = supportpair(v)
    source = pos(pp, t)
    return {
        "supports": pp,
        "time_observed": v["birth_time"],
        "id": v["id"],
        "birth_miss_observer": v["birth_miss"],
        "exact_dyadic_motion_minus_recorded_birth": vec_shown(vec_sub(exact, rec)),
        "rounded_non_fma_replay_minus_recorded_birth": vec_shown(vec_sub([Q(x) for x in rounded], rec)),
        "exact_dyadic_motion_minus_rounded_non_fma_replay": vec_shown(vec_sub(exact, [Q(x) for x in rounded])),
        "source_support_intersection_minus_recorded_birth": vec_shown(vec_sub(source, rec)),
        "recorded_birth_signed_distance_from_supports": {
            str(i): shown(m.dist_to_line(lines, i, [m.I(x) for x in rec], t)) for i in pp
        },
    }
def event_case(e, vertices):
    t = Q(e["time"])
    rows = []
    for vid, logged in zip(e["vertices"], e["positions"]):
        v = vertices[vid]
        pair = supportpair(v)
        try:
            source = pos(pair, t)
        except (KeyError, AssertionError):
            continue
        exact = exact_position(v, t)
        rows.append({
            "id": vid, "supports": pair, "logged_position": logged,
            "exact_dyadic_motion_minus_original_support_motion":
                vec_shown(vec_sub(exact, source)),
            "exact_dyadic_motion_minus_logged_float_position":
                vec_shown(vec_sub(exact, [Q(x) for x in logged])),
        })
    return {"step": e["step"], "time_observed": e["time"], "kind": e["kind"],
            "side": e["side"], "spread_observer": e["spread"], "vertices": rows}
def find_birth(trace, step, pair):
    matches = [b for b in trace["births"] if b["step"] == step
               and set(supportpair(b)) == set(pair)]
    assert len(matches) == 1, (step, pair, len(matches))
    return matches[0]
def same_pair(v, pair):
    return set(supportpair(v)) == set(pair)
def frac_at_source_split(parent, target, endpoints):
    t, p = arrival(parent, target)
    a, b = [pos(pair, t) for pair in endpoints]
    f = fraction_on_x(p, a, b)
    assert 0 < f.lo <= f.hi < 1
    return t, p, f
# Exact input predicates here certify bit identity, distinct non-collinear
# support pairs, and strict interior fractions in the conditional source model.
# The separate inherited witness check certifies polygon simplicity.
for a,b in [(599,598),(598,597),(599,597),(600,599),(597,596),
            (597,600),(597,601),(601,600),(601,602),(594,595),
            (601,596),(595,601),(595,596)]:
    l1,l2=lines[a],lines[b]
    det=l1[0]*l2[1]-l2[0]*l1[1]
    assert det != 0, (a,b)
t0,p0 = arrival((599,598),597)
t1,p1 = arrival((600,599),597)
t_compete,_ = arrival((597,596),599)
t4,p4,f4 = frac_at_source_split((597,596),601,((601,600),(601,602)))
t5,p5 = arrival((597,600),601)
t6,p6,f6 = frac_at_source_split((594,595),601,((601,602),(601,596)))
t7,p7 = arrival((595,596),601)
assert t0.hi < t1.lo and t1.hi < t_compete.lo and t4.hi < t5.lo and t6.hi < t7.lo
# The source-line edge 597 points from the 596 intersection toward the 598
# intersection initially. Check that the post-event endpoint 600/597 is on
# that same side at the time of the source split. This is a local sign check,
# not an independent proof that those two endpoints are active.
a,b,_,_ = lines[597]
d=(b,-a)
def edge597_coeff(pair,t):
    delta=vec_sub(pos(pair,t),pos((597,596),t))
    return (delta[0]*d[0]+delta[1]*d[1])/(d[0]*d[0]+d[1]*d[1])
assert edge597_coeff((597,598),m.I(0)).hi < 0
assert edge597_coeff((597,600),t1).hi < 0
assert edge597_coeff((597,600),t4).hi < 0
# 601's original endpoint on support 600 is not consumed by the 601/600/602
# endpoint event before the early split. This does not test unrelated edges.
t_target,_ = arrival((601,600),602)
assert t4.hi < t_target.lo
out = {
    "method": "exact binary64 dyadics; archived certify.py exact rational interval arithmetic and 2^-192 square-root enclosures",
    "sha256": {"fixture": sha(fixture), "source_certifier": sha(cert_path),
               "full_trace_for_original_orientations": sha(full_path),
               "request": sha(request_path)},
    "request_original_indices": ids,
    "input_predicates": {"all_request_bits_match_original":True,
                         "winding_preserved":True, "distinct_vertices":True,
                         "simple_no_proper_crossing_or_collinear_overlap":True,
                         "relevant_original_support_weights_are_one":True,
                         "synthetic_edges":[[603,1143],[1143,3151],[3151,594]]},
    "source_model": {
        "first_597_598_599_meeting_time": shown(t0),
        "next_597_599_600_meeting_time": shown(t1),
        "time_first_to_next": shown(t1-t0),
        "competing_597_596_599_support_meeting_time": shown(t_compete),
        "competing_support_meeting_minus_next": shown(t_compete-t1),
        "edge_597_signed_coefficient_at_next": shown(edge597_coeff((597,600),t1)),
        "edge_597_endpoint_vector_596_to_600_at_next": vec_shown(vec_sub(pos((597,600),t1),pos((597,596),t1))),
        "early_597_596_hits_601_time": shown(t4),
        "early_597_596_hits_601_point": vec_shown(p4),
        "early_fraction_from_601_600_toward_601_602": shown(f4),
        "early_split_to_597_600_601_stop_time": shown(t5-t4),
        "early_597_600_601_stop_time": shown(t5),
        "early_597_600_601_stop_point": vec_shown(p5),
        "edge_597_initial_signed_coefficient": shown(edge597_coeff((597,598),m.I(0))),
        "edge_597_post_collapse_signed_coefficient_at_source_split": shown(edge597_coeff((597,600),t4)),
        "edge_597_endpoint_vector_600_to_596_at_source_split": vec_shown(vec_sub(pos((597,596),t4),pos((597,600),t4))),
        "edge_601_endpoint_vector_600_to_602_at_source_split": vec_shown(vec_sub(pos((601,602),t4),pos((601,600),t4))),
        "edge_601_600_602_meeting_time": shown(t_target),
        "later_594_595_hits_601_time": shown(t6),
        "later_594_595_hits_601_point": vec_shown(p6),
        "later_fraction_from_601_602_toward_601_596": shown(f6),
        "later_595_596_601_stop_time": shown(t7),
        "later_split_to_stop_time": shown(t7-t6),
    },
    "cases": {},
}
for name in ("baseline","intervention","control"):
    path=traces / (name+".json")
    trace=json.loads(path.read_text())
    vertices={v["id"]:v for v in trace["vertices"]}
    events=trace["events"]
    ev0=events[0]
    assert ev0["kind"] == "ConstraintCollapse"
    assert same_pair(vertices[ev0["vertices"][0]], (599,598))
    assert same_pair(vertices[ev0["vertices"][1]], (598,597))
    later = next(e for e in events if e["kind"]=="SplitOrFlipRefine"
                 and e["side"] in (0,1,2)
                 and same_pair(vertices[e["vertices"][e["side"]]],(594,595)))
    final = next(e for e in events if e["kind"]=="TriangleCollapse"
                 and {frozenset(supportpair(vertices[v])) for v in e["vertices"]}
                     == {frozenset((595,596)),frozenset((596,601)),frozenset((595,601))})
    row={"trace_sha256":sha(path), "first_event":event_case(ev0,vertices),
         "first_event_observed_minus_source_time":shown(Q(ev0["time"])-t0),
         "first_birth":birth_case(find_birth(trace,0,(599,597))),
         "later_split":event_case(later,vertices),
         "later_split_observed_minus_source_time":shown(Q(later["time"])-t6),
         "later_birth_fast":birth_case(find_birth(trace,later["step"],(595,601))),
         "later_stop":event_case(final,vertices),
         "later_stop_observed_minus_source_time":shown(Q(final["time"])-t7)}
    if name in ("baseline","intervention"):
        e1,e2,e3,e4,e5=events[1:6]
        assert [e["kind"] for e in events[:6]] == ["ConstraintCollapse","ConstraintCollapse",
                "SplitOrFlipRefine","VertexMovesOverSpoke","SplitOrFlipRefine","TriangleCollapse"]
        assert [set(supportpair(vertices[v])) for v in e4["vertices"]] == [
            {600,601},{596,597},{601,602}]
        assert [set(supportpair(vertices[v])) for v in e5["vertices"]] == [
            {597,600},{597,601},{600,601}]
        assert e4["side"]==1
        t4obs=Q(e4["time"])
        true_child=pos((597,601),t4obs)
        true_old_end=pos((600,601),t4obs)
        row.update({
            "next_event":event_case(e1,vertices),
            "next_event_observed_minus_source_time":shown(Q(e1["time"])-t1),
            "next_birth":birth_case(find_birth(trace,1,(597,600))),
            "local_predecessor_events":[event_case(e,vertices) for e in (e2,e3)],
            "early_split":event_case(e4,vertices),
            "early_split_observed_minus_source_split_time":shown(t4obs-t4),
            "early_split_observed_minus_source_collapse_time":shown(t4obs-t5),
            "early_split_source_child_minus_old_601_600_endpoint":vec_shown(vec_sub(true_child,true_old_end)),
            "early_birth_597_601":birth_case(find_birth(trace,4,(597,601))),
            "early_birth_596_601":birth_case(find_birth(trace,4,(596,601))),
            "early_stop":event_case(e5,vertices),
            "early_stop_observed_minus_source_collapse_time":shown(Q(e5["time"])-t5),
        })
    out["cases"][name]=row
# Compare the added pre-event topology snapshots with the unmodified baseline.
# Equality without these fields establishes non-interference of recorded JSON,
# not proof that the simulation's own active set is mathematically correct.
active_path=traces/"active.json"
active=json.loads(active_path.read_text())
baseline=json.loads((traces/"baseline.json").read_text())
added={"active_edges","active_vertices","live_triangles"}
def strip_added(x):
    if isinstance(x,dict):
        return {k:strip_added(v) for k,v in x.items() if k not in added}
    if isinstance(x,list):
        return [strip_added(v) for v in x]
    return x
assert strip_added(active)==baseline

def edge_snapshot(e):
    verts={v["id"]:v for v in e["active_vertices"]}
    assert all(v in verts for v in e["vertices"])
    triangle=[t for t in e["live_triangles"] if t["triangle"]==e["triangle"]]
    assert len(triangle)==1 and triangle[0]["vertices"]==e["vertices"]
    result=[]
    for edge in e["active_edges"]:
        if edge["source"][1] not in (595,596,597,598,599,600,601):
            continue
        pairs=[supportpair(verts[v]) for v in edge["endpoints"]]
        result.append({"source":edge["source"][1],"id":edge["id"],
                       "endpoints":edge["endpoints"],"endpoint_supports":pairs,
                       "incident_triangle":edge["incident_triangle"]})
    return triangle[0],result
def require_edge(edges,src,pairs,triangle=None):
    required={frozenset(p) for p in pairs}
    matches=[e for e in edges if e["source"]==src
             and {frozenset(p) for p in e["endpoint_supports"]}==required]
    assert len(matches)==1, (src,pairs,matches)
    if triangle is not None:
        assert matches[0]["incident_triangle"]==triangle
    return matches[0]
lifecycle=[]
for e in active["events"][:8]:
    tri,edges=edge_snapshot(e)
    step=e["step"]
    if step==0:
        require_edge(edges,598,((599,598),(598,597)),e["triangle"])
    if step==1:
        require_edge(edges,599,((600,599),(599,597)),e["triangle"])
    if step in (2,3,4):
        require_edge(edges,597,((600,597),(597,596)))
        require_edge(edges,600,((601,600),(600,597)))
        require_edge(edges,601,((602,601),(601,600)),4)
    if step==5:
        require_edge(edges,597,((600,597),(597,601)),e["triangle"])
        require_edge(edges,600,((601,600),(600,597)),e["triangle"])
        require_edge(edges,601,((597,601),(601,600)),e["triangle"])
        require_edge(edges,601,((602,601),(601,596)))
    if step==6:
        require_edge(edges,601,((602,601),(601,596)),e["triangle"])
    if step==7:
        require_edge(edges,595,((596,595),(595,601)),e["triangle"])
        require_edge(edges,596,((601,596),(596,595)),e["triangle"])
        require_edge(edges,601,((595,601),(601,596)),e["triangle"])
    lifecycle.append({"step":step,"kind":e["kind"],"time":e["time"],
                      "event_triangle":tri,"relevant_active_edges":edges})
out["lifecycle_observer"]={"trace_sha256":sha(active_path),
                            "baseline_identical_after_removing_observer_fields":True,
                            "snapshots":lifecycle}
print(json.dumps(out,indent=2))
