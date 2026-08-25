//! Indexed mutable binary min-heap.
//!
//! Upstream: `surf/src/Heap.h` (rev c42f9a7).
//! Copyright 2011–2019 Peter Palfrader — GPL-3.0-or-later.
//!
//! Equal priorities do not reorder parent↔child. No arbitrary ID tiebreak.

use std::cmp::Ordering;

/// Item stored in the heap; `idx` is maintained by the heap (-1 = absent).
#[derive(Debug, Clone)]
pub struct HeapEntry<P> {
    pub priority: P,
    pub idx_in_heap: i32,
    /// Payload index (e.g. triangle generation key as u64 bits, or dense tidx).
    pub payload: u32,
}

impl<P> HeapEntry<P> {
    pub fn new(priority: P, payload: u32) -> Self {
        Self {
            priority,
            idx_in_heap: -1,
            payload,
        }
    }
}

/// Min-heap over entries addressed by dense payload indices via side map.
#[derive(Debug, Clone)]
pub struct IndexedHeap<P> {
    /// Heap array of entry indices into `entries`.
    heap: Vec<usize>,
    entries: Vec<HeapEntry<P>>,
    /// payload → entry index (usize::MAX = none).
    by_payload: Vec<usize>,
}

impl<P: Ord + Clone> IndexedHeap<P> {
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            entries: Vec::new(),
            by_payload: Vec::new(),
        }
    }

    pub fn with_payload_capacity(n: usize) -> Self {
        Self {
            heap: Vec::with_capacity(n),
            entries: Vec::with_capacity(n),
            by_payload: vec![usize::MAX; n],
        }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn clear(&mut self) {
        self.heap.clear();
        self.entries.clear();
        self.by_payload.fill(usize::MAX);
    }

    pub fn ensure_payload_capacity(&mut self, n: usize) {
        if self.by_payload.len() < n {
            self.by_payload.resize(n, usize::MAX);
        }
    }

    pub fn peak(&self) -> Option<&HeapEntry<P>> {
        self.heap.first().map(|&i| &self.entries[i])
    }

    pub fn get_by_payload(&self, payload: u32) -> Option<&HeapEntry<P>> {
        let ei = *self.by_payload.get(payload as usize)?;
        if ei == usize::MAX {
            None
        } else {
            Some(&self.entries[ei])
        }
    }

    pub fn contains_payload(&self, payload: u32) -> bool {
        matches!(self.by_payload.get(payload as usize), Some(&ei) if ei != usize::MAX)
            && self
                .get_by_payload(payload)
                .is_some_and(|e| e.idx_in_heap >= 0)
    }

    pub fn push(&mut self, priority: P, payload: u32) {
        self.ensure_payload_capacity(payload as usize + 1);
        if self.by_payload[payload as usize] != usize::MAX {
            // Update existing
            let ei = self.by_payload[payload as usize];
            self.entries[ei].priority = priority;
            let idx = self.entries[ei].idx_in_heap;
            if idx >= 0 {
                self.fix_idx(idx as usize);
            } else {
                self.entries[ei].idx_in_heap = self.heap.len() as i32;
                self.heap.push(ei);
                self.sift_up(self.heap.len() - 1);
            }
            return;
        }
        let ei = self.entries.len();
        let mut entry = HeapEntry::new(priority, payload);
        entry.idx_in_heap = self.heap.len() as i32;
        self.entries.push(entry);
        self.by_payload[payload as usize] = ei;
        self.heap.push(ei);
        self.sift_up(self.heap.len() - 1);
    }

    pub fn set_priority_payload(&mut self, payload: u32, priority: P) -> bool {
        let Some(&ei) = self.by_payload.get(payload as usize) else {
            return false;
        };
        if ei == usize::MAX {
            return false;
        }
        let idx = self.entries[ei].idx_in_heap;
        if idx < 0 {
            return false;
        }
        self.entries[ei].priority = priority;
        self.fix_idx(idx as usize);
        true
    }

    pub fn remove_payload(&mut self, payload: u32) -> bool {
        let Some(&ei) = self.by_payload.get(payload as usize) else {
            return false;
        };
        if ei == usize::MAX {
            return false;
        }
        let idx = self.entries[ei].idx_in_heap;
        if idx < 0 {
            self.by_payload[payload as usize] = usize::MAX;
            return false;
        }
        self.remove_at(idx as usize);
        self.by_payload[payload as usize] = usize::MAX;
        true
    }

    pub fn pop(&mut self) -> Option<HeapEntry<P>> {
        if self.heap.is_empty() {
            return None;
        }
        let ei = self.heap[0];
        let payload = self.entries[ei].payload;
        self.remove_at(0);
        self.by_payload[payload as usize] = usize::MAX;
        // Move entry out by cloning priority/payload; idx cleared
        let mut e = self.entries[ei].clone();
        e.idx_in_heap = -1;
        Some(e)
    }

    fn remove_at(&mut self, idx: usize) {
        let last = self.heap.len() - 1;
        if idx != last {
            self.swap_idx(idx, last);
        }
        let ei = self.heap.pop().unwrap();
        self.entries[ei].idx_in_heap = -1;
        if idx < self.heap.len() {
            self.fix_idx(idx);
        }
    }

    fn fix_idx(&mut self, idx: usize) {
        if idx > 0 && self.less_or_eq(idx, Self::parent(idx)) {
            // May need sift up if smaller than parent (use < for strict)
            if self.less(idx, Self::parent(idx)) {
                self.sift_up(idx);
                return;
            }
        }
        self.sift_down(idx);
    }

    fn sift_up(&mut self, mut child: usize) {
        while child > 0 {
            let parent = Self::parent(child);
            if !self.less(child, parent) {
                break;
            }
            self.swap_idx(child, parent);
            child = parent;
        }
    }

    fn sift_down(&mut self, mut root: usize) {
        let len = self.heap.len();
        loop {
            let left = Self::left(root);
            if left >= len {
                break;
            }
            let right = Self::right(root);
            // No right child: left wins without comparison.
            let smallest = if right >= len || self.less_or_eq(left, right) {
                left
            } else {
                right
            };
            if !self.less(smallest, root) {
                break;
            }
            self.swap_idx(root, smallest);
            root = smallest;
        }
    }

    fn less(&self, i: usize, j: usize) -> bool {
        self.prio(i).cmp(self.prio(j)) == Ordering::Less
    }

    fn less_or_eq(&self, i: usize, j: usize) -> bool {
        matches!(
            self.prio(i).cmp(self.prio(j)),
            Ordering::Less | Ordering::Equal
        )
    }

    fn prio(&self, heap_idx: usize) -> &P {
        &self.entries[self.heap[heap_idx]].priority
    }

    fn swap_idx(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        let ea = self.heap[a];
        let eb = self.heap[b];
        self.entries[ea].idx_in_heap = a as i32;
        self.entries[eb].idx_in_heap = b as i32;
    }

    #[inline]
    fn parent(i: usize) -> usize {
        (i - 1) / 2
    }
    #[inline]
    fn left(i: usize) -> usize {
        i * 2 + 1
    }
    #[inline]
    fn right(i: usize) -> usize {
        i * 2 + 2
    }

    /// Linear-time heap property check (tests / debug).
    pub fn is_heap(&self) -> bool {
        for i in 1..self.heap.len() {
            let p = Self::parent(i);
            if self.less(i, p) {
                return false;
            }
        }
        for (hi, &ei) in self.heap.iter().enumerate() {
            if self.entries[ei].idx_in_heap != hi as i32 {
                return false;
            }
        }
        true
    }
}

impl<P: Ord + Clone> Default for IndexedHeap<P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CollapseSpec, CollapseType};

    #[test]
    fn push_pop_sorted() {
        let mut h = IndexedHeap::new();
        for (i, t) in [3.0, 1.0, 2.0, 0.5].iter().enumerate() {
            h.push(
                CollapseSpec::new(0, CollapseType::TriangleCollapse, *t),
                i as u32,
            );
        }
        assert!(h.is_heap());
        let mut times = Vec::new();
        while let Some(e) = h.pop() {
            times.push(e.priority.time);
        }
        assert_eq!(times, vec![0.5, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn update_priority() {
        let mut h = IndexedHeap::new();
        h.push(CollapseSpec::new(0, CollapseType::TriangleCollapse, 5.0), 0);
        h.push(CollapseSpec::new(0, CollapseType::TriangleCollapse, 3.0), 1);
        h.set_priority_payload(0, CollapseSpec::new(0, CollapseType::TriangleCollapse, 0.1));
        assert!(h.is_heap());
        assert_eq!(h.peak().unwrap().payload, 0);
    }

    #[test]
    fn equal_keys_stable_heap_property() {
        let mut h = IndexedHeap::new();
        let p = CollapseSpec::new(0, CollapseType::TriangleCollapse, 1.0);
        h.push(p, 0);
        h.push(p, 1);
        h.push(p, 2);
        assert!(h.is_heap());
        assert_eq!(h.len(), 3);
    }
}
