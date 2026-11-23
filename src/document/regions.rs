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
    #[cfg(test)]
    disabled: bool,
    changes: VecDeque<(u64, u64, Edits)>,
}
impl Default for Journal {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT.fetch_add(1, Ordering::Relaxed),
            #[cfg(test)]
            disabled: false,
            changes: VecDeque::new(),
        }
    }
}
impl Journal {
    pub(super) fn clear(&mut self) {
        self.changes.clear();
    }
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
    pub(super) fn track_display_regions(&self) -> bool {
        #[cfg(test)]
        if self.regions.disabled {
            return false;
        }
        self.mesh().vertices.len() >= 20_000
    }
    #[cfg(test)]
    pub(crate) fn disable_display_regions(&mut self) {
        self.regions.clear();
        self.regions.disabled = true;
    }

    pub(crate) fn region_stamp(&self) -> Stamp {
        Stamp(self.regions.identity, self.display_revision)
    }
    pub(crate) fn display_edits(&self, since: Stamp) -> Option<Edits> {
        self.regions.since(since, self.display_revision)
    }
    pub(super) fn display_changed_in(&mut self, edits: Edits) {
        let before = self.display_revision;
        self.invalidate_display();
        self.regions.add(before, self.display_revision, edits);
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;

    #[test]
    fn changed_regions_expire_on_replacement_eviction_and_large_updates() {
        let mut journal = Journal::default();
        let first = Stamp(journal.identity, 0);
        let unrelated = Stamp(Journal::default().identity, 0);
        for i in 0..FRAMES {
            journal.add(
                i as u64,
                (i + 1) as u64,
                Edits {
                    positions: vec![2, i],
                    mask: vec![3],
                    materials: vec![i],
                },
            );
        }
        let values = journal.since(first, FRAMES as u64).unwrap();
        assert_eq!(values.positions, (0..FRAMES).collect::<Vec<_>>());
        assert_eq!(values.mask, [3]);
        assert_eq!(values.materials, (0..FRAMES).collect::<Vec<_>>());
        assert!(journal.since(unrelated, FRAMES as u64).is_none());
        journal.add(FRAMES as u64, FRAMES as u64 + 1, Edits::default());
        assert!(journal.since(first, FRAMES as u64 + 1).is_none());
        let recent = Stamp(journal.identity, FRAMES as u64);
        journal.add(
            100,
            101,
            Edits {
                positions: vec![7],
                ..Default::default()
            },
        );
        assert!(journal.since(recent, 101).is_none());
        let from = Stamp(journal.identity, 100);
        journal.add(
            101,
            102,
            Edits {
                materials: vec![0; LIMIT + 1],
                ..Default::default()
            },
        );
        assert!(journal.since(from, 102).is_none());
    }
}
