//! Extrusion joins reuse existing targets rather than leaving coincident endpoints.
use super::*;

impl Mesh {
    pub(crate) fn connect_extruded_point(
        &self,
        source: usize,
        target: usize,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<(Mesh, Vec<usize>), String> {
        manifold(self)?;
        self.protect_model_vertices(&[source, target], protect)?;
        let existing: BTreeSet<_> = self.model_edges().into_iter().collect();
        let mut edges = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for map in &symmetry.vertices {
            let (a, b) = (map[source], map[target]);
            self.protect_model_vertices(&[a, b], protect)?;
            if a == b || existing.contains(&edge(a, b)) {
                return Err("These points are already connected".into());
            }
            if (self.vertices[a] - self.vertices[b]).len() < 1e-7 {
                return Err("Choose a target away from the starting point".into());
            }
            edges.insert(edge(a, b));
            targets.insert(b);
        }
        let mut out = self.clone();
        out.loose_edges.extend(edges);
        out.loose_edges.sort_unstable();
        out.retain_loose_vertices();
        out.validate()?;
        symmetry.validate_result(&out)?;
        Ok((out, targets.into_iter().collect()))
    }

    /// Boundary directions follow their single incident face.
    pub(crate) fn open_model_edges(&self) -> BTreeMap<[usize; 2], [usize; 2]> {
        let mut incidence = BTreeMap::<_, Vec<_>>::new();
        for p in self.polygons() {
            for (a, b) in p.edges() {
                incidence
                    .entry(edge(a, b))
                    .or_default()
                    .push(([a, b], p.visible));
            }
        }
        incidence
            .into_iter()
            .filter_map(|(key, faces)| {
                (faces.len() == 1 && faces[0].1).then_some((key, faces[0].0))
            })
            .collect()
    }

    pub(crate) fn join_extruded_edge(
        &self,
        source: [usize; 2],
        target: [usize; 2],
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        manifold(self)?;
        self.protect_model_vertices(&[source[0], source[1], target[0], target[1]], protect)?;
        if !self.uv.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear the inactive UV map before joining edges".into());
        }
        let boundaries = self.open_model_edges();
        let mut faces = BTreeMap::new();
        for map in &symmetry.vertices {
            let source = source.map(|v| map[v]);
            let target = target.map(|v| map[v]);
            let &[a, b] = boundaries
                .get(&edge(source[0], source[1]))
                .ok_or("Start from an open edge")?;
            let &[c, d] = boundaries
                .get(&edge(target[0], target[1]))
                .ok_or("Snap to another open edge")?;
            let corners = [b, a, d, c];
            let mut key = corners;
            key.sort_unstable();
            if key.windows(2).any(|w| w[0] == w[1]) {
                return Err("Choose an edge with separate endpoints".into());
            }
            self.protect_model_vertices(&corners, protect)?;
            let [p, q, r, s] = corners.map(|v| self.vertices[v]);
            if (q - p).cross(r - p).dot((r - p).cross(s - p)) <= 0. {
                return Err("These edges would form a folded or collapsed face".into());
            }
            faces.entry(key).or_insert(corners);
        }
        let mut out = self.clone();
        out.polygroups.resize(self.triangles.len(), 0);
        out.hidden_faces.resize(self.triangles.len(), false);
        for corners in faces.values() {
            out.push_polygon(corners, 0, false);
        }
        let surface: BTreeSet<_> = out.polygon_edges().into_iter().collect();
        out.loose_edges.retain(|e| !surface.contains(e));
        out.retain_loose_vertices();
        if self.uv.current(self) {
            out.uv.removed.resize(self.triangles.len(), false);
            out.uv.removed.resize(out.triangles.len(), true);
            out.uv.corners.resize(out.triangles.len(), [[0.; 2]; 3]);
            if !out.uv.masks.is_empty() {
                out.uv.masks.resize(out.triangles.len(), [0.; 3]);
            }
            out.uv.topology = out.triangles.clone();
        }
        if !self.shading_edges().is_empty() {
            out.set_shading_seams(self.shading_edges().to_vec());
        }
        manifold(&out)?;
        crate::sculpt::fusion::validate_added_faces(&out, self.triangles.len(), &|| false)
            .map_err(|_| "The join would intersect an existing surface".to_string())?;
        crate::sculpt::fusion::validate_surface(&out)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    use crate::sculpt::Symmetry;

    fn panels() -> Mesh {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3(-1., -0.5, 0.),
                Vec3(-0.3, -0.5, 0.),
                Vec3(-0.3, 0.5, 0.),
                Vec3(-1., 0.5, 0.),
                Vec3(0.3, -0.5, 0.),
                Vec3(1., -0.5, 0.),
                Vec3(1., 0.5, 0.),
                Vec3(0.3, 0.5, 0.),
            ],
            mask: vec![0.; 8],
            ..Default::default()
        };
        mesh.push_polygon(&[0, 1, 2, 3], 0, false);
        mesh.push_polygon(&[4, 5, 6, 7], 0, false);
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = vec![[[0., 0.], [1., 0.], [1., 1.]]; 4];
        mesh
    }

    #[test]
    fn extrusion_snap_join_reuses_vertices_and_preserves_faces_and_uvs() {
        let mesh = panels();
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let (point, tips) = mesh.connect_extruded_point(1, 4, true, &symmetry).unwrap();
        assert_eq!(tips, vec![4]);
        assert_eq!(point.loose_edges, vec![[1, 4]]);
        assert_eq!(point.vertices, mesh.vertices);
        assert_eq!(point.triangles, mesh.triangles);
        assert_eq!(point.uv, mesh.uv);
        let connected = point.connect_extruded_point(1, 4, true, &symmetry);
        assert!(connected.is_err());
        let joined = point
            .join_extruded_edge([1, 2], [4, 7], true, &symmetry)
            .unwrap();
        assert_eq!(joined.vertices, mesh.vertices);
        assert_eq!(joined.quads.len(), 3);
        assert!(joined.loose_edges.is_empty());
        assert_eq!(&joined.uv.corners[..4], mesh.uv.corners);
        assert!(joined.uv.current(&joined));
        assert!(!joined.uv.mapped(4) && !joined.uv.mapped(5));
        assert!(!joined.open_model_edges().contains_key(&[1, 2]));
        assert!(!joined.open_model_edges().contains_key(&[4, 7]));
        manifold(&joined).unwrap();
    }

    #[test]
    fn extrusion_snap_join_protects_targets_and_rejects_obstructions() {
        let mesh = panels();
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        assert!(mesh.connect_extruded_point(1, 2, true, &symmetry).is_err());
        assert!(mesh.connect_extruded_point(1, 99, true, &symmetry).is_err());
        assert!(
            mesh.join_extruded_edge([1, 2], [2, 3], true, &symmetry)
                .is_err()
        );
        for hidden in [false, true] {
            let mut protected = mesh.clone();
            if hidden {
                protected.hidden_faces[2] = true;
                protected.hidden_faces[3] = true;
            } else {
                protected.mask[4] = 1.;
            }
            assert!(
                protected
                    .connect_extruded_point(1, 4, true, &symmetry)
                    .is_err()
            );
            assert!(
                protected
                    .join_extruded_edge([1, 2], [4, 7], true, &symmetry)
                    .is_err()
            );
        }
        let mut obstructed = mesh.clone();
        obstructed
            .vertices
            .extend([Vec3(0., -0.2, -0.5), Vec3(0., 0.2, -0.5), Vec3(0., 0., 0.5)]);
        obstructed.mask.resize(11, 0.);
        obstructed.push_polygon(&[8, 9, 10], 0, false);
        obstructed.uv = Default::default();
        let sym = ModelingSymmetry::new(&obstructed, &Symmetry::default()).unwrap();
        assert!(
            obstructed
                .join_extruded_edge([1, 2], [4, 7], true, &sym)
                .is_err()
        );
    }

    #[test]
    fn extrusion_snap_join_handles_mirrored_targets_once() {
        let mesh = panels();
        let settings = Symmetry {
            enabled: true,
            mirror: [true, false, false],
            ..Default::default()
        };
        let symmetry = ModelingSymmetry::new(&mesh, &settings).unwrap();
        let (out, tips) = mesh.connect_extruded_point(1, 4, true, &symmetry).unwrap();
        assert_eq!(out.loose_edges, vec![[1, 4]]);
        assert_eq!(tips, vec![1, 4]);
        let out = mesh
            .join_extruded_edge([1, 2], [4, 7], true, &symmetry)
            .unwrap();
        assert_eq!(out.quads.len(), 3);
        symmetry.validate_result(&out).unwrap();
    }
}
