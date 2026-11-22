//! Face visibility is independent of masks and polygroup membership.
use super::Mesh;
impl Mesh {
    /// Classify polygon centers, keeping both triangles of a quad together.
    /// Shared by visibility previews and document commits.
    pub(crate) fn face_region_selection(
        &self,
        contains: impl Fn(super::Vec3) -> bool,
    ) -> Vec<bool> {
        let mut selected: Vec<_> = self
            .triangles
            .iter()
            .map(|t| {
                contains(
                    (self.vertices[t[0]] + self.vertices[t[1]] + self.vertices[t[2]]) * (1. / 3.),
                )
            })
            .collect();
        let key = |mut t: [usize; 3]| {
            t.sort_unstable();
            t
        };
        let faces: std::collections::HashMap<_, _> = self
            .triangles
            .iter()
            .enumerate()
            .map(|(i, &t)| (key(t), i))
            .collect();
        for &[a, b, c, d] in &self.quads {
            let inside = contains(
                (self.vertices[a] + self.vertices[b] + self.vertices[c] + self.vertices[d]) * 0.25,
            );
            for t in [[a, b, c], [a, c, d], [a, b, d], [b, c, d]] {
                if let Some(&i) = faces.get(&key(t)) {
                    selected[i] = inside;
                }
            }
        }
        selected
    }

    pub fn has_hidden_faces(&self) -> bool {
        self.hidden_faces.iter().any(|&hidden| hidden)
            || (self.vertex_groups.iter().any(|g| !g.visible)
                && self
                    .polygroups
                    .iter()
                    .any(|&id| id != 0 && !self.vertex_groups[id - 1].visible))
    }
    pub fn hidden_face_count(&self) -> usize {
        if !self.has_hidden_faces() {
            return 0;
        }
        (0..self.triangles.len())
            .filter(|&i| !self.face_visible(i))
            .count()
    }
    pub fn show_all_faces(&mut self) {
        self.hidden_faces.clear();
        for group in &mut self.vertex_groups {
            group.visible = true;
        }
    }
    /// Resolve group and rectangle hiding to one effective face set.
    pub fn set_face_visibility(&mut self, visible: Vec<bool>) {
        assert_eq!(visible.len(), self.triangles.len());
        self.show_all_faces();
        if visible.iter().any(|&v| !v) {
            self.hidden_faces = visible.into_iter().map(|v| !v).collect();
        }
    }
    pub fn invert_visibility(&mut self) {
        self.set_face_visibility(
            (0..self.triangles.len())
                .map(|i| !self.face_visible(i))
                .collect(),
        );
    }
    /// Extract a face set without welding or capping its boundary. Both halves
    /// retain masks, weights, categories, and only complete surviving quads.
    pub fn visibility_part(&self, visible: bool) -> Result<Self, String> {
        let mut result = self.face_subset(|i| self.face_visible(i) == visible)?;
        result.show_all_faces();
        Ok(result)
    }
    /// Compact a face subset while preserving its attributes and visibility.
    /// Shared by visibility edits and mask splitting; cut boundaries stay open.
    pub(crate) fn face_subset(&self, include: impl Fn(usize) -> bool) -> Result<Self, String> {
        self.require_surface_topology()?;
        if !self.uv.corners.is_empty() && !self.uv.current(self) {
            return Err(
                "Restore or replace the stale UV layout before splitting or deleting faces".into(),
            );
        }
        let old_faces: Vec<_> = (0..self.triangles.len()).filter(|&i| include(i)).collect();
        let mut result = Self {
            loose_edges: Vec::new(),
            loose_vertices: Vec::new(),
            uv: Default::default(),
            creases: Vec::new(),
            shading_seams: None,
            vertices: Vec::new(),
            triangles: Vec::new(),
            quads: Vec::new(),
            mask: Vec::new(),
            materials: Vec::new(),
            vertex_groups: Vec::new(),
            polygroups: Vec::new(),
            hidden_faces: Vec::new(),
        };
        let mut indices = vec![usize::MAX; self.vertices.len()];
        let mut old_vertices = Vec::new();
        for &fi in &old_faces {
            for &i in &self.triangles[fi] {
                if indices[i] == usize::MAX {
                    indices[i] = result.vertices.len();
                    old_vertices.push(i);
                    result.vertices.push(self.vertices[i]);
                    result.mask.push(self.mask[i]);
                }
            }
            result
                .triangles
                .push(self.triangles[fi].map(|i| indices[i]));
        }
        let key = |mut t: [usize; 3]| {
            t.sort_unstable();
            t
        };
        let faces: std::collections::HashSet<_> =
            old_faces.iter().map(|&i| key(self.triangles[i])).collect();
        for &[a, b, c, d] in &self.quads {
            if (faces.contains(&key([a, b, c])) && faces.contains(&key([a, c, d])))
                || (faces.contains(&key([a, b, d])) && faces.contains(&key([b, c, d])))
            {
                result.quads.push([a, b, c, d].map(|i| indices[i]));
            }
        }
        self.reindex_groups(&mut result, &old_vertices, &old_faces);
        // Corner UVs, textures, weights and creases are already reindexed above.
        // Explicit UV cut marks and Boolean shading boundaries also follow the
        // surviving edges, including seams that were marked before unwrapping.
        result.uv.textures = self.uv.textures.clone();
        if !self.uv.seams.is_empty() || !self.shading_edges().is_empty() {
            let edges: std::collections::HashSet<_> = result
                .triangles
                .iter()
                .flat_map(|&[a, b, c]| {
                    [
                        super::uv::edge(a, b),
                        super::uv::edge(b, c),
                        super::uv::edge(c, a),
                    ]
                })
                .collect();
            let remap = |source: &[[usize; 2]]| {
                let mut mapped: Vec<_> = source
                    .iter()
                    .filter_map(|&[a, b]| {
                        let (a, b) = (*indices.get(a)?, *indices.get(b)?);
                        let edge = super::uv::edge(a, b);
                        (a != usize::MAX && b != usize::MAX && edges.contains(&edge))
                            .then_some(edge)
                    })
                    .collect();
                mapped.sort_unstable();
                mapped.dedup();
                mapped
            };
            result.uv.seams = remap(&self.uv.seams);
            result.set_shading_seams(remap(self.shading_edges()));
        }
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sculpt::{Material, Primitive, PrimitiveKind};

    #[test]
    fn visibility_extract_retains_uv_and_shading_seams_and_shared_images() {
        let mut p = Primitive::for_kind(PrimitiveKind::Plane);
        p.divisions = [3, 1, 1];
        let mut mesh = p.mesh().unwrap();
        mesh.set_shading_seams(mesh.polygon_edges());
        mesh.uv.seams = mesh.polygon_edges();
        mesh.uv.corners = mesh
            .triangles
            .iter()
            .map(|_| [[0., 0.], [1., 0.], [0., 1.]])
            .collect();
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.textures.push(
            crate::sculpt::texture::Texture::new(
                crate::sculpt::texture::Channel::Color,
                16,
                &Material::default(),
            )
            .unwrap(),
        );
        mesh.set_face_visibility(mesh.face_region_selection(|p| p.0 < 0.));
        mesh.mask = mesh
            .vertices
            .iter()
            .map(|p| if p.0 > 0. { 0.8 } else { 0.2 })
            .collect();
        for visible in [false, true] {
            for result in [
                mesh.visibility_part(visible).unwrap(),
                mesh.face_subset(|f| mesh.face_mask_selected(f, 0.6) == visible)
                    .unwrap(),
            ] {
                assert!(result.uv.current(&result));
                assert_eq!(result.uv.textures, mesh.uv.textures);
                assert_eq!(result.uv.seams, result.polygon_edges());
                assert_eq!(result.shading_edges(), result.polygon_edges());
                assert_eq!(result.quads.len() * 2, result.triangles.len());
                let restored: Mesh =
                    serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(result, restored);
            }
        }
        // Marked UV cuts and assigned images also survive before unwrapping.
        mesh.uv.corners.clear();
        mesh.uv.topology.clear();
        let result = mesh.visibility_part(true).unwrap();
        assert_eq!(result.uv.seams, result.polygon_edges());
        assert_eq!(result.uv.textures, mesh.uv.textures);
    }
}
