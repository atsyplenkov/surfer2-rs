//! Structured errors for the public API.
//!
//! Translated from Surfer2 assert/exit failure modes into recoverable enums.
//! Upstream: various `assert` / `exit` sites in `surf/src/*`.

use thiserror::Error;

/// Invalid planar straight-line graph or polygon input.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum InputError {
    #[error("input is empty")]
    Empty,

    #[error("non-finite coordinate at vertex {index}")]
    NonFiniteCoordinate { index: usize },

    #[error("vertex index {index} out of range (vertex count {vertex_count})")]
    VertexIndexOutOfRange { index: usize, vertex_count: usize },

    #[error("edge {edge} is degenerate (zero length) between vertices {v0} and {v1}")]
    ZeroLengthEdge { edge: usize, v0: usize, v1: usize },

    #[error("edge {edge} has non-positive weight {weight}")]
    NonPositiveWeight { edge: usize, weight: f64 },

    #[error("duplicate consecutive vertices in ring {ring} at offset {offset}")]
    DuplicateConsecutiveVertices { ring: usize, offset: usize },

    #[error("self-intersecting or improperly nested rings (edge {e0} vs {e1})")]
    CrossingConstraints { e0: usize, e1: usize },

    #[error("hole {hole} is not strictly inside the exterior")]
    HoleOutsideExterior { hole: usize },

    #[error("touching or shared boundary between rings is unsupported")]
    TouchingRings,

    #[error("polygon interior is empty or not a single connected component")]
    InvalidPolygonInterior,

    #[error("duplicate input edge between vertices {v0} and {v1}")]
    DuplicateEdge { v0: usize, v1: usize },

    #[error("ring {ring} has fewer than 3 distinct vertices")]
    RingTooSmall { ring: usize },
}

/// Failures during constrained Delaunay construction or region labeling.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum TriangulationError {
    #[error("CDT vertex insertion failed at input vertex {index}")]
    VertexInsertion { index: usize },

    #[error("CDT constraint insertion failed for input edge {edge}")]
    ConstraintInsertion { edge: usize },

    #[error("constraint for input edge {edge} was split or lost during CDT")]
    ConstraintSplitOrLost { edge: usize },

    #[error("requested component {index} does not exist ({count} components)")]
    ComponentOutOfRange { index: usize, count: usize },

    #[error("failed to classify planar regions")]
    RegionClassification,
}

/// Failures during kinetic propagation or exact/fast numeric evaluation.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PropagationError {
    #[error("fast kernel predicate uncertain at time {time}")]
    UncertainPredicate { time: f64 },

    #[error("inconsistent event for triangle {triangle:?} at time {time}")]
    InconsistentEvent { triangle: u64, time: f64 },

    #[error("non-advancing event loop detected (component {component}, time {time})")]
    NonAdvancingLoop { component: usize, time: f64 },

    #[error("arithmetic overflow or non-finite conversion at time {time}")]
    Arithmetic { time: f64 },

    #[error("topology invariant violated: {detail}")]
    TopologyInvariant { detail: String },

    #[error("exact algebraic evaluation failed: {detail}")]
    ExactEvaluation { detail: String },
}

/// Failures while building or querying the skeleton DCEL / offsets.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum SkeletonError {
    #[error(transparent)]
    Input(#[from] InputError),

    #[error(transparent)]
    Triangulation(#[from] TriangulationError),

    #[error(transparent)]
    Propagation(#[from] PropagationError),

    #[error("non-manifold topology while building DCEL: {detail}")]
    NonManifoldTopology { detail: String },

    #[error("offset construction failed at time {time}: {detail}")]
    Offset { time: f64, detail: String },

    #[error("skeleton is not finalized")]
    NotFinalized,
}
