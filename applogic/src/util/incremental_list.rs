// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A list that records structured diffs for each mutation.
//!
//! Consumers drain the pending diffs to react to changes incrementally
//! rather than diffing the entire list on every update.

use std::{fmt, ops::Index};

/// A mutation record produced by [`IncrementalList`].
#[derive(Debug, Clone)]
pub enum IncrementalDiff<T> {
    /// Items were inserted starting at `index`.
    Insert { index: usize, items: Vec<T> },
    /// `count` items were removed starting at `index`.
    Remove { index: usize, count: usize },
    /// The item at `index` was replaced.
    Update { index: usize, item: T },
    /// The entire list was replaced. Contains the new items.
    Reload { items: Vec<T> },
}

/// A list that tracks mutations as structured diffs.
///
/// Each mutation method appends a corresponding [`IncrementalDiff`] to a
/// pending queue. Call [`drain_diffs`](IncrementalList::drain_diffs) to
/// consume them.
pub struct IncrementalList<T> {
    items: Vec<T>,
    pending_diffs: Vec<IncrementalDiff<T>>,
}

impl<T: Clone> Clone for IncrementalList<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
            pending_diffs: self.pending_diffs.clone(),
        }
    }
}

impl<T> Default for IncrementalList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for IncrementalList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IncrementalList")
            .field("items", &self.items)
            .field("pending_diffs", &self.pending_diffs.len())
            .finish()
    }
}

impl<T> IncrementalList<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            pending_diffs: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    #[cfg(test)]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    pub fn first(&self) -> Option<&T> {
        self.items.first()
    }

    pub fn last(&self) -> Option<&T> {
        self.items.last()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }
}

impl<T> Index<usize> for IncrementalList<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        &self.items[index]
    }
}

impl<T> IncrementalList<T> {
    /// Returns and clears pending diffs.
    pub fn drain_diffs(&mut self) -> Vec<IncrementalDiff<T>> {
        std::mem::take(&mut self.pending_diffs)
    }
}

impl<T: Clone> IncrementalList<T> {
    /// Replace the entire list. Clears any pending diffs.
    pub fn reload(&mut self, items: Vec<T>) {
        self.pending_diffs.clear();
        self.pending_diffs.push(IncrementalDiff::Reload {
            items: items.clone(),
        });
        self.items = items;
    }

    /// Insert items at `index`.
    pub fn insert_range(&mut self, index: usize, items: Vec<T>) {
        self.pending_diffs.push(IncrementalDiff::Insert {
            index,
            items: items.clone(),
        });
        self.items.splice(index..index, items);
    }

    /// Remove `count` items starting at `index`.
    pub fn remove_range(&mut self, index: usize, count: usize) {
        self.items.drain(index..index + count);
        self.pending_diffs
            .push(IncrementalDiff::Remove { index, count });
    }

    /// Truncate the list to `len` items, recording a remove diff if needed.
    pub fn truncate(&mut self, len: usize) {
        if len < self.items.len() {
            let count = self.items.len() - len;
            self.items.truncate(len);
            self.pending_diffs
                .push(IncrementalDiff::Remove { index: len, count });
        }
    }

    /// Replace the item at `index`.
    #[cfg(test)]
    pub fn update(&mut self, index: usize, item: T) {
        self.pending_diffs.push(IncrementalDiff::Update {
            index,
            item: item.clone(),
        });
        self.items[index] = item;
    }
}

impl<T: Clone + PartialEq> IncrementalList<T> {
    /// Mutate the list in place and emit `Update` diffs for tracked indices whose
    /// final value changed.
    ///
    /// The closure receives the full list as a mutable slice so callers can
    /// update items using neighboring context while still preserving
    /// incremental change tracking.
    pub fn mutate_and_record_updates<F, I>(&mut self, tracked_indices: I, mutate: F)
    where
        F: FnOnce(&mut [T]),
        I: IntoIterator<Item = usize>,
    {
        let tracked_items: Vec<(usize, T)> = tracked_indices
            .into_iter()
            .filter_map(|index| self.items.get(index).cloned().map(|item| (index, item)))
            .collect();

        mutate(&mut self.items);

        for (index, old_item) in tracked_items {
            let Some(new_item) = self.items.get(index) else {
                continue;
            };
            if *new_item != old_item {
                self.pending_diffs.push(IncrementalDiff::Update {
                    index,
                    item: new_item.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_clears_pending_diffs() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        list.insert_range(0, vec![0]);
        list.reload(vec![10, 20]);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(&diffs[0], IncrementalDiff::Reload { items } if items == &[10, 20]));
        assert_eq!(list.as_slice(), &[10, 20]);
    }

    #[test]
    fn insert_range_records_diff() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        let _ = list.drain_diffs();

        list.insert_range(0, vec![10, 20]);
        assert_eq!(list.as_slice(), &[10, 20, 1, 2, 3]);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(
            &diffs[0],
            IncrementalDiff::Insert { index: 0, items } if items == &[10, 20]
        ));
    }

    #[test]
    fn remove_range_records_diff() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3, 4, 5]);
        let _ = list.drain_diffs();

        list.remove_range(1, 2);
        assert_eq!(list.as_slice(), &[1, 4, 5]);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(
            diffs[0],
            IncrementalDiff::Remove { index: 1, count: 2 }
        ));
    }

    #[test]
    fn truncate_records_diff() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3, 4, 5]);
        let _ = list.drain_diffs();

        list.truncate(3);
        assert_eq!(list.as_slice(), &[1, 2, 3]);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(
            diffs[0],
            IncrementalDiff::Remove { index: 3, count: 2 }
        ));
    }

    #[test]
    fn update_records_diff() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        let _ = list.drain_diffs();

        list.update(1, 20);
        assert_eq!(list.as_slice(), &[1, 20, 3]);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 1);
        assert!(matches!(
            &diffs[0],
            IncrementalDiff::Update { index: 1, item: 20 }
        ));
    }

    #[test]
    fn multiple_mutations_accumulate() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        let _ = list.drain_diffs();

        list.insert_range(3, vec![4, 5]);
        list.update(0, 10);
        list.remove_range(1, 1);

        let diffs = list.drain_diffs();
        assert_eq!(diffs.len(), 3);
        assert_eq!(list.as_slice(), &[10, 3, 4, 5]);
    }

    #[test]
    fn clone_preserves_pending_diffs() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        list.insert_range(0, vec![0]);

        let mut cloned = list.clone();
        cloned.update(0, 99);

        // Original has 2 diffs (reload + insert)
        assert_eq!(list.drain_diffs().len(), 2);
        // Clone carried forward 2 diffs + added 1
        assert_eq!(cloned.drain_diffs().len(), 3);
    }

    #[test]
    fn mutate_and_record_updates_tracks_changed_items() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3, 4]);
        let _ = list.drain_diffs();

        list.mutate_and_record_updates(1..3, |items| {
            items[1] = 20;
            items[2] = 30;
        });

        let diffs = list.drain_diffs();
        assert_eq!(list.as_slice(), &[1, 20, 30, 4]);
        assert_eq!(diffs.len(), 2);
        assert!(matches!(
            &diffs[0],
            IncrementalDiff::Update { index: 1, item: 20 }
        ));
        assert!(matches!(
            &diffs[1],
            IncrementalDiff::Update { index: 2, item: 30 }
        ));
    }

    #[test]
    fn mutate_and_record_updates_ignores_unchanged_items() {
        let mut list = IncrementalList::new();
        list.reload(vec![1, 2, 3]);
        let _ = list.drain_diffs();

        list.mutate_and_record_updates(0..3, |items| {
            items[1] = 2;
        });

        assert!(list.drain_diffs().is_empty());
    }
}
