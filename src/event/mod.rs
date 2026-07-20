//! Event queue, collapse specifications, and indexed heap.
//!
//! Upstream: `CollapseSpec.*`, `EventQueue.*`, `Heap.h`.

mod collapse;
mod indexed_heap;
mod queue;

pub use collapse::{CollapseSpec, CollapseType, EdgeCollapseType};
pub use indexed_heap::{HeapEntry, IndexedHeap};
pub use queue::EventQueue;
