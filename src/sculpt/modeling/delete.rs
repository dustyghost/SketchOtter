//! Target-based deletion preserves unrelated components and protected borders.
use super::*;

impl Mesh {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn delete_model_components(
        &self,
        faces: &[usize],
        edges: &[[usize; 2]],
        vertices: &[usize],
        keep_vertices: bool,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        let polygons = self.polygons();
        if faces.iter().any(|&f| f >= polygons.len())
            || edges
                .iter()
                .flatten()
                .chain(vertices)
                .any(|&v| v >= self.vertices.len())
        {
            return Err("Select components on the current mesh".into());
        }
        let blocked = self.protected_model_vertices(protect);
        let allowed = |vs: &[usize]| vs.iter().all(|v| !blocked.contains(v));
        let faces: BTreeSet<_> = faces
            .iter()
            .filter(|&&f| {
                symmetry
                    .face_copies(f)
                    .iter()
                    .all(|&f| allowed(&polygons[f].vertices))
            })
            .flat_map(|&f| symmetry.face_copies(f))
            .collect();
        let edge_allowed = |e: &[usize; 2]| {
            allowed(e)
                && polygons
                    .iter()
                    .filter(|p| p.edges().any(|(a, b)| edge(a, b) == *e))
                    .all(|p| allowed(&p.vertices))
        };
        let edges: BTreeSet<_> = edges
            .iter()
            .filter(|&&e| symmetry.edges(&[e]).iter().all(edge_allowed))
            .flat_map(|&e| symmetry.edges(&[e]))
            .collect();
        let vertex_allowed = |v: usize| {
            !blocked.contains(&v)
                && polygons
                    .iter()
                    .filter(|p| p.vertices.contains(&v))
                    .all(|p| allowed(&p.vertices))
        };
        let vertices: BTreeSet<_> = vertices
            .iter()
            .filter(|&&v| symmetry.vertex_copies(v).into_iter().all(vertex_allowed))
            .flat_map(|&v| symmetry.vertex_copies(v))
            .collect();
        if faces.is_empty() && edges.is_empty() && vertices.is_empty() {
            return Ok(self.clone());
        }
        let mut out = self.empty_topology();
        for (f, p) in polygons.iter().enumerate() {
            if !faces.contains(&f)
                && !p.vertices.iter().any(|v| vertices.contains(v))
                && !p.edges().any(|(a, b)| edges.contains(&edge(a, b)))
            {
                out.push_polygon(&p.vertices, p.group, p.hidden);
            }
        }
        out.loose_edges
            .retain(|e| !edges.contains(e) && !e.iter().any(|v| vertices.contains(v)));
        out.loose_vertices.retain(|v| !vertices.contains(v));
        if keep_vertices {
            let used: BTreeSet<_> = out
                .triangles
                .iter()
                .flatten()
                .chain(out.loose_edges.iter().flatten())
                .copied()
                .collect();
            out.loose_vertices.extend(
                (0..self.vertices.len()).filter(|v| !vertices.contains(v) && !used.contains(v)),
            );
            out.loose_vertices.sort_unstable();
            out.loose_vertices.dedup();
        }
        out.retain_creases();
        out.preserve_model_uvs(self, &BTreeMap::new());
        out.compact_model_vertices();
        self.check_model_protection(&out, protect)?;
        manifold(&out)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }
}
