//! THROWAWAY read-only probes for the first-invalid-event investigation.
use std::sync::OnceLock;

pub(crate) fn watches(source: Option<u32>) -> bool {
    static EDGES: OnceLock<Vec<u32>> = OnceLock::new();
    let edges = EDGES.get_or_init(|| {
        std::env::var("SURFER2_PROTOTYPE_EDGES")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect()
    });
    source.is_some_and(|s| edges.contains(&s))
}
