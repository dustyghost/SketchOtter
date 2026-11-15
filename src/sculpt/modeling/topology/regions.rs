//! Conservative face edits: shell orientation and lossless planar dissolution.
use super::*;

type Corner = ([f32; 2], f32);

impl Mesh {
    pub(super) fn edit_face_regions(
        &self,
        edit: TopologyEdit,
        faces: &[usize],
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        manifold(self)?;
        crate::sculpt::fusion::validate_surface(self)?;
        if !self.uv.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear the inactive UV map before editing face regions".into());
        }
        let polygons = self.polygons();
        if faces.is_empty() || faces.iter().any(|&f| f >= polygons.len()) {
            return Err("Select current faces on the active model".into());
        }
        let selected: BTreeSet<_> = faces
            .iter()
            .flat_map(|&f| symmetry.face_copies(f))
            .collect();
        if selected.len() > 4096 {
            return Err("Edit at most 4096 faces in one region preview".into());
        }
        let touched: Vec<_> = selected
            .iter()
            .flat_map(|&f| polygons[f].vertices.iter().copied())
            .collect();
        self.protect_model_vertices(&touched, false)?;
        let mut incidence = BTreeMap::<_, Vec<usize>>::new();
        for (f, p) in polygons.iter().enumerate() {
            for (a, b) in p.edges() {
                incidence.entry(edge(a, b)).or_default().push(f);
            }
        }
        let triangles = crate::sculpt::uv::polygon_triangles(self);
        let mut out = if edit == TopologyEdit::FlipFaces {
            if incidence.values().any(|fs| {
                fs.iter().any(|f| selected.contains(f)) && fs.iter().any(|f| !selected.contains(f))
            }) {
                return Err("Flip requires complete connected shells. Use Connected selection; flipping an attached subset would break shared-edge orientation".into());
            }
            let mut out = self.clone();
            for &f in &selected {
                if f < self.quads.len() {
                    out.quads[f].swap(1, 3);
                }
                for &t in &triangles[f] {
                    out.triangles[t].swap(1, 2);
                    if self.uv.current(self) {
                        out.uv.corners[t].swap(1, 2);
                        if !out.uv.masks.is_empty() {
                            out.uv.masks[t].swap(1, 2);
                        }
                    }
                }
            }
            if self.uv.current(self) {
                out.uv.topology = out.triangles.clone();
            }
            out
        } else {
            self.dissolve_face_regions(&selected, &polygons, &incidence, &triangles)?
        };
        if !self.shading_edges().is_empty() {
            out.set_shading_seams(self.shading_edges().to_vec());
        }
        manifold(&out)?;
        crate::sculpt::fusion::validate_surface(&out)?;
        // Compact only dissolution's unused interior vertices, carrying hard-edge
        // indices as well as UV seams through the same remapping.
        if edit == TopologyEdit::DissolveFaces {
            let used: BTreeSet<_> = out.triangles.iter().flatten().copied().collect();
            let mapping: BTreeMap<_, _> = used.iter().enumerate().map(|(i, &v)| (v, i)).collect();
            let hard: Vec<_> = out
                .shading_edges()
                .iter()
                .filter_map(|&[a, b]| Some(edge(*mapping.get(&a)?, *mapping.get(&b)?)))
                .collect();
            out = crate::sculpt::fusion::compact(out);
            if !hard.is_empty() {
                out.set_shading_seams(hard);
            }
        }
        symmetry.validate_result(&out)?;
        Ok(out)
    }

    fn dissolve_face_regions(
        &self,
        selected: &BTreeSet<usize>,
        polygons: &[Polygon],
        incidence: &BTreeMap<[usize; 2], Vec<usize>>,
        triangles: &[Vec<usize>],
    ) -> Result<Mesh, String> {
        let removed: BTreeSet<_> = selected
            .iter()
            .flat_map(|&f| triangles[f].iter().copied())
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
            .filter(|(f, _)| !selected.contains(f))
            .map(|(_, &q)| q)
            .collect();
        if self.uv.current(self) {
            out.uv.corners = keep.iter().map(|&f| self.uv.corners[f]).collect();
            out.uv.removed = keep.iter().map(|&f| !self.uv.mapped(f)).collect();
            if !self.uv.masks.is_empty() {
                out.uv.masks = keep.iter().map(|&f| self.uv.masks[f]).collect();
            }
        }
        let first_new = out.triangles.len();
        let mut pending = selected.clone();
        while let Some(&first) = pending.first() {
            let mut region = BTreeSet::new();
            let mut queue = vec![first];
            while let Some(f) = queue.pop() {
                if !pending.remove(&f) {
                    continue;
                }
                region.insert(f);
                for (a, b) in polygons[f].edges() {
                    queue.extend(
                        incidence[&edge(a, b)]
                            .iter()
                            .filter(|f| pending.contains(f)),
                    );
                }
            }
            if region.len() < 2 {
                return Err("Select at least two adjoining coplanar faces in each region".into());
            }
            let group = polygons[first].group;
            if region.iter().any(|&f| polygons[f].group != group) {
                return Err(
                    "Dissolve would erase a face-group boundary; select one group at a time".into(),
                );
            }
            let mut rim = BTreeMap::new();
            let mut inner_edges = BTreeSet::new();
            for &f in &region {
                for (a, b) in polygons[f].edges() {
                    let e = edge(a, b);
                    if incidence[&e].iter().filter(|f| region.contains(f)).count() == 2 {
                        inner_edges.insert(e);
                    } else if rim.insert(a, b).is_some() {
                        return Err(
                            "The region boundary branches; select a simple face patch".into()
                        );
                    }
                }
            }
            // Triangle-corner seams can also lie on a retained quad's internal
            // diagonal, which is absent from the logical polygon edge view.
            let boundary_edges: BTreeSet<_> = rim.iter().map(|(&a, &b)| edge(a, b)).collect();
            for &f in &region {
                for &t in &triangles[f] {
                    let t = self.triangles[t];
                    for i in 0..3 {
                        let e = edge(t[i], t[(i + 1) % 3]);
                        if !boundary_edges.contains(&e) {
                            inner_edges.insert(e);
                        }
                    }
                }
            }
            if self
                .creases
                .iter()
                .chain(self.shading_edges())
                .chain(&self.uv.seams)
                .any(|e| inner_edges.contains(e))
            {
                return Err("Dissolve would erase a crease, hard edge or UV seam; remove it explicitly or change the region".into());
            }
            let start = *rim
                .first_key_value()
                .ok_or("Dissolve needs a region with an outer boundary")?
                .0;
            let mut cycle = vec![start];
            let mut at = start;
            loop {
                at = rim
                    .remove(&at)
                    .ok_or("The region boundary is not a simple loop")?;
                if at == start {
                    break;
                }
                if cycle.contains(&at) {
                    return Err("The region boundary crosses itself".into());
                }
                cycle.push(at);
            }
            if !rim.is_empty() {
                return Err(
                    "Dissolve does not fill holes; select a region with one boundary".into(),
                );
            }
            let region_vertices: BTreeSet<_> = region
                .iter()
                .flat_map(|&f| polygons[f].vertices.iter().copied())
                .collect();
            let origin = self.vertices[cycle[0]];
            let scale = region_vertices
                .iter()
                .map(|&v| (self.vertices[v] - origin).len())
                .fold(0_f32, f32::max);
            let n = normal(self, &polygons[first]);
            if region_vertices
                .iter()
                .any(|&v| (self.vertices[v] - origin).dot(n).abs() > scale * 1e-6)
                || region
                    .iter()
                    .any(|&f| normal(self, &polygons[f]).dot(n) < 0.99999)
            {
                return Err("Dissolve preserves flat surfaces only; this region contains curvature or sculpt detail".into());
            }
            // Retriangulation can change interpolation even when every retained
            // vertex survives. Conservatively require uniform painted channels.
            let v0 = cycle[0];
            let material = self.material(v0);
            if region_vertices.iter().any(|&v| {
                self.mask[v] != self.mask[v0]
                    || self.material(v) != material
                    || self
                        .vertex_groups
                        .iter()
                        .any(|g| g.weights[v] != g.weights[v0])
            }) {
                return Err("Dissolve would change painted material, mask or vertex-group detail; use a region with uniform vertex attributes".into());
            }
            let source_faces: Vec<_> = region
                .iter()
                .flat_map(|&f| triangles[f].iter().copied())
                .collect();
            let uv = self.dissolve_region_uv(&source_faces)?;
            let (patch, quad) = manual::triangulate_boundary(self, &cycle)?;
            // Ensure tessellation covers the same flat area; rejects folded or
            // overlapping source patches rather than silently flattening them.
            let area = |t: [usize; 3]| {
                let [a, b, c] = t.map(|v| self.vertices[v]);
                (b - a).cross(c - a).len()
            };
            let before: f32 = source_faces.iter().map(|&f| area(self.triangles[f])).sum();
            let after: f32 = patch.iter().copied().map(area).sum();
            if (before - after).abs() > before * 1e-5 {
                return Err(
                    "The region cannot be dissolved without changing its covered surface".into(),
                );
            }
            if quad {
                out.quads.push([cycle[0], cycle[1], cycle[2], cycle[3]]);
            }
            for t in patch {
                out.push_polygon(&t, group, false);
                if self.uv.current(self) {
                    out.uv
                        .corners
                        .push(uv.as_ref().map_or([[0.; 2]; 3], |uv| t.map(|v| uv[&v].0)));
                    out.uv.removed.push(uv.is_none());
                    if !self.uv.masks.is_empty() {
                        out.uv
                            .masks
                            .push(uv.as_ref().map_or([0.; 3], |uv| t.map(|v| uv[&v].1)));
                    }
                }
            }
        }
        out.retain_creases();
        if self.uv.current(self) {
            out.uv.topology = out.triangles.clone();
        }
        crate::sculpt::fusion::validate_added_faces(&out, first_new, &|| false).map_err(|_| {
            "The dissolved region overlaps another surface; change the selection".to_string()
        })?;
        Ok(out)
    }

    fn dissolve_region_uv(
        &self,
        faces: &[usize],
    ) -> Result<Option<BTreeMap<usize, Corner>>, String> {
        if !self.uv.current(self) || faces.iter().all(|&f| !self.uv.mapped(f)) {
            return Ok(None);
        }
        if faces.iter().any(|&f| !self.uv.mapped(f)) {
            return Err("Dissolve cannot mix mapped and unmapped UV faces".into());
        }
        let mut corners = BTreeMap::new();
        for &f in faces {
            for (i, &v) in self.triangles[f].iter().enumerate() {
                let value = (
                    self.uv.corners[f][i],
                    self.uv.masks.get(f).map_or(0., |m| m[i]),
                );
                if corners.insert(v, value).is_some_and(|old| old != value) {
                    return Err("Dissolve would erase a UV or UV-mask seam".into());
                }
            }
        }
        // A single affine map across the patch preserves texture interpolation
        // under any valid retriangulation, including removing interior vertices.
        let tri = self.triangles[faces[0]];
        let [a, b, c] = tri.map(|v| self.vertices[v]);
        let u = b - a;
        let v = c - a;
        let uu = u.dot(u);
        let uv = u.dot(v);
        let vv = v.dot(v);
        let det = uu * vv - uv * uv;
        for (&vertex, &(coords, mask)) in &corners {
            let p = self.vertices[vertex] - a;
            let y = (vv * p.dot(u) - uv * p.dot(v)) / det;
            let z = (uu * p.dot(v) - uv * p.dot(u)) / det;
            let w = [1. - y - z, y, z];
            for (channel, actual) in [coords[0], coords[1], mask].into_iter().enumerate() {
                let expected: f32 = tri
                    .iter()
                    .zip(w)
                    .map(|(&v, w)| {
                        w * match channel {
                            0 => corners[&v].0[0],
                            1 => corners[&v].0[1],
                            _ => corners[&v].1,
                        }
                    })
                    .sum();
                if !expected.is_finite() || (expected - actual).abs() > 1e-6 {
                    return Err("Dissolve would distort UV or UV-mask detail; use an affine planar UV patch".into());
                }
            }
        }
        Ok(Some(corners))
    }
}
