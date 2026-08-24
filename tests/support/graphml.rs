//! Dev-only GraphML reader for Surfer2 fixtures.
//!
//! Upstream attributes (BGLGraph): `vertex-coordinate-x/y`, `edge-weight`.
//! Additive weights are ignored. Handles `<edge …/>` and `<edge>…</edge>`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use surfer2::{InputEdge, Point2, Pslg};

#[derive(Debug, Clone)]
pub struct GraphMlInput {
    pub vertices: Vec<Point2>,
    pub edges: Vec<InputEdge>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitsRecord {
    pub xy_bits: Vec<[u64; 2]>,
    pub edges: Vec<[u64; 3]>,
}

pub fn read_graphml_path(path: &Path) -> Result<GraphMlInput, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut input = read_graphml_bytes(&bytes)?;
    input.name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    Ok(input)
}

fn local_end(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| *b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn attrs(e: &BytesStart<'_>) -> Result<BTreeMap<Vec<u8>, String>, String> {
    let mut m = BTreeMap::new();
    for a in e.attributes() {
        let a = a.map_err(|err| err.to_string())?;
        let k = a.key.local_name().as_ref().to_vec();
        let v = match a.unescape_value() {
            Ok(c) => c.into_owned(),
            Err(_) => String::from_utf8_lossy(&a.value).into_owned(),
        };
        m.insert(k, v);
    }
    Ok(m)
}

fn handle_key(e: &BytesStart<'_>, key_names: &mut BTreeMap<String, String>) -> Result<(), String> {
    let a = attrs(e)?;
    let id = a.get(b"id".as_slice()).cloned().unwrap_or_default();
    let nm = a
        .get(b"attr.name".as_slice())
        .cloned()
        .unwrap_or_default();
    if !id.is_empty() {
        key_names.insert(id, nm);
    }
    Ok(())
}

pub fn read_graphml_bytes(bytes: &[u8]) -> Result<GraphMlInput, String> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut key_names: BTreeMap<String, String> = BTreeMap::new();
    let mut vertex_order: Vec<String> = Vec::new();
    let mut vertex_xy: BTreeMap<String, (Option<f64>, Option<f64>)> = BTreeMap::new();
    let mut edges: Vec<(String, String, f64)> = Vec::new();

    let mut buf = Vec::new();
    let mut in_node = false;
    let mut current_node = String::new();
    let mut in_edge = false;
    let mut edge_source = String::new();
    let mut edge_target = String::new();
    let mut edge_weight = 1.0f64;
    let mut in_data = false;
    let mut data_key = String::new();
    let mut data_text = String::new();

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| e.to_string())?;
        match &event {
            Event::Start(e) => {
                let raw = e.name().as_ref().to_vec();
                match local_end(&raw) {
                    b"key" => handle_key(e, &mut key_names)?,
                    b"node" => {
                        in_node = true;
                        let a = attrs(e)?;
                        current_node = a.get(b"id".as_slice()).cloned().unwrap_or_default();
                        if !current_node.is_empty() {
                            vertex_order.push(current_node.clone());
                            vertex_xy.insert(current_node.clone(), (None, None));
                        }
                    }
                    b"edge" => {
                        in_edge = true;
                        let a = attrs(e)?;
                        edge_source = a.get(b"source".as_slice()).cloned().unwrap_or_default();
                        edge_target = a.get(b"target".as_slice()).cloned().unwrap_or_default();
                        edge_weight = 1.0;
                    }
                    b"data" => {
                        in_data = true;
                        data_text.clear();
                        let a = attrs(e)?;
                        data_key = a.get(b"key".as_slice()).cloned().unwrap_or_default();
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let raw = e.name().as_ref().to_vec();
                match local_end(&raw) {
                    b"key" => handle_key(e, &mut key_names)?,
                    b"edge" => {
                        let a = attrs(e)?;
                        let s = a.get(b"source".as_slice()).cloned().unwrap_or_default();
                        let t = a.get(b"target".as_slice()).cloned().unwrap_or_default();
                        edges.push((s, t, 1.0));
                    }
                    _ => {}
                }
            }
            Event::Text(t) => {
                if in_data {
                    let cow = t.decode().map_err(|e| e.to_string())?;
                    data_text.push_str(&cow);
                }
            }
            Event::End(e) => {
                let raw = e.name().as_ref().to_vec();
                match local_end(&raw) {
                    b"data" => {
                        let attr = key_names
                            .get(&data_key)
                            .cloned()
                            .unwrap_or_else(|| data_key.clone());
                        if in_node {
                            let entry = vertex_xy.get_mut(&current_node).ok_or_else(|| {
                                format!("data for unknown node {current_node}")
                            })?;
                            let val: f64 = data_text.parse().map_err(|err| {
                                format!("bad coordinate '{data_text}': {err}")
                            })?;
                            match attr.as_str() {
                                "vertex-coordinate-x" | "x" => entry.0 = Some(val),
                                "vertex-coordinate-y" | "y" => entry.1 = Some(val),
                                _ => {}
                            }
                        } else if in_edge && matches!(attr.as_str(), "edge-weight" | "w") {
                            edge_weight = data_text.parse().unwrap_or(1.0);
                        }
                        in_data = false;
                    }
                    b"node" => in_node = false,
                    b"edge" => {
                        edges.push((edge_source.clone(), edge_target.clone(), edge_weight));
                        in_edge = false;
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    let mut id_to_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut vertices = Vec::with_capacity(vertex_order.len());
    for id in &vertex_order {
        let (x, y) = vertex_xy.get(id).copied().unwrap_or((None, None));
        let x = x.ok_or_else(|| format!("missing x for vertex {id}"))?;
        let y = y.ok_or_else(|| format!("missing y for vertex {id}"))?;
        id_to_idx.insert(id.clone(), vertices.len());
        vertices.push(Point2::new(x, y));
    }

    let mut input_edges = Vec::with_capacity(edges.len());
    for (s, t, w) in edges {
        let u = *id_to_idx
            .get(&s)
            .ok_or_else(|| format!("unknown source {s}"))?;
        let v = *id_to_idx
            .get(&t)
            .ok_or_else(|| format!("unknown target {t}"))?;
        input_edges.push(InputEdge::new(u, v, w));
    }

    Ok(GraphMlInput {
        vertices,
        edges: input_edges,
        name: String::new(),
    })
}

impl GraphMlInput {
    pub fn to_pslg(&self) -> Result<Pslg, surfer2::InputError> {
        Pslg::new(self.vertices.clone(), self.edges.clone())
    }

    pub fn from_bits_record(bits: &BitsRecord, name: impl Into<String>) -> Self {
        let vertices: Vec<Point2> = bits
            .xy_bits
            .iter()
            .copied()
            .map(Point2::from_bits)
            .collect();
        let edges: Vec<InputEdge> = bits
            .edges
            .iter()
            .map(|e| InputEdge::new(e[0] as usize, e[1] as usize, f64::from_bits(e[2])))
            .collect();
        Self {
            vertices,
            edges,
            name: name.into(),
        }
    }

    pub fn to_bits_record(&self) -> BitsRecord {
        BitsRecord {
            xy_bits: self
                .vertices
                .iter()
                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|e| {
                    [
                        e.vertices[0] as u64,
                        e.vertices[1] as u64,
                        e.weight.to_bits(),
                    ]
                })
                .collect(),
        }
    }

    /// Dev-only Surfer2 GraphML (17-digit coords) for regenerating C++ OBJ oracles.
    pub fn to_graphml(&self) -> String {
        let mut out = String::new();
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\"\n");
        out.push_str("  xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n");
        out.push_str("  xsi:schemaLocation=\"http://graphml.graphdrawing.org/xmlns http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd\">\n");
        out.push_str("  <key attr.name=\"vertex-coordinate-x\" attr.type=\"string\" for=\"node\" id=\"x\"/>\n");
        out.push_str("  <key attr.name=\"vertex-coordinate-y\" attr.type=\"string\" for=\"node\" id=\"y\"/>\n");
        out.push_str("  <key attr.name=\"edge-weight\" attr.type=\"string\" for=\"edge\" id=\"w\">\n");
        out.push_str("    <default>1.0</default>\n");
        out.push_str("  </key>\n");
        out.push_str("  <key attr.name=\"edge-weight-additive\" attr.type=\"string\" for=\"edge\" id=\"wa\">\n");
        out.push_str("    <default>0.0</default>\n");
        out.push_str("  </key>\n");
        out.push_str("  <graph edgedefault=\"undirected\">\n");
        for (i, p) in self.vertices.iter().enumerate() {
            out.push_str(&format!("    <node id=\"{i}\">\n"));
            // Display is shortest round-trip for f64.
            out.push_str(&format!("      <data key=\"x\">{}</data>\n", p.x));
            out.push_str(&format!("      <data key=\"y\">{}</data>\n", p.y));
            out.push_str("    </node>\n");
        }
        for (i, e) in self.edges.iter().enumerate() {
            out.push_str(&format!(
                "    <edge id=\"e{i}\" source=\"{}\" target=\"{}\">\n",
                e.vertices[0], e.vertices[1]
            ));
            out.push_str(&format!("      <data key=\"w\">{}</data>\n", e.weight));
            out.push_str("    </edge>\n");
        }
        out.push_str("  </graph>\n</graphml>\n");
        out
    }
}
