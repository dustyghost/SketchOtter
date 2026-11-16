//! Ctrl-extrude copies retain their originals and have no connecting side walls.
use super::*;

impl Mesh {
    pub(crate) fn duplicate_extruded_faces(
        &self,
        selected: &[usize],
        distance: f32,
        individual: bool,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Extrusion, String> {
        manifold(self)?;
        if !distance.is_finite() {
            return Err("Use a finite distance".into());
        }
        if !self.uv.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear inactive UVs before duplicating faces".into());
        }
        let polygons = self.polygons();
        if selected.is_empty() || selected.iter().any(|f| *f >= polygons.len()) {
            return Err("Select current faces".into());
        }
        let selected: BTreeSet<_> = selected
            .iter()
            .flat_map(|&f| symmetry.face_copies(f))
            .collect();
        let touched: Vec<_> = selected
            .iter()
            .flat_map(|&f| polygons[f].vertices.iter().copied())
            .collect();
        self.protect_model_vertices(&touched, protect)?;
        let mut out = self.clone();
        if distance.abs() >= 1e-7 {
            let groups = if individual {
                selected.iter().map(|&f| vec![f]).collect()
            } else {
                vec![selected.iter().copied().collect::<Vec<_>>()]
            };
            let source_triangles = crate::sculpt::uv::polygon_triangles(self);
            let mut shading = self.shading_edges().to_vec();
            out.polygroups.resize(self.triangles.len(), 0);
            out.hidden_faces.resize(self.triangles.len(), false);
            if self.uv.current(self) {
                out.uv.removed.resize(self.triangles.len(), false);
            }
            for faces in groups {
                let mut normals = BTreeMap::<usize, Vec3>::new();
                for &f in &faces {
                    let n = normal(self, &polygons[f]);
                    for &v in &polygons[f].vertices {
                        let sum = normals.entry(v).or_default();
                        *sum = *sum + n;
                    }
                }
                if normals.values().any(|n| n.len() < 1e-7) {
                    return Err("Selected normals cancel; choose a smaller region".into());
                }
                let mapping: BTreeMap<_, _> = normals
                    .iter()
                    .map(|(&v, &n)| {
                        (
                            v,
                            out.append_blend(
                                self,
                                &[(v, 1.)],
                                self.vertices[v] + n.norm() * distance,
                            ),
                        )
                    })
                    .collect();
                for &f in &faces {
                    let p = &polygons[f];
                    if p.vertices.len() == 4 {
                        out.quads
                            .push(std::array::from_fn(|i| mapping[&p.vertices[i]]));
                    }
                    for &t in &source_triangles[f] {
                        out.triangles.push(self.triangles[t].map(|v| mapping[&v]));
                        out.polygroups
                            .push(self.polygroups.get(t).copied().unwrap_or(0));
                        out.hidden_faces.push(false);
                        if self.uv.current(self) {
                            out.uv.corners.push(self.uv.corners[t]);
                            out.uv.removed.push(!self.uv.mapped(t));
                            if !self.uv.masks.is_empty() {
                                out.uv.masks.push(self.uv.masks[t]);
                            }
                        }
                    }
                }
                let copied = |edges: &[[usize; 2]]| {
                    edges
                        .iter()
                        .filter_map(|e| Some(edge(*mapping.get(&e[0])?, *mapping.get(&e[1])?)))
                        .collect::<Vec<_>>()
                };
                out.creases.extend(copied(&self.creases));
                shading.extend(copied(self.shading_edges()));
                if self.uv.current(self) {
                    out.uv.seams.extend(copied(&self.uv.seams));
                }
            }
            out.creases.sort_unstable();
            out.creases.dedup();
            if self.uv.current(self) {
                out.uv.topology = out.triangles.clone();
                out.uv.seams.sort_unstable();
                out.uv.seams.dedup();
            }
            out.set_shading_seams(shading);
            manifold(&out)?;
            symmetry.validate_result(&out)?;
        }
        Ok(Extrusion {
            mesh: out,
            distance,
            joined: false,
            collapsed: false,
            partial_contact: false,
            varying_distance: false,
            rim: Vec::new(),
            preview: Default::default(),
        })
    }

    /// Convert a point-extrusion draft into isolated copies, retaining original points.
    pub(crate) fn detach_extruded_points(&self, out: &mut Mesh) -> Result<(), String> {
        let original: BTreeSet<_> = self.loose_edges.iter().copied().collect();
        out.loose_edges.retain(|e| original.contains(e));
        out.loose_vertices = self.loose_vertices.clone();
        out.loose_vertices
            .extend(self.vertices.len()..out.vertices.len());
        out.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detached_copy_faces_keep_originals_connections_uvs_and_groups() {
        let mut grid = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Grid);
        grid.divisions = [2, 1, 1];
        let mut mesh = grid.mesh().unwrap();
        mesh.create_vertex_group("Source".into(), 0.5, true)
            .unwrap();
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = vec![[[0., 0.], [1., 0.], [1., 1.]]; mesh.triangles.len()];
        let sym = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let out = mesh
            .duplicate_extruded_faces(&[0, 1], 0.3, false, true, &sym)
            .unwrap()
            .mesh;
        assert_eq!(&out.vertices[..mesh.vertices.len()], mesh.vertices);
        assert_eq!(&out.triangles[..mesh.triangles.len()], mesh.triangles);
        assert_eq!(out.vertices.len(), mesh.vertices.len() * 2);
        assert_eq!(out.quads.len(), 4);
        assert_eq!(
            out.uv.corners[..mesh.triangles.len()],
            out.uv.corners[mesh.triangles.len()..]
        );
        assert!(out.uv.current(&out));
        assert_eq!(out.vertex_groups[0].weights.len(), out.vertices.len());
        assert!(
            out.triangles
                .iter()
                .all(|t| t.iter().all(|v| *v < mesh.vertices.len())
                    || t.iter().all(|v| *v >= mesh.vertices.len()))
        );
        let separate = mesh
            .duplicate_extruded_faces(&[0, 1], 0.3, true, true, &sym)
            .unwrap()
            .mesh;
        assert_eq!(separate.vertices.len(), mesh.vertices.len() + 8);
        mesh.mask[0] = 1.;
        assert!(
            mesh.duplicate_extruded_faces(&[0, 1], 0.3, false, true, &sym)
                .is_err()
        );
    }
    #[test]
    fn detached_copy_isolated_vertices_survive_modeling_and_exchange() {
        let mut mesh = Mesh {
            vertices: vec![Vec3(-1., 0., 0.), Vec3(0., 0., 0.), Vec3(1., 0., 0.)],
            mask: vec![0.; 3],
            loose_edges: vec![[0, 1]],
            loose_vertices: vec![2],
            ..Default::default()
        };
        let sym = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let mut out = mesh
            .extrude_points_by(&[2], mesh.vertices[2], Vec3(0., 0.5, 0.), &sym)
            .unwrap();
        mesh.detach_extruded_points(&mut out).unwrap();
        assert_eq!(out.loose_vertices, vec![2, 3]);
        assert_eq!(out.loose_edges, mesh.loose_edges);
        let sym = ModelingSymmetry::new(&out, &Default::default()).unwrap();
        let extended = out
            .extrude_points_by(&[3], out.vertices[3], Vec3(0.5, 0., 0.), &sym)
            .unwrap();
        assert_eq!(extended.loose_vertices, vec![2]);
        assert_eq!(extended.loose_edges, vec![[0, 1], [3, 4]]);
        let deleted = out
            .edit_model_topology(TopologyEdit::DeleteVertices, &[], &[], &[0], &sym)
            .unwrap();
        assert_eq!(deleted.loose_vertices.len(), 2);
        assert_eq!(deleted.vertices.len(), 2);
        let mut scene = crate::sculpt::Geometry::new(mesh.clone());
        scene
            .import_mesh(deleted.clone(), "Isolated vertices")
            .unwrap();
        assert_eq!(scene.mesh, deleted);
        let path =
            std::env::temp_dir().join(format!("clay-detached-points-{}.obj", std::process::id()));
        crate::persistence::export_obj(path.to_str().unwrap(), &out).unwrap();
        let restored = crate::persistence::load_obj(path.to_str().unwrap()).unwrap();
        assert_eq!(restored.loose_vertices, out.loose_vertices);
        assert_eq!(restored.loose_edges, out.loose_edges);
        std::fs::remove_file(path).unwrap();
        let cube = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube)
            .mesh()
            .unwrap();
        let mixed = cube.merged_with(&out).unwrap();
        assert_eq!(
            mixed.subdivide().unwrap().loose_vertices,
            mixed.loose_vertices
        );
        mesh.mask[2] = 1.;
        assert!(
            mesh.check_model_protection(
                &Mesh {
                    loose_vertices: vec![],
                    ..mesh.clone()
                },
                true
            )
            .is_err()
        );
    }
}
