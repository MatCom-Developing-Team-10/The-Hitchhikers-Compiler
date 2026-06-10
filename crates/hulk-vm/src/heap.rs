//! GC-managed object heap for the HULK VM.
//!
//! Implements a classic **mark-and-sweep** garbage collector:
//!
//! - The heap is a `Vec<Slot>`; each [`ObjectId`] is an index into it.
//! - [`Heap::alloc`] reuses freed slots from a free-list before growing.
//! - [`Heap::collect`] traces objects reachable from a set of root ids,
//!   marking them; unmarked live slots are released back into the free-list.
//! - GC is triggered after [`Heap::gc_threshold`] new allocations
//!   accumulate since the last collection. The threshold defaults to 1024
//!   but can be overridden via the `HULK_GC_THRESHOLD` environment variable
//!   (parsed lazily in [`Heap::new`]).
//!
//! Cycles between objects are correctly reclaimed because tracing starts
//! from live roots — an unreachable cycle has no path from any root and
//! thus is not marked.
//!
//! Strings are owned `String`s embedded in `Value::Str` and are not managed
//! by this heap (they follow Rust's normal ownership rules and are dropped
//! when their last owner goes away).

use std::collections::HashMap;

use hulk_ir::{Object, ObjectId, Value};

/// A heap slot. `Live(Object)` holds a real object; `Free` is a recyclable
/// hole referenced by the free-list.
#[derive(Debug)]
enum Slot {
    Live(Object),
    Free,
}

/// GC-managed heap of [`Object`]s.
#[derive(Debug)]
pub struct Heap {
    slots: Vec<Slot>,
    free_list: Vec<u32>,
    /// Number of `alloc` calls since the last `collect`. Used to decide when
    /// to trigger collection.
    allocations_since_gc: usize,
    /// When `allocations_since_gc` reaches this threshold, the caller is
    /// expected to invoke [`collect`](Self::collect). Defaults to 1024.
    pub gc_threshold: usize,
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap {
    /// Create an empty heap. Reads `HULK_GC_THRESHOLD` from the environment
    /// for the GC trigger.
    pub fn new() -> Self {
        let gc_threshold = std::env::var("HULK_GC_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1024);
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
            allocations_since_gc: 0,
            gc_threshold,
        }
    }

    /// Allocate a new object, returning its handle. Reuses a freed slot when
    /// available; otherwise grows the slot vector by one.
    pub fn alloc(&mut self, obj: Object) -> ObjectId {
        self.allocations_since_gc += 1;
        if let Some(id) = self.free_list.pop() {
            self.slots[id as usize] = Slot::Live(obj);
            ObjectId(id)
        } else {
            let id = u32::try_from(self.slots.len()).expect("invariant: heap size fits in u32");
            self.slots.push(Slot::Live(obj));
            ObjectId(id)
        }
    }

    /// Borrow the object referenced by `id`.
    ///
    /// # Panics
    /// Panics if the handle is invalid or refers to a freed slot — this
    /// always indicates a bug in the VM (e.g. forgetting to mark a root).
    pub fn get(&self, id: ObjectId) -> &Object {
        match self.slots.get(id.0 as usize) {
            Some(Slot::Live(o)) => o,
            _ => panic!("invariant: dangling ObjectId({})", id.0),
        }
    }

    /// Mutably borrow the object referenced by `id`.
    pub fn get_mut(&mut self, id: ObjectId) -> &mut Object {
        match self.slots.get_mut(id.0 as usize) {
            Some(Slot::Live(o)) => o,
            _ => panic!("invariant: dangling ObjectId({})", id.0),
        }
    }

    /// Total number of slots ever allocated (live + free).
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Current number of live (non-freed) objects.
    pub fn live_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, Slot::Live(_)))
            .count()
    }

    /// Number of allocations since the last GC.
    pub fn pending(&self) -> usize {
        self.allocations_since_gc
    }

    /// `true` when the caller should invoke [`collect`](Self::collect).
    pub fn should_collect(&self) -> bool {
        self.allocations_since_gc >= self.gc_threshold
    }

    /// Run mark-and-sweep starting from the given roots. Returns the number
    /// of slots freed by this collection.
    pub fn collect<I: IntoIterator<Item = ObjectId>>(&mut self, roots: I) -> usize {
        let mut marked = vec![false; self.slots.len()];

        // Mark: depth-first walk from each root.
        let mut stack: Vec<ObjectId> = roots.into_iter().collect();
        while let Some(id) = stack.pop() {
            let idx = id.0 as usize;
            if idx >= marked.len() || marked[idx] {
                continue;
            }
            // Only mark if the slot is still live.
            let Some(Slot::Live(obj)) = self.slots.get(idx) else {
                continue;
            };
            marked[idx] = true;
            for v in obj.fields.values() {
                if let Value::Object(child) = v {
                    stack.push(*child);
                }
            }
        }

        // Sweep: any live slot that wasn't marked becomes free.
        let mut freed = 0;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if matches!(slot, Slot::Live(_)) && !marked[i] {
                *slot = Slot::Free;
                self.free_list.push(i as u32);
                freed += 1;
            }
        }

        self.allocations_since_gc = 0;
        freed
    }

    /// Iterate over (id, &object) pairs for every live slot. Useful for
    /// debugging and root-set helpers; runtime code should use `get`.
    pub fn iter_live(&self) -> impl Iterator<Item = (ObjectId, &Object)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| match s {
            Slot::Live(o) => Some((ObjectId(i as u32), o)),
            Slot::Free => None,
        })
    }
}

/// Helper: collect every `ObjectId` reachable in a slice of `Value`s.
pub fn ids_in_values<'a, I: IntoIterator<Item = &'a Value>>(values: I) -> Vec<ObjectId> {
    values
        .into_iter()
        .filter_map(|v| match v {
            Value::Object(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// Helper: collect every `ObjectId` reachable in a slice of scope maps.
pub fn ids_in_scopes(scopes: &[HashMap<String, Value>]) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for scope in scopes {
        for v in scope.values() {
            if let Value::Object(id) = v {
                out.push(*id);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obj(name: &str) -> Object {
        Object {
            type_name: name.to_string(),
            fields: HashMap::new(),
        }
    }

    #[test]
    fn alloc_then_get_roundtrip() {
        let mut h = Heap::new();
        let id = h.alloc(make_obj("Dog"));
        assert_eq!(h.get(id).type_name, "Dog");
        assert_eq!(h.live_count(), 1);
    }

    #[test]
    fn collect_with_no_roots_frees_everything() {
        let mut h = Heap::new();
        h.alloc(make_obj("A"));
        h.alloc(make_obj("B"));
        h.alloc(make_obj("C"));
        let freed = h.collect(std::iter::empty());
        assert_eq!(freed, 3);
        assert_eq!(h.live_count(), 0);
        assert_eq!(h.capacity(), 3);
    }

    #[test]
    fn collect_keeps_rooted_objects() {
        let mut h = Heap::new();
        let a = h.alloc(make_obj("A"));
        h.alloc(make_obj("B"));
        let c = h.alloc(make_obj("C"));
        let freed = h.collect([a, c]);
        assert_eq!(freed, 1);
        assert_eq!(h.live_count(), 2);
    }

    #[test]
    fn collect_traces_through_fields() {
        let mut h = Heap::new();
        let child = h.alloc(make_obj("Child"));
        let mut parent = make_obj("Parent");
        parent.fields.insert("c".into(), Value::Object(child));
        let parent_id = h.alloc(parent);
        let freed = h.collect([parent_id]);
        assert_eq!(freed, 0); // child is reachable through parent
        assert_eq!(h.live_count(), 2);
    }

    #[test]
    fn collect_reclaims_cycles() {
        // a.next := b; b.next := a; roots is empty → both freed.
        let mut h = Heap::new();
        let a = h.alloc(make_obj("Node"));
        let b = h.alloc(make_obj("Node"));
        h.get_mut(a).fields.insert("next".into(), Value::Object(b));
        h.get_mut(b).fields.insert("next".into(), Value::Object(a));
        let freed = h.collect(std::iter::empty());
        assert_eq!(freed, 2);
    }

    #[test]
    fn freed_slots_are_reused() {
        let mut h = Heap::new();
        let _ = h.alloc(make_obj("A"));
        let _ = h.alloc(make_obj("B"));
        h.collect(std::iter::empty()); // frees both
        let new = h.alloc(make_obj("Replacement"));
        // The new id reused an existing slot — capacity didn't grow.
        assert!(new.0 < 2);
        assert_eq!(h.capacity(), 2);
    }
}
