//! Create a triangle or quad from a small vertex selection, including wire outlines.
use super::manual::triangulate_boundary;
use super::*;

impl Mesh {
    pub(super) fn create_selected_face(
        &self,
        selected: &[usize],
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        let wanted: BTreeSet<_> = selected.iter().copied().collect();
        if !(3..=4).contains(&wanted.len()) {
            return Err("Select three or four vertices, or the edges between them".into());
        }
        let touched: Vec<_> = wanted
            .iter()
            .flat_map(|&v| symmetry.vertex_copies(v))
            .collect();
        self.protect_model_vertices(&touched, false)?;
        let required: BTreeSet<_> = self
            .model_edges()
            .into_iter()
            .filter(|e| e.iter().all(|v| wanted.contains(v)))
            .collect();
        let mut incidence = BTreeMap::<_, Vec<_>>::new();
        for p in self.polygons() {
            for (a, b) in p.edges() {
                incidence.entry(edge(a, b)).or_default().push([a, b]);
            }
        }
        let first = *wanted.first().unwrap();
        let mut orders = Vec::new();
        fn order(prefix: Vec<usize>, rest: Vec<usize>, out: &mut Vec<Vec<usize>>) {
            if rest.is_empty() {
                out.push(prefix);
                return;
            }
            for i in 0..rest.len() {
                let mut next = prefix.clone();
                next.push(rest[i]);
                let mut remaining = rest.clone();
                remaining.remove(i);
                order(next, remaining, out);
            }
        }
        order(
            vec![first],
            wanted.iter().copied().filter(|v| *v != first).collect(),
            &mut orders,
        );
        let mut choices = Vec::new();
        for cycle in orders {
            let edges: Vec<_> = cycle
                .iter()
                .copied()
                .zip(cycle.iter().copied().cycle().skip(1))
                .take(cycle.len())
                .collect();
            let keys: BTreeSet<_> = edges.iter().map(|&(a, b)| edge(a, b)).collect();
            if !required.is_subset(&keys) {
                continue;
            }
            if edges.iter().any(|&(a, b)| {
                incidence
                    .get(&edge(a, b))
                    .is_some_and(|faces| faces.len() != 1 || faces[0] != [b, a])
            }) {
                continue;
            }
            if triangulate_boundary(self, &cycle).is_err() {
                continue;
            }
            if !keys.iter().any(|e| incidence.contains_key(e)) {
                let n = normal(
                    self,
                    &Polygon {
                        vertices: cycle.clone(),
                        group: 0,
                        hidden: false,
                        visible: true,
                    },
                );
                let dominant = [n.0, n.1, n.2]
                    .into_iter()
                    .max_by(|a, b| a.abs().total_cmp(&b.abs()))
                    .unwrap();
                if dominant < 0. {
                    continue;
                }
            }
            let length = edges
                .iter()
                .map(|&(a, b)| (self.vertices[a] - self.vertices[b]).len())
                .sum::<f32>();
            choices.push((length, cycle));
        }
        let cycle = choices.into_iter().min_by(|a,b| a.0.total_cmp(&b.0)).ok_or("These vertices cannot form a simple face that matches the existing edges. Check the selection and alignment")?.1;
        let mut out = self.clone();
        out.polygroups.resize(self.triangles.len(), 0);
        out.hidden_faces.resize(self.triangles.len(), false);
        let mut added = BTreeSet::new();
        for (map, basis) in symmetry.vertices.iter().zip(&symmetry.bases) {
            let mut corners: Vec<_> = cycle.iter().map(|&v| map[v]).collect();
            let mut key = corners.clone();
            key.sort_unstable();
            if !added.insert(key) {
                continue;
            }
            if basis[0].dot(basis[1].cross(basis[2])) < 0. {
                corners.reverse();
            }
            let (triangles, quad) = triangulate_boundary(self, &corners)?;
            if quad {
                out.quads
                    .push([corners[0], corners[1], corners[2], corners[3]]);
            }
            for triangle in triangles {
                out.push_polygon(&triangle, 0, false);
            }
        }
        let surface: BTreeSet<_> = out.polygon_edges().into_iter().collect();
        out.loose_edges.retain(|e| !surface.contains(e));
        out.retain_loose_vertices();
        if self.uv.current(self) {
            out.uv.removed.resize(self.triangles.len(), false);
            out.uv.removed.resize(out.triangles.len(), true);
            out.uv.corners.resize(out.triangles.len(), [[0.; 2]; 3]);
            if !self.uv.masks.is_empty() {
                out.uv.masks.resize(out.triangles.len(), [0.; 3]);
            }
            out.uv.topology = out.triangles.clone();
        }
        crate::sculpt::fusion::validate_added_faces(&out, self.triangles.len(), &|| false)
            .map_err(|_| "The new face intersects an existing surface".to_string())?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire() -> Mesh {
        Mesh {
            vertices: vec![
                Vec3(-0.5, -0.5, 0.),
                Vec3(0.5, -0.5, 0.),
                Vec3(0.5, 0.5, 0.),
                Vec3(-0.5, 0.5, 0.),
            ],
            mask: vec![0.; 4],
            loose_edges: vec![[0, 1], [1, 2], [2, 3]],
            ..Default::default()
        }
    }
    #[test]
    fn create_selected_face_fills_three_or_four_vertices_and_edge_endpoints() {
        let mesh = wire();
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        for vertices in [&[2, 0, 1][..], &[3, 1, 0, 2][..]] {
            let out = mesh
                .edit_model_topology(TopologyEdit::CreateFace, &[], &[], vertices, &symmetry)
                .unwrap();
            assert_eq!(out.vertices, mesh.vertices);
            assert_eq!(out.triangles.len(), vertices.len() - 2);
            assert_eq!(out.quads.len(), usize::from(vertices.len() == 4));
            assert_eq!(out.loose_edges.len(), usize::from(vertices.len() == 3));
            manifold(&out).unwrap();
        }
        let out = mesh
            .edit_model_topology(
                TopologyEdit::CreateFace,
                &[],
                &[[0, 1], [2, 3]],
                &[],
                &symmetry,
            )
            .unwrap();
        assert_eq!(out.quads.len(), 1);
        assert!(out.loose_edges.is_empty());
        assert!(
            out.edit_model_topology(TopologyEdit::CreateFace, &[], &[], &[0, 1, 2, 3], &symmetry)
                .is_err()
        );
    }
    #[test]
    fn create_selected_face_rejects_collapsed_stale_and_obstructed_points() {
        let mut mesh = wire();
        mesh.vertices[2] = Vec3(2., -0.5, 0.);
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        assert!(
            mesh.edit_model_topology(TopologyEdit::CreateFace, &[], &[], &[0, 1, 2], &symmetry)
                .is_err()
        );
        assert!(
            mesh.edit_model_topology(TopologyEdit::CreateFace, &[], &[], &[0, 1, 99], &symmetry)
                .is_err()
        );
        let mut mesh = wire();
        mesh.vertices
            .extend([Vec3(0., -0.2, -0.5), Vec3(0., 0.2, -0.5), Vec3(0., 0., 0.5)]);
        mesh.mask.resize(7, 0.);
        mesh.push_polygon(&[4, 5, 6], 0, false);
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        assert!(
            mesh.edit_model_topology(TopologyEdit::CreateFace, &[], &[], &[0, 1, 2, 3], &symmetry)
                .is_err()
        );
    }
    #[test]
    fn create_selected_face_preserves_mirrored_wire_orientation() {
        let mut mesh = wire();
        for p in &mut mesh.vertices {
            p.0 -= 1.;
        }
        let mirrored: Vec<_> = mesh.vertices.iter().map(|p| Vec3(-p.0, p.1, p.2)).collect();
        mesh.vertices.extend(mirrored);
        mesh.mask.resize(8, 0.);
        mesh.loose_edges.extend([[4, 5], [5, 6], [6, 7]]);
        let settings = crate::sculpt::Symmetry {
            enabled: true,
            mirror: [true, false, false],
            ..Default::default()
        };
        let symmetry = ModelingSymmetry::new(&mesh, &settings).unwrap();
        let out = mesh
            .edit_model_topology(TopologyEdit::CreateFace, &[], &[], &[0, 1, 2, 3], &symmetry)
            .unwrap();
        assert_eq!(out.quads.len(), 2);
        assert!(out.loose_edges.is_empty());
        symmetry.validate_result(&out).unwrap();
    }
}
