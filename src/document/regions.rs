//! Short-lived dirty history for dense, fixed-topology viewport edits.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
const LIMIT: usize = 16_384;
const FRAMES: usize = 8;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp(u64, u64);

#[derive(Default)]
pub(crate) struct Edits {
    pub(crate) positions: Vec<usize>,
    pub(crate) mask: Vec<usize>,
    pub(crate) materials: Vec<usize>,
}
impl Edits {
    pub(super) fn push(list: &mut Vec<usize>, index: usize) {
        if list.len() <= LIMIT {
            list.push(index)
        }
    }
    fn len(&self) -> usize {
        self.positions.len() + self.mask.len() + self.materials.len()
    }
    fn canonicalise(&mut self) {
        for list in [&mut self.positions, &mut self.mask, &mut self.materials] {
            list.sort_unstable();
            list.dedup();
        }
    }
}
pub(super) struct Journal {
    identity: u64,
    changes: VecDeque<(u64, u64, Edits)>,
}
impl Default for Journal {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            changes: VecDeque::new(),
        }
    }
}
impl Journal {
    pub(super) fn add(&mut self, before: u64, after: u64, mut edits: Edits) {
        if edits.len() > LIMIT {
            self.changes.clear();
            return;
        }
        edits.canonicalise();
        if self.changes.back().is_some_and(|last| last.1 != before) {
            self.changes.clear();
        }
        if self.changes.len() == FRAMES {
            self.changes.pop_front();
        }
        self.changes.push_back((before, after, edits));
    }
    fn since(&self, stamp: Stamp, after: u64) -> Option<Edits> {
        if stamp.0 != self.identity || stamp.1 == after {
            return None;
        }
        let start = self.changes.iter().position(|e| e.0 == stamp.1)?;
        let mut out = Edits::default();
        let mut cursor = stamp.1;
        for (from, to, changes) in self.changes.iter().skip(start) {
            if *from != cursor || out.len() + changes.len() > LIMIT {
                return None;
            }
            out.positions.extend_from_slice(&changes.positions);
            out.mask.extend_from_slice(&changes.mask);
            out.materials.extend_from_slice(&changes.materials);
            cursor = *to;
            if cursor == after {
                out.canonicalise();
                return Some(out);
            }
        }
        None
    }
}
impl super::Document {
    pub(crate) fn region_stamp(&self) -> Stamp {
        Stamp(self.regions.identity, self.display_revision)
    }
    pub(crate) fn display_edits(&self, since: Stamp) -> Option<Edits> {
        self.regions.since(since, self.display_revision)
    }
    pub(super) fn display_changed_in(&mut self, edits: Edits) {
        let before = self.display_revision;
        self.display_changed();
        self.regions.add(before, self.display_revision, edits);
    }
}
