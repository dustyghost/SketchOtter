//! Edges without faces are persistent editable topology, not render triangles.
use super::Mesh;
use std::collections::BTreeSet;

impl Mesh {
    pub(crate) fn require_surface_topology(&self) -> Result<(), String> {
        if self.loose_edges.is_empty() && self.loose_vertices.is_empty() {
            Ok(())
        } else {
            Err("This surface operation cannot preserve loose edges or isolated vertices. Delete their vertices first, or keep this mesh for component modeling.".into())
        }
    }
    pub fn model_edges(&self) -> Vec<[usize; 2]> {
        self.polygon_edges()
            .into_iter()
            .chain(self.loose_edges.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(crate) fn visible_loose_edges(&self) -> Vec<[usize; 2]> {
        if self.loose_edges.is_empty() {
            return Vec::new();
        }
        let hidden: BTreeSet<_> = self
            .triangles
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.face_visible(*i))
            .flat_map(|(_, t)| t.iter().copied())
            .collect();
        self.loose_edges
            .iter()
            .copied()
            .filter(|e| e.iter().all(|v| !hidden.contains(v)))
            .collect()
    }

    pub(crate) fn retain_loose_vertices(&mut self) {
        if self.loose_vertices.is_empty() {
            return;
        }
        let used: BTreeSet<_> = self
            .triangles
            .iter()
            .flatten()
            .chain(self.loose_edges.iter().flatten())
            .copied()
            .collect();
        self.loose_vertices.retain(|v| !used.contains(v));
        self.loose_vertices.sort_unstable();
        self.loose_vertices.dedup();
    }
    pub(super) fn validate_loose_edges(&self) -> Result<(), String> {
        if !self.loose_vertices.is_empty() {
            let used: BTreeSet<_> = self
                .triangles
                .iter()
                .flatten()
                .chain(self.loose_edges.iter().flatten())
                .copied()
                .collect();
            if self.loose_vertices.windows(2).any(|w| w[0] >= w[1])
                || self
                    .loose_vertices
                    .iter()
                    .any(|v| *v >= self.vertices.len() || used.contains(v))
            {
                return Err("Invalid isolated modeling vertices".into());
            }
        }
        if self.loose_edges.len() > super::max_vertices()
            || self.loose_edges.windows(2).any(|w| w[0] >= w[1])
            || self
                .loose_edges
                .iter()
                .any(|&[a, b]| a >= b || b >= self.vertices.len())
        {
            return Err("Invalid loose edges".into());
        }
        if !self.loose_edges.is_empty() {
            let surface: BTreeSet<_> = self.polygon_edges().into_iter().collect();
            if self.loose_edges.iter().any(|e| surface.contains(e)) {
                return Err("A loose edge duplicates a surface edge".into());
            }
        }
        Ok(())
    }
}
