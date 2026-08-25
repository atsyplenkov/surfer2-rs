//! Event queue with pending update/drop batching.
//!
//! Upstream: `surf/src/EventQueue.*` (rev c42f9a7).
//! Copyright 2018, 2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! `process_pending_updates`: drop all first, then update all.

use super::collapse::{CollapseSpec, CollapseType};
use super::indexed_heap::IndexedHeap;
use crate::error::PropagationError;

/// One queue item per live triangle (`tidx` dense).
#[derive(Debug, Clone)]
pub struct EventQueue {
    heap: IndexedHeap<CollapseSpec>,
    need_update: Vec<u32>,
    need_dropping: Vec<u32>,
    in_need_update: Vec<bool>,
    in_need_dropping: Vec<bool>,
}

impl EventQueue {
    pub fn with_capacity(n_triangles: usize) -> Self {
        Self {
            heap: IndexedHeap::with_payload_capacity(n_triangles),
            need_update: Vec::new(),
            need_dropping: Vec::new(),
            in_need_update: vec![false; n_triangles],
            in_need_dropping: vec![false; n_triangles],
        }
    }

    pub fn ensure_capacity(&mut self, n: usize) {
        self.heap.ensure_payload_capacity(n);
        if self.in_need_update.len() < n {
            self.in_need_update.resize(n, false);
            self.in_need_dropping.resize(n, false);
        }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Seed or replace the collapse for triangle `tidx`.
    pub fn seed(&mut self, tidx: u32, spec: CollapseSpec) {
        self.ensure_capacity(tidx as usize + 1);
        self.heap.push(spec, tidx);
    }

    pub fn peak(&self) -> Result<Option<&CollapseSpec>, PropagationError> {
        self.assert_no_pending()?;
        Ok(self.heap.peak().map(|e| &e.priority))
    }

    pub fn peak_tidx(&self) -> Result<Option<u32>, PropagationError> {
        self.assert_no_pending()?;
        Ok(self.heap.peak().map(|e| e.payload))
    }

    pub fn needs_update(&mut self, tidx: u32) -> Result<(), PropagationError> {
        self.ensure_capacity(tidx as usize + 1);
        // Drop wins: a dying triangle must not be refreshed.
        if self.in_need_dropping[tidx as usize] {
            return Ok(());
        }
        if !self.in_need_update[tidx as usize] {
            self.in_need_update[tidx as usize] = true;
            self.need_update.push(tidx);
        }
        Ok(())
    }

    pub fn needs_dropping(&mut self, tidx: u32) -> Result<(), PropagationError> {
        self.ensure_capacity(tidx as usize + 1);
        // If previously queued for update, cancel the update.
        if self.in_need_update[tidx as usize] {
            self.in_need_update[tidx as usize] = false;
            self.need_update.retain(|&x| x != tidx);
        }
        if !self.in_need_dropping[tidx as usize] {
            self.in_need_dropping[tidx as usize] = true;
            self.need_dropping.push(tidx);
        }
        Ok(())
    }

    /// Drop pending first, then recompute priorities via `recompute(tidx) -> CollapseSpec`.
    pub fn process_pending_updates<F>(&mut self, mut recompute: F) -> Result<(), PropagationError>
    where
        F: FnMut(u32) -> Result<CollapseSpec, PropagationError>,
    {
        // Drop phase
        let dropping = std::mem::take(&mut self.need_dropping);
        for tidx in dropping {
            self.in_need_dropping[tidx as usize] = false;
            self.heap.remove_payload(tidx);
        }

        // Update phase
        let updating = std::mem::take(&mut self.need_update);
        for tidx in updating {
            self.in_need_update[tidx as usize] = false;
            let spec = recompute(tidx)?;
            if spec.type_ == CollapseType::Never && !self.heap.contains_payload(tidx) {
                // Keep NEVER entries so peak can detect termination consistently.
                self.heap.push(spec, tidx);
            } else {
                self.heap.push(spec, tidx);
            }
        }
        Ok(())
    }

    pub fn assert_no_pending(&self) -> Result<(), PropagationError> {
        if !self.need_update.is_empty() || !self.need_dropping.is_empty() {
            return Err(PropagationError::TopologyInvariant {
                detail: "event queue has pending updates".into(),
            });
        }
        Ok(())
    }

    /// Drop all NEVER entries from the top until a real event or empty.
    pub fn peak_is_never(&self) -> Result<bool, PropagationError> {
        Ok(self.peak()?.is_some_and(|s| s.type_ == CollapseType::Never))
    }
}
