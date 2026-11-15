//! THROWAWAY observer/reducer driver for "Locate the first invalid event in the
//! residual basin failures". No engine policy changes. JSON-lines in/out.
use serde::Deserialize;
use serde_json::{json, Value};
use slotmap::Key;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead};
use surfer2::event::CollapseType;
use surfer2::triangulation::{KineticTriangulation, VertexId, WavefrontVertex};
use surfer2::{Point2, Polygon, Propagator};

#[derive(Deserialize)]
struct Request {
    rings: Vec<Vec<[u64; 2]>>,
    #[serde(default)]
    indices: Vec<Vec<usize>>,
    #[serde(default)]
    trace: bool,
    #[serde(default)]
    force_event: Option<ForceEvent>,
    #[serde(default)]
    pin_pair: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct ForceEvent {
    constraints: Vec<Value>,
    collapse: Option<Value>,
    time: f64,
}

fn p(p: Point2) -> Value {
    json!([p.x, p.y])
}

fn bits(p: Point2) -> Value {
    json!([p.x.to_bits(), p.y.to_bits()])
}

fn vid(id: VertexId) -> u64 {
    id.data().as_ffi()
}

fn vertex(kt: &KineticTriangulation, id: VertexId, v: &WavefrontVertex) -> Value {
    json!({
        "id": vid(id), "origin": p(v.pos_zero), "origin_bits": bits(v.pos_zero),
        "birth": p(v.pos_start), "birth_bits": bits(v.pos_start),
        "velocity": p(v.velocity), "velocity_bits": bits(v.velocity),
        "birth_time": v.time_start, "birth_time_bits": v.time_start.to_bits(),
        "stop": v.pos_stop.map(p), "stop_time": v.time_stop,
        "infinite_speed": format!("{:?}", v.infinite_speed),
        "initial": v.is_initial, "source_vertex": v.source_input_vertex,
        "angle": format!("{:?}", v.angle),
        "prev": v.prev_vertex.map(|x| x.map(vid)),
        "next": v.next_vertex.map(|x| x.map(vid)),
        "edges": v.incident_edges.map(|x| x.map(|e| e.data().as_ffi())),
        "supports": v.incident_edges.map(|x| x.map(|e| {
            let e = &kt.edges[e];
            json!({"face":e.skeleton_face, "input_edge":e.source_input_edge,
                "point":p(e.supporting_line.point),
                "direction":p(e.supporting_line.direction),
                "normal":p(e.supporting_line.normal),
                "weight":e.supporting_line.weight})
        })),
    })
}

fn run(mut req: Request) -> Value {
    if req.indices.is_empty() {
        req.indices = req.rings.iter().map(|r| (0..r.len()).collect()).collect();
    }
    let mut xy = Vec::new();
    let mut offsets = Vec::new();
    let mut input_edges = Vec::new();
    for (ri, ring) in req.rings.iter().enumerate() {
        offsets.push(xy.len());
        for (i, b) in ring.iter().enumerate() {
            xy.push([f64::from_bits(b[0]), f64::from_bits(b[1])]);
            input_edges.push(json!([ri, req.indices[ri][i], req.indices[ri][(i+1)%ring.len()]]));
        }
    }
    let polygon = match Polygon::from_flat(&xy, &offsets) {
        Ok(p) => p,
        Err(e) => return json!({"status":"invalid_input","error":e.to_string()}),
    };
    let mut prop = match Propagator::from_polygon(&polygon) {
        Ok(p) => p,
        Err(e) => return json!({"status":"initialization_error","error":e.to_string()}),
    };
    let mut births = Vec::new();
    let mut meetings = Vec::new();
    let mut events = Vec::new();
    let mut initial = Vec::new();
    if req.trace {
        initial = prop.kt.vertices.iter().map(|(id,v)|vertex(&prop.kt,id,v)).collect();
    }
    let mut step = 0;
    let mut interventions=Vec::new();
    loop {
        let mut event = prop.queue.peak_event().unwrap();
        if interventions.is_empty() {
            if let (Some(force),Some((tidx,mut spec)))=(&req.force_event,event) {
                let t=&prop.kt.triangles[prop.kt.triangle_by_tidx(tidx).unwrap()];
                let sources:Vec<_>=t.wavefronts.iter().flatten().filter_map(|e|prop.kt.edges[*e].source_input_edge)
                    .map(|i|input_edges[i as usize].clone()).collect();
                if sources.len()==force.constraints.len() && force.constraints.iter().all(|s|sources.contains(s)) {
                    let old=format!("{:?}",spec);
                    spec.time=force.time;
                    if let Some(collapse)=&force.collapse {
                        spec.type_=CollapseType::ConstraintCollapse;
                        spec.relevant_edge=t.wavefronts.iter().position(|e|e.and_then(|e|prop.kt.edges[e].source_input_edge)
                            .is_some_and(|i|input_edges[i as usize]==*collapse)).unwrap() as i8;
                    }
                    prop.queue.seed(tidx,spec);
                    event=prop.queue.peak_event().unwrap();
                    interventions.push(json!({"step":step,"old":old,"new":format!("{:?}",spec)}));
                }
            }
        }
        let before_len = prop.kt.vertices.len();
        let mut event_json = Value::Null;
        if let Some((tidx, spec)) = event {
            if spec.type_ != CollapseType::Never {
                let t = &prop.kt.triangles[prop.kt.triangle_by_tidx(tidx).unwrap()];
                let ids: Vec<_> = t.vertices.into_iter().flatten().collect();
                let ps: Vec<_> = ids.iter().map(|v| {
                    let v=&prop.kt.vertices[*v];
                    Point2::new(v.pos_zero.x + spec.time*v.velocity.x,
                                v.pos_zero.y + spec.time*v.velocity.y)
                }).collect();
                let mut spread: f64 = 0.0;
                if spec.type_ == CollapseType::TriangleCollapse {
                    for a in &ps { for b in &ps { spread=spread.max(a.distance_to(*b)); } }
                } else if matches!(spec.type_,CollapseType::ConstraintCollapse|CollapseType::SpokeCollapse) {
                    let i=spec.relevant_edge as usize;
                    spread=ps[(i+1)%3].distance_to(ps[(i+2)%3]);
                }
                let faces: Vec<_> = t.wavefronts.iter().flatten()
                    .map(|e| prop.kt.edges[*e].source_input_edge.map(|i| input_edges[i as usize].clone())).collect();
                event_json=json!({"step":step,"time":spec.time,"kind":format!("{:?}",spec.type_),
                    "triangle":tidx,"side":spec.relevant_edge,"vertices":ids.iter().map(|v|vid(*v)).collect::<Vec<_>>(),
                    "constraints":faces,"spread":spread,"positions":ps.iter().copied().map(p).collect::<Vec<_>>()});
                if step <= 7 {
                    event_json["active_vertices"] = json!(prop.kt.vertices.iter().map(|(id,v)|vertex(&prop.kt,id,v)).collect::<Vec<_>>());
                    event_json["active_edges"] = json!(prop.kt.edges.iter().filter(|(_,e)| !e.dead).map(|(id,e)| json!({
                        "id": id.data().as_ffi(), "source": e.source_input_edge.map(|i|input_edges[i as usize].clone()),
                        "endpoints": e.endpoints.map(vid),
                        "incident_triangle": e.incident_triangle.map(|t| prop.kt.triangles[t].tidx),
                    })).collect::<Vec<_>>());
                    event_json["live_triangles"] = json!(prop.kt.triangles.iter().filter(|(_,t)| t.live()).map(|(_,t)| json!({
                        "triangle": t.tidx, "vertices": t.vertices.map(|v|v.map(vid)),
                        "wavefronts": t.wavefronts.map(|e| e.map(|e|e.data().as_ffi())),
                    })).collect::<Vec<_>>());
                }
                // Selection thresholds only: neither is an acceptance tolerance.
                if spread > 0.005 {
                    let mut record=event_json.clone();
                    record["states"]=json!(ids.iter().map(|v|vertex(&prop.kt,*v,&prop.kt.vertices[*v])).collect::<Vec<_>>());
                    meetings.push(record);
                }
            }
        }
        match prop.step() {
            Ok(keep_going) => {
                if let Some(pair)=&req.pin_pair {
                    let ids:Vec<_>=prop.kt.vertices.keys().skip(before_len).collect();
                    for id in ids {
                        let v=&prop.kt.vertices[id];
                        let original:Vec<_>=v.incident_edges.into_iter().flatten().filter_map(|e|prop.kt.edges[e].source_input_edge)
                            .map(|i|input_edges[i as usize].clone()).collect();
                        if original==*pair {
                            let v=&mut prop.kt.vertices[id];
                            let old=v.pos_zero;
                            v.pos_zero=Point2::new(v.pos_start.x-v.time_start*v.velocity.x,v.pos_start.y-v.time_start*v.velocity.y);
                            interventions.push(json!({"step":step,"pin_birth":vid(id),"old_origin":p(old),"new_origin":p(v.pos_zero)}));
                        }
                    }
                }
                for (id,v) in prop.kt.vertices.iter().skip(before_len) {
                    let replay=Point2::new(v.pos_zero.x+v.time_start*v.velocity.x,
                                          v.pos_zero.y+v.time_start*v.velocity.y);
                    let miss=replay.distance_to(v.pos_start);
                    if miss>0.005 || req.trace {
                        let mut record=vertex(&prop.kt,id,v);
                        record["step"]=json!(step);
                        record["event"]=event_json.clone();
                        record["birth_miss"]=json!(miss);
                        record["original_supports"]=json!(v.incident_edges.map(|e|e.and_then(|e|prop.kt.edges[e].source_input_edge).map(|i|input_edges[i as usize].clone())));
                        births.push(record);
                    }
                }
                if req.trace { events.push(event_json); }
                if !keep_going { break; }
            }
            Err(e) => return json!({"status":"propagation_error","error":e.to_string(),"births":births,"meetings":meetings}),
        }
        step += 1;
    }
    let faces: Vec<_>=prop.kt.faces.iter().map(|f|f.source_input_edge.map(|i|input_edges[i as usize].clone())).collect();
    let final_vertices = if req.trace {json!(prop.kt.vertices.iter().map(|(id,v)|vertex(&prop.kt,id,v)).collect::<Vec<_>>())} else {Value::Null};
    let arcs = if req.trace {json!(prop.kt.arcs.iter().map(|a| json!({
        "start":p(a.start),"end":a.end.map(p),"start_time":a.start_time,"end_time":a.end_time,
        "left":a.left_face,"right":a.right_face,
        "start_end":format!("{:?}",a.start_end),"end_end":format!("{:?}",a.end_end),
        "traced_vertex":a.traced_vertex().map(vid)})).collect::<Vec<_>>())} else {Value::Null};
    let mut out=json!({"events_processed":prop.stats.events_processed,"interventions":interventions,
        "births":births,"meetings":meetings,"initial":initial,"events":events,
        "vertices":final_vertices,"arcs":arcs,"faces":faces});
    match prop.into_skeleton() {
        Err(e) => {
            let err=format!("{e:?}");
            out["status"]=json!("dcel_error");
            if let Some(part)=err.split("face=").nth(1) {
                if let Ok(fi)=part.split(':').next().unwrap().parse::<usize>() {
                    out["failed_constraint"]=json!(faces[fi]);
                }
            }
            out["error"]=json!(err);
        }
        Ok(sk) => {
            let mut nodes=HashMap::new();
            let mut parent=Vec::new();
            let mut edges=0;
            fn root(p:&mut [usize],mut i:usize)->usize {while p[i]!=i {p[i]=p[p[i]];i=p[i];}i}
            for s in sk.finite_segments() {
                let mut index=|pt:Point2|*nodes.entry((pt.x.to_bits(),pt.y.to_bits())).or_insert_with(|| {let i=parent.len();parent.push(i);i});
                let a=index(s.start);let b=index(s.end);
                let a=root(&mut parent,a);let b=root(&mut parent,b);parent[a]=b;edges+=1;
            }
            let c=(0..parent.len()).map(|i|root(&mut parent,i)).collect::<HashSet<_>>().len();
            out["status"]=json!(if c==1 {"connected"} else {"disconnected"});
            out["graph"]=json!({"components":c,"edges":edges,"nodes":nodes.len(),"rank":edges as i64-nodes.len() as i64+c as i64,
                "stored_segments":sk.stored_segments().len()});
        }
    }
    out
}

fn main() {
    for line in io::stdin().lock().lines() {
        let req: Request=serde_json::from_str(&line.unwrap()).unwrap();
        println!("{}",run(req));
    }
}
