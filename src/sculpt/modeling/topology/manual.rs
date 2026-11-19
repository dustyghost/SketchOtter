//! Manual construction keeps all surviving vertex indices and face-corner attributes.
use super::*;

const MAX_BOUNDARY: usize = 1024;

impl Mesh {
    pub(super) fn manual_topology(
        &self,
        edit: TopologyEdit,
        edges: &[[usize; 2]],
        vertices: &[usize],
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        if vertices
            .iter()
            .chain(edges.iter().flatten())
            .any(|&v| v >= self.vertices.len())
        {
            return Err("The component selection is stale; select it again".into());
        }
        manifold(self)?;
        if !self.uv.is_empty() && !self.uv.current(self) && !self.triangles.is_empty() {
            let mut source = self.clone();
            source.preserve_model_uvs(self, &BTreeMap::new());
            return source.manual_topology(edit, edges, vertices, symmetry);
        }
        let points: Vec<_> = vertices
            .iter()
            .chain(edges.iter().flatten())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut out = if edit == TopologyEdit::Connect {
            self.connect_quad(vertices, symmetry)?
        } else if edit == TopologyEdit::CreateFace && (3..=4).contains(&points.len()) {
            self.create_selected_face(&points, symmetry)?
        } else {
            self.fill_selected_boundary(edit, edges, &points, symmetry)?
        };
        // Surviving vertex indices and hard edges are unchanged. Refresh the
        // topology key so the new faces do not disable shading on unrelated edges.
        if !self.shading_edges().is_empty() {
            out.set_shading_seams(self.shading_edges().to_vec());
        }
        manifold(&out)?;
        crate::sculpt::fusion::validate_surface(&out)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }

    fn connect_quad(
        &self,
        vertices: &[usize],
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        let wanted: BTreeSet<_> = vertices.iter().copied().collect();
        if wanted.len() < 2 || wanted.iter().any(|&v| v >= self.vertices.len()) {
            return Err("Select two vertices on the same quad".into());
        }
        let polygons = self.polygons();
        // A selection may include the exact mirror/radial copies of the pair.
        let pair = if wanted.len() == 2 {
            [*wanted.first().unwrap(), *wanted.last().unwrap()]
        } else {
            self.quads.iter().flat_map(|q| [[q[0], q[2]], [q[1], q[3]]])
            .find(|pair| symmetry.edge_copies(*pair).into_iter().flatten().collect::<BTreeSet<_>>() == wanted)
            .ok_or("Select opposite corners of one quad and its symmetry copies. Existing edges are not duplicated; loose edges are not supported")?
        };
        let mut split = BTreeMap::new();
        for [a, b] in symmetry.edge_copies(pair) {
            let face = polygons
                .iter()
                .position(|p| {
                    p.visible
                        && p.vertices.len() == 4
                        && p.vertices.contains(&a)
                        && p.vertices.contains(&b)
                        && !p.edges().any(|(u, v)| edge(u, v) == edge(a, b))
                })
                .ok_or("Each selected pair must be opposite corners of a visible quad")?;
            if split
                .insert(face, edge(a, b))
                .is_some_and(|old| old != edge(a, b))
            {
                return Err("Symmetry would create crossing diagonals in one face".into());
            }
        }
        let source_triangles = crate::sculpt::uv::polygon_triangles(self);
        let removed: BTreeSet<_> = split
            .keys()
            .flat_map(|&f| source_triangles[f].iter().copied())
            .collect();
        let keep: Vec<_> = (0..self.triangles.len())
            .filter(|f| !removed.contains(f))
            .collect();
        let mut out = self.clone();
        out.triangles = keep.iter().map(|&f| self.triangles[f]).collect();
        out.polygroups = keep
            .iter()
            .map(|&f| self.polygroups.get(f).copied().unwrap_or(0))
            .collect();
        out.hidden_faces = keep
            .iter()
            .map(|&f| self.hidden_faces.get(f).copied().unwrap_or(false))
            .collect();
        out.quads = self
            .quads
            .iter()
            .enumerate()
            .filter(|(f, _)| !split.contains_key(f))
            .map(|(_, q)| *q)
            .collect();
        if self.uv.current(self) {
            out.uv.corners = keep.iter().map(|&f| self.uv.corners[f]).collect();
            out.uv.removed = keep.iter().map(|&f| !self.uv.mapped(f)).collect();
            if !self.uv.masks.is_empty() {
                out.uv.masks = keep.iter().map(|&f| self.uv.masks[f]).collect();
            }
        }
        let first_new = out.triangles.len();
        for (face, diagonal) in split {
            let p = &polygons[face];
            let start = p.vertices.iter().position(|&v| v == diagonal[0]).unwrap();
            let q: Vec<_> = (0..4).map(|i| p.vertices[(i + start) % 4]).collect();
            let mut uv = BTreeMap::new();
            let mapped =
                self.uv.current(self) && source_triangles[face].iter().all(|&f| self.uv.mapped(f));
            if self.uv.current(self)
                && source_triangles[face].iter().any(|&f| self.uv.mapped(f))
                && !mapped
            {
                return Err("This quad mixes mapped and unmapped UV triangles; repair its UV mapping before connecting".into());
            }
            if mapped {
                for &f in &source_triangles[face] {
                    for (corner, &v) in self.triangles[f].iter().enumerate() {
                        let value = (
                            self.uv.corners[f][corner],
                            self.uv.masks.get(f).map_or(0., |m| m[corner]),
                        );
                        if uv
                            .insert(v, value)
                            .is_some_and(|previous| previous != value)
                        {
                            return Err("This quad has a UV or UV-mask seam across its internal diagonal; split an unambiguous quad instead".into());
                        }
                    }
                }
            }
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                out.push_polygon(&t, p.group, false);
                if self.uv.current(self) {
                    out.uv.corners.push(if mapped {
                        t.map(|v| uv[&v].0)
                    } else {
                        [[0.; 2]; 3]
                    });
                    if !self.uv.masks.is_empty() {
                        out.uv
                            .masks
                            .push(if mapped { t.map(|v| uv[&v].1) } else { [0.; 3] });
                    }
                    out.uv.removed.push(!mapped);
                }
            }
        }
        if self.uv.current(self) {
            out.uv.topology = out.triangles.clone();
        }
        crate::sculpt::fusion::validate_added_faces(&out, first_new, &|| false).map_err(|_| {
            "The new diagonal would make a face overlap another surface".to_string()
        })?;
        Ok(out)
    }

    fn fill_selected_boundary(
        &self,
        edit: TopologyEdit,
        edges: &[[usize; 2]],
        vertices: &[usize],
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        let selected = if edit == TopologyEdit::CreateFace {
            let wanted: BTreeSet<_> = vertices
                .iter()
                .flat_map(|&v| symmetry.vertex_copies(v))
                .collect();
            if wanted.len() < 3 || wanted.iter().any(|&v| v >= self.vertices.len()) {
                return Err(
                    "Select at least three vertices around a complete open boundary".into(),
                );
            }
            let boundaries = crate::sculpt::fusion::boundaries(self)?;
            let selected: Vec<_> = boundaries
                .iter()
                .filter(|b| b.vertices.iter().all(|v| wanted.contains(v)))
                .flat_map(|b| {
                    b.vertices
                        .iter()
                        .copied()
                        .zip(b.vertices.iter().copied().cycle().skip(1))
                        .take(b.vertices.len())
                        .map(|(a, b)| edge(a, b))
                })
                .collect();
            if selected.iter().flatten().copied().collect::<BTreeSet<_>>() != wanted {
                return Err("Select every vertex of a complete opening. Loose vertices and partial boundaries cannot define a face; use edge extrusion to extend a surface".into());
            }
            selected
        } else {
            symmetry.edges(edges)
        };
        let loops = self.selected_boundaries(&selected)?;
        let mut out = self.clone();
        out.polygroups.resize(self.triangles.len(), 0);
        out.hidden_faces.resize(self.triangles.len(), false);
        for mut cycle in loops {
            cycle.reverse(); // Opposite winding on each shared boundary edge.
            let (triangles, quad) = triangulate_boundary(self, &cycle)?;
            if quad {
                out.quads.push([cycle[0], cycle[1], cycle[2], cycle[3]]);
            }
            for triangle in triangles {
                out.push_polygon(&triangle, 0, false);
            }
        }
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
            .map_err(|_| "The proposed face intersects an existing surface; change the boundary or remove the obstruction".to_string())?;
        Ok(out)
    }
}

pub(in crate::sculpt::modeling) fn triangulate_boundary(
    mesh: &Mesh,
    cycle: &[usize],
) -> Result<(Vec<[usize; 3]>, bool), String> {
    if !(3..=MAX_BOUNDARY).contains(&cycle.len()) {
        return Err(format!(
            "Basic filling supports 3–{MAX_BOUNDARY} boundary vertices"
        ));
    }
    let origin = mesh.vertices[cycle[0]];
    let scale = cycle
        .iter()
        .map(|&v| (mesh.vertices[v] - origin).len())
        .fold(0_f32, f32::max);
    let n = normal(
        mesh,
        &Polygon {
            vertices: cycle.to_vec(),
            group: 0,
            hidden: false,
            visible: true,
        },
    );
    if scale < 1e-8 || n.len() < 0.9 {
        return Err("The selected boundary is collapsed or has ambiguous winding".into());
    }
    if cycle
        .iter()
        .any(|&v| (mesh.vertices[v] - origin).dot(n).abs() > scale * 0.05)
    {
        return Err("Basic filling needs a planar or mildly nonplanar boundary (depth within 5% of its span)".into());
    }
    let u = (mesh.vertices[cycle[1]] - origin).norm();
    let v = n.cross(u).norm();
    let u = v.cross(n).norm();
    let points: Vec<[f64; 2]> = cycle
        .iter()
        .map(|&i| {
            let p = mesh.vertices[i] - origin;
            [
                p.dot(u) as f64 / scale as f64,
                p.dot(v) as f64 / scale as f64,
            ]
        })
        .collect();
    let orient = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        for j in i + 1..points.len() {
            let c = points[j];
            if (a[0] - c[0]).hypot(a[1] - c[1]) < 1e-6 {
                return Err("The boundary contains coincident or collapsed corners".into());
            }
            if j == i + 1 || i == 0 && j == points.len() - 1 {
                continue;
            }
            let d = points[(j + 1) % points.len()];
            let overlap = (0..2).all(|axis| {
                a[axis].max(b[axis]) + 1e-10 >= c[axis].min(d[axis])
                    && c[axis].max(d[axis]) + 1e-10 >= a[axis].min(b[axis])
            });
            if overlap
                && orient(a, b, c) * orient(a, b, d) <= 1e-12
                && orient(c, d, a) * orient(c, d, b) <= 1e-12
            {
                return Err("The boundary crosses or touches itself; select a simple loop".into());
            }
        }
    }
    let quad = cycle.len() == 4
        && (0..4).all(|i| orient(points[i], points[(i + 1) % 4], points[(i + 2) % 4]) > 1e-10);
    let triangles = if quad {
        vec![
            [cycle[0], cycle[1], cycle[2]],
            [cycle[0], cycle[2], cycle[3]],
        ]
    } else {
        let coords: Vec<_> = points.iter().flatten().copied().collect();
        let indices = earcutr::earcut(&coords, &[], 2)
            .map_err(|_| "Could not triangulate the selected boundary")?;
        if indices.len() != (cycle.len() - 2) * 3 {
            return Err("The boundary cannot be filled without dropping corners".into());
        }
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|i| cycle[i]))
            .collect()
    };
    for t in &triangles {
        let [a, b, c] = t.map(|i| mesh.vertices[i]);
        if (b - a).cross(c - a).dot(n) <= scale * scale * 1e-10 {
            return Err("The boundary would produce a folded or collapsed face".into());
        }
    }
    Ok((triangles, quad))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sculpt::{Material, Symmetry};
    fn tube(points: &[[f32; 3]]) -> Mesh {
        let n = points.len();
        let mut mesh = Mesh {
            vertices: points
                .iter()
                .map(|p| Vec3(p[0], p[1], p[2]))
                .chain(points.iter().map(|p| Vec3(p[0], p[1], -1.)))
                .collect(),
            mask: vec![0.; n * 2],
            ..Default::default()
        };
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.push_polygon(&[j, i, i + n, j + n], 0, false);
        }
        mesh
    }
    fn square() -> Mesh {
        tube(&[[-1., -1., 0.], [1., -1., 0.], [1., 1., 0.], [-1., 1., 0.]])
    }
    fn uv(mesh: &mut Mesh) {
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = mesh
            .triangles
            .iter()
            .map(|t| {
                t.map(|v| {
                    [
                        mesh.vertices[v].0 * 0.2 + 0.5,
                        mesh.vertices[v].2 * 0.2 + 0.5,
                    ]
                })
            })
            .collect();
        mesh.uv.masks = vec![[0.3; 3]; mesh.triangles.len()];
    }
    fn apply(mesh: &Mesh, edit: TopologyEdit, vertices: &[usize]) -> Result<Mesh, String> {
        let symmetry = ModelingSymmetry::new(mesh, &Symmetry::default()).unwrap();
        mesh.edit_model_topology(edit, &[], &[], vertices, &symmetry)
    }
    #[test]
    fn manual_topology_caps_triangles_quads_concavity_and_mild_depth() {
        for points in [
            vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            vec![[-1., -1., 0.], [1., -1., 0.02], [1., 1., 0.], [-1., 1., 0.]],
            vec![
                [0., 0., 0.],
                [2., 0., 0.],
                [2., 1., 0.],
                [1., 1., 0.],
                [1., 2., 0.],
                [0., 2., 0.],
            ],
        ] {
            let n = points.len();
            let mesh = tube(&points);
            let out = apply(&mesh, TopologyEdit::CreateFace, &(0..n).collect::<Vec<_>>()).unwrap();
            assert_eq!(&out.triangles[..mesh.triangles.len()], &mesh.triangles);
            assert_eq!(out.vertices, mesh.vertices);
            assert_eq!(out.triangles.len(), mesh.triangles.len() + n - 2);
            assert_eq!(crate::sculpt::fusion::boundaries(&out).unwrap().len(), 1);
            assert_eq!(out.quads.len(), mesh.quads.len() + usize::from(n == 4));
        }
    }
    #[test]
    fn manual_topology_preserves_attributes_and_marks_only_new_caps_unmapped() {
        let mut mesh = square();
        uv(&mut mesh);
        mesh.materials = vec![Material::from_srgb([0.3, 0.5, 0.7], 0.5, 0.); mesh.vertices.len()];
        mesh.create_vertex_group("Rim".into(), 0., true).unwrap();
        mesh.creases = vec![[0, 1]];
        mesh.set_shading_seams(vec![[0, 1]]);
        let original = mesh.clone();
        let out = apply(&mesh, TopologyEdit::CreateFace, &[0, 1, 2, 3]).unwrap();
        assert_eq!(out.vertices, mesh.vertices);
        assert_eq!(out.mask, mesh.mask);
        assert_eq!(out.materials, mesh.materials);
        assert_eq!(out.vertex_groups, mesh.vertex_groups);
        assert_eq!(out.creases, mesh.creases);
        assert_eq!(out.shading_edges(), mesh.shading_edges());
        assert!(out.uv.current(&out));
        assert_eq!(&out.uv.corners[..mesh.triangles.len()], mesh.uv.corners);
        assert_eq!(&out.uv.masks[..mesh.triangles.len()], mesh.uv.masks);
        assert!(out.uv.removed[mesh.triangles.len()..].iter().all(|v| *v));
        assert_eq!(mesh, original);
        let restored: Mesh = serde_json::from_slice(&serde_json::to_vec(&out).unwrap()).unwrap();
        assert_eq!(restored, out);
    }
    #[test]
    fn manual_topology_connects_either_quad_diagonal_with_uvs_and_rejects_existing_edges() {
        let mut mesh = square();
        uv(&mut mesh);
        let q = mesh.quads[0];
        for pair in [[q[0], q[2]], [q[1], q[3]]] {
            let out = apply(&mesh, TopologyEdit::Connect, &pair).unwrap();
            assert_eq!(out.vertices, mesh.vertices);
            assert_eq!(out.quads.len(), mesh.quads.len() - 1);
            assert!(out.polygon_edges().contains(&edge(pair[0], pair[1])));
            assert!(out.uv.current(&out));
            assert_eq!(out.uv.masks, vec![[0.3; 3]; out.triangles.len()]);
            for (f, t) in out.triangles.iter().enumerate() {
                assert_eq!(
                    out.uv.corners[f],
                    t.map(|v| [
                        mesh.vertices[v].0 * 0.2 + 0.5,
                        mesh.vertices[v].2 * 0.2 + 0.5
                    ])
                );
            }
        }
        assert!(apply(&mesh, TopologyEdit::Connect, &[q[0], q[1]]).is_err());
        assert!(apply(&mesh, TopologyEdit::Connect, &[0, 2]).is_err());
    }
    #[test]
    fn manual_topology_connect_rejects_ambiguous_uv_corners_without_mutating_source() {
        let mut mesh = square();
        uv(&mut mesh);
        let q = mesh.quads[0];
        mesh.uv.corners[0][0][0] += 0.1;
        let before = mesh.clone();
        assert!(
            apply(&mesh, TopologyEdit::Connect, &[q[1], q[3]])
                .unwrap_err()
                .contains("seam")
        );
        assert_eq!(mesh, before);
        uv(&mut mesh);
        mesh.uv.removed = vec![false; mesh.triangles.len()];
        mesh.uv.removed[0] = true;
        assert!(
            apply(&mesh, TopologyEdit::Connect, &[q[0], q[2]])
                .unwrap_err()
                .contains("mapped and unmapped")
        );
    }
    #[test]
    fn manual_topology_rejects_partial_branched_crossing_obstructed_and_stale_boundaries() {
        let mut mesh = square();
        assert!(apply(&mesh, TopologyEdit::CreateFace, &[0, 1]).is_err());
        assert!(apply(&mesh, TopologyEdit::CreateFace, &[0, 1, 2]).is_ok());
        assert!(apply(&mesh, TopologyEdit::CreateFace, &[0, 1, 2, usize::MAX]).is_err());
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        assert!(
            mesh.edit_model_topology(TopologyEdit::Fill, &[], &[[0, 1], [1, 2]], &[], &symmetry)
                .is_err()
        );
        assert!(
            mesh.edit_model_topology(
                TopologyEdit::Fill,
                &[],
                &[[0, 1], [1, 2], [1, 5]],
                &[],
                &symmetry
            )
            .is_err()
        );
        mesh.vertices[2].2 = 1.;
        assert!(apply(&mesh, TopologyEdit::CreateFace, &[0, 1, 2, 3]).is_err());
        let crossing = tube(&[[-1., -1., 0.], [1., 1., 0.], [-1., 1., 0.], [1., -1., 0.]]);
        assert!(apply(&crossing, TopologyEdit::CreateFace, &[0, 1, 2, 3]).is_err());
        let mut obstructed = square();
        obstructed.vertices.extend([
            Vec3(-0.3, 0., -0.5),
            Vec3(0.3, 0., 0.5),
            Vec3(0.3, 0.5, -0.5),
        ]);
        obstructed.mask.resize(11, 0.);
        obstructed.push_polygon(&[8, 9, 10], 0, false);
        assert!(
            apply(&obstructed, TopologyEdit::CreateFace, &[0, 1, 2, 3])
                .unwrap_err()
                .contains("intersects")
        );
    }
    #[test]
    fn manual_topology_fill_respects_exact_mirror_correspondence() {
        let mesh = square();
        let settings = Symmetry {
            enabled: true,
            ..Default::default()
        };
        let symmetry = ModelingSymmetry::new(&mesh, &settings).unwrap();
        let out = mesh
            .edit_model_topology(
                TopologyEdit::Fill,
                &[],
                &[[0, 1], [1, 2], [2, 3], [3, 0]],
                &[],
                &symmetry,
            )
            .unwrap();
        symmetry.validate_result(&out).unwrap();
        assert_eq!(out.quads.len(), 5);
    }
}
