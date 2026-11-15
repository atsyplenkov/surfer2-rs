#!/usr/bin/env python3
"""Throwaway comparison of the single-motion experiment; not a repair oracle."""
import json
import re
from fractions import Fraction
from pathlib import Path

D = Path(__file__).resolve().parent
INDICES = [594, 595, 596, 597, 598, 599, 600, 601, 602, 603, 1143, 3151]


def supports(v):
    return sorted(INDICES[s['input_edge']] for s in v['supports'] if s)


def event(trace, sources):
    return next(e for e in trace['events'] if e and e['kind'] == 'TriangleCollapse'
                and sorted(s[1] for s in e['constraints']) == sorted(sources))


def summarize(name):
    trace = json.loads((D / f'{name}.json').read_text())
    log = (D / f'{name}.log').read_text()
    vertices = {v['id']: v for v in trace['vertices']}
    last = event(trace, [595, 596, 601])
    final = [vertices[i] for i in last['vertices']]
    fast = next(v for v in final if supports(v) == [595, 601])
    fast_birth = next(v for v in trace['births'] if v['id'] == fast['id'])
    slow = next(v for v in final if supports(v) == [596, 601])
    maps = re.findall(r'endpoint-map start=(.*?) end=(.*?) nodes=NodeId\((\d+)\)/NodeId\((\d+)\)', log)
    arcs = [a for a in trace['arcs'] if a['traced_vertex'] in last['vertices']]
    for a in arcs:
        a['emitted_nodes'] = [[int(n0), int(n1)] for start, end, n0, n1 in maps
                              if start == a['start_end'] and end == a['end_end']]
        assert len(a['emitted_nodes']) == 1
    faces = []
    for section in log.split('[DEBUG-wayfinder] emitted-face ')[1:]:
        header, *lines = section.splitlines()
        face, source = map(int, re.match(r'face=(\d+) source=Some\((\d+)\)', header).groups())
        halfedges = [l for l in lines if '[DEBUG-wayfinder] halfedge ' in l]
        faces.append({'face': face, 'original_support': INDICES[source],
                      'header': header, 'halfedges': halfedges,
                      'infinity_chords': [l for l in halfedges if 'geometry=InfinityChord' in l]})
    pairs = {tuple(supports(v)): v for v in final}
    quotients = []
    for edge, p0, p1 in [(596, (596, 601), (595, 596)),
                         (595, (595, 596), (595, 601)),
                         (601, (595, 601), (596, 601))]:
        a, b = pairs[p0], pairs[p1]
        raw, exact = [], []
        for k in (0, 1):
            raw.append((b['origin'][k] - a['origin'][k]) / (a['velocity'][k] - b['velocity'][k]))
            exact.append((Fraction(b['origin'][k]) - Fraction(a['origin'][k])) /
                         (Fraction(a['velocity'][k]) - Fraction(b['velocity'][k])))
        quotients.append({'support': edge, 'x_f64': raw[0], 'y_f64': raw[1],
                          'exact_dyadic_x_minus_y': str(exact[0] - exact[1]),
                          'x_relative_speed': a['velocity'][0] - b['velocity'][0]})
    return {'status': trace['status'], 'error': trace.get('error'), 'graph': trace.get('graph'),
            'predicted_scalar_times': quotients,
            'final_event': last, 'slow_motion': {k: slow[k] for k in ['origin_bits', 'velocity_bits', 'velocity']},
            'fast_birth': {k: fast_birth[k] for k in ['birth', 'birth_time', 'birth_miss', 'origin_bits', 'velocity_bits']},
            'final_stop_nodes': [a['emitted_nodes'][0][1] for a in arcs],
            'final_arcs': arcs, 'watched_faces': faces}


def main():
    request = json.loads((D / 'representative-request.ndjson').read_text())
    assert request['indices'] == [INDICES]
    assert 'force_event' not in request and 'pin_pair' not in request
    traces = {n: json.loads((D / f'{n}.json').read_text()) for n in ['baseline', 'intervention', 'control']}
    a, b = traces['baseline'], traces['intervention']
    va, vb = [{v['id']: v for v in t['vertices']} for t in [a, b]]
    common = va.keys() & vb.keys()
    changed = [i for i in common if va[i]['velocity_bits'] != vb[i]['velocity_bits']]
    assert len(changed) == 1 and supports(va[changed[0]]) == [596, 601]
    assert va[changed[0]]['origin_bits'] == vb[changed[0]]['origin_bits']
    assert a['initial'] == b['initial']
    assert a['events'][:6] == b['events'][:6]
    assert a['interventions'] == b['interventions'] == []
    assert (D / 'intervention.log').read_text().count('[DEBUG-issue10] slow-motion') == 1
    result = {n: summarize(n) for n in traces}
    result['isolation'] = {'only_changed_velocity_vertex': changed[0],
                           'same_initial_vertices': True, 'same_events_through_earlier_stop': True,
                           'removed_finalization_vertex_ids': sorted(va.keys() - vb.keys()),
                           'new_vertex_ids': sorted(vb.keys() - va.keys()),
                           'no_direct_time_or_birth_override': True}
    (D / 'comparison.json').write_text(json.dumps(result, indent=2) + '\n')
    for name in traces:
        r = result[name]
        print(name, r['status'], r['error'] or r['graph'])
        print('  split time:', r['fast_birth']['birth_time'], 'birth replay miss:', r['fast_birth']['birth_miss'])
        print('  final time:', r['final_event']['time'], 'spread:', r['final_event']['spread'],
              'stop nodes:', r['final_stop_nodes'])
        print('  watched chords:', [(f['original_support'], len(f['infinity_chords']))
                                     for f in r['watched_faces'] if f['infinity_chords']])
    print('isolation checks passed:', result['isolation'])


if __name__ == '__main__':
    main()
