//! Research-only first-write history for fixed-topology Standard strokes.
use super::Document;
use crate::diagnostics::EditSettings;
use crate::sculpt::{Brush, Dab, Kind, Vec3};

fn bits(p: Vec3) -> [u32; 3] {
    [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()]
}

pub(crate) struct Capture {
    dirty_before: bool,
    seen: Vec<u64>,
    entry: Entry,
}

pub(crate) struct Entry {
    previous: Vec<(u32, Vec3)>,
    settings: Option<EditSettings>,
}

impl Capture {
    pub fn begin(doc: &mut Document) -> Self {
        assert!(
            !doc.stroke_active
                && !doc.mesh_settings().adaptive.enabled
                && !doc.mesh_settings().dynamic
        );
        doc.begin_stroke_with_snapshot(false);
        Self {
            dirty_before: doc.is_dirty(),
            seen: vec![0; doc.mesh().vertices.len().div_ceil(64)],
            entry: Entry {
                previous: Vec::new(),
                settings: None,
            },
        }
    }

    fn observe(&mut self, vertex: usize, old: Vec3, proposed: Vec3) {
        if bits(old) == bits(proposed) {
            return;
        }
        let word = &mut self.seen[vertex / 64];
        let mask = 1_u64 << (vertex % 64);
        if *word & mask == 0 {
            *word |= mask;
            self.entry
                .previous
                .push((u32::try_from(vertex).unwrap(), old));
        }
    }

    pub fn dab(&mut self, doc: &mut Document, brush: &Brush, dab: Dab) -> bool {
        assert_eq!(brush.kind, Kind::Standard);
        let changed = doc.apply_dab_observing(brush, dab, None, &mut |i, old, new| {
            self.observe(i, old, new)
        });
        if changed && self.entry.settings.is_none() {
            self.entry.settings = Some(doc.stroke_edit_settings(brush, dab));
        }
        changed
    }

    pub fn cancel(mut self, doc: &mut Document) {
        doc.end_stroke();
        self.entry.swap(doc);
        doc.dirty = self.dirty_before;
    }

    pub fn finish(mut self, doc: &mut Document) -> Entry {
        doc.end_stroke();
        // A guard can reject a move, or subsequent dabs can reverse it.
        self.entry
            .previous
            .retain(|&(i, old)| bits(old) != bits(doc.mesh().vertices[i as usize]));
        self.entry
    }
}

impl Entry {
    pub fn len(&self) -> usize {
        self.previous.len()
    }
    pub fn position_bytes(&self) -> usize {
        self.previous.capacity() * size_of::<(u32, Vec3)>()
    }
    pub fn swap(&mut self, doc: &mut Document) {
        for (index, old) in &mut self.previous {
            std::mem::swap(&mut doc.geometry.mesh.vertices[*index as usize], old);
        }
        doc.display_changed();
    }
}
