//! Remove complete, nonbranching quad loops without leaving holes or T junctions.
use super::*;
struct QuadLoopSelection {
    polygons: Vec<Polygon>,
    selected: BTreeSet<[usize; 2]>,
    degree: BTreeMap<usize, usize>,
    neighbors: Vec<Vec<usize>>,
    closed: bool,
    retained: BTreeSet<usize>,
}
impl Mesh {
    fn quad_loop_selection(
        &self,
        selected: &[[usize; 2]],
        protected: Option<&BTreeSet<usize>>,
    ) -> Result<QuadLoopSelection, String> {
        manifold(self)?;
        let selected: BTreeSet<_> = selected.iter().map(|e| edge(e[0], e[1])).collect();
        if selected.is_empty() {
            return Err("Select complete edge loops".into());
        }
        let polygons = self.polygons();
        let mut incidence = BTreeMap::<_, Vec<_>>::new();
        let mut at_vertex = BTreeMap::<_, BTreeSet<_>>::new();
        for (f, p) in polygons.iter().enumerate() {
            for (a, b) in p.edges() {
                let e = edge(a, b);
                incidence.entry(e).or_default().push(f);
                for v in e {
                    at_vertex.entry(v).or_default().insert(e);
                }
            }
        }
        let mut degree = BTreeMap::<usize, usize>::new();
        let mut neighbors = vec![Vec::new(); polygons.len()];
        for e in &selected {
            let fs = incidence
                .get(e)
                .ok_or("The selected edge no longer exists")?;
            if fs.len() != 2 {
                return Err("Select interior loops; boundary edges are not supported".into());
            }
            if fs
                .iter()
                .any(|&f| !polygons[f].visible || polygons[f].vertices.len() != 4)
            {
                return Err("Loop deletion needs visible quads on both sides".into());
            }
            for &v in e {
                *degree.entry(v).or_default() += 1;
            }
            neighbors[fs[0]].push(fs[1]);
            neighbors[fs[1]].push(fs[0]);
        }
        let mut retained = BTreeSet::new();
        for (&v, &n) in &degree {
            let edges = &at_vertex[&v];
            let borders = edges.iter().filter(|e| incidence[*e].len() == 1).count();
            let stop = n == 1
                && borders == 0
                && protected.is_some_and(|blocked| {
                    edges
                        .iter()
                        .flat_map(|e| &incidence[e])
                        .any(|&f| polygons[f].vertices.iter().any(|v| blocked.contains(v)))
                });
            if stop {
                retained.insert(v);
            }
            if !(stop
                || (n == 2 && edges.len() == 4 && borders == 0)
                || (n == 1 && edges.len() == 3 && borders == 2))
            {
                return Err(
                    "Select complete, noncrossing loops; open loops must end at the mesh border"
                        .into(),
                );
            }
            if protected.is_none()
                && edges
                    .iter()
                    .flat_map(|e| &incidence[e])
                    .any(|&f| !polygons[f].visible || polygons[f].vertices.len() != 4)
            {
                return Err("Show all adjoining faces and use complete quad loops".into());
            }
        }
        Ok(QuadLoopSelection {
            polygons,
            selected,
            degree,
            neighbors,
            closed: incidence.values().all(|fs| fs.len() == 2),
            retained,
        })
    }

    /// The gap variant removes the adjoining face strip using ordinary face
    /// deletion, after the same complete-loop validation used by dissolution.
    pub(super) fn faces_along_loops(
        &self,
        selected: &[[usize; 2]],
    ) -> Result<BTreeSet<usize>, String> {
        let loops = self.quad_loop_selection(selected, None)?;
        Ok(loops
            .neighbors
            .iter()
            .enumerate()
            .filter_map(|(f, neighbors)| (!neighbors.is_empty()).then_some(f))
            .collect())
    }

    pub fn dissolve_loops(&self, selected: &[[usize; 2]]) -> Result<Mesh, String> {
        self.dissolve_loop_selection(self.quad_loop_selection(selected, None)?)
    }

    /// Stop a removal at protected faces. Retain interior end vertices so that
    /// the untouched neighbouring faces keep their complete shared boundary.
    pub(crate) fn remove_protected_loops(
        &self,
        selected: &[[usize; 2]],
        gap: bool,
        protect: bool,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        if selected.is_empty() {
            return Err("Select edge loops first".into());
        }
        let blocked: BTreeSet<_> = self
            .protected_model_vertices(protect)
            .into_iter()
            .flat_map(|v| symmetry.vertex_copies(v))
            .collect();
        let polygons = self.polygons();
        let allowed = |e: &[usize; 2]| {
            !polygons
                .iter()
                .filter(|p| p.edges().any(|(a, b)| edge(a, b) == *e))
                .any(|p| p.vertices.iter().any(|v| blocked.contains(v)))
        };
        let selected: Vec<_> = selected
            .iter()
            .filter(|&&e| symmetry.edges(&[e]).iter().all(allowed))
            .flat_map(|&e| symmetry.edges(&[e]))
            .collect();
        if selected.is_empty() {
            return Ok(self.clone());
        }
        let loops = self.quad_loop_selection(&selected, Some(&blocked))?;
        let out = if gap {
            let mut out = self.empty_topology();
            for (f, p) in loops.polygons.iter().enumerate() {
                if loops.neighbors[f].is_empty() {
                    out.push_polygon(&p.vertices, p.group, p.hidden);
                }
            }
            out.retain_creases();
            out.preserve_model_uvs(self, &BTreeMap::new());
            out.compact_model_vertices();
            manifold(&out)?;
            out
        } else {
            self.dissolve_loop_selection(loops)?
        };
        self.check_model_protection(&out, protect)?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }

    fn dissolve_loop_selection(&self, loops: QuadLoopSelection) -> Result<Mesh, String> {
        let QuadLoopSelection {
            polygons,
            selected,
            degree,
            neighbors,
            closed,
            retained,
        } = loops;
        let mut out = self.empty_topology();
        let mut visited = BTreeSet::new();
        for first in 0..polygons.len() {
            if !visited.insert(first) {
                continue;
            }
            let mut patch = vec![first];
            let mut pending = vec![first];
            while let Some(f) = pending.pop() {
                for &next in &neighbors[f] {
                    if visited.insert(next) {
                        patch.push(next);
                        pending.push(next);
                    }
                }
            }
            if patch.len() == 1 {
                let p = &polygons[first];
                out.push_polygon(&p.vertices, p.group, p.hidden);
                continue;
            }
            let group = polygons[first].group;
            if patch.iter().any(|&f| polygons[f].group != group) {
                return Err("Dissolving this loop would merge different face-group labels".into());
            }
            let mut next = BTreeMap::new();
            for &f in &patch {
                for (a, b) in polygons[f].edges() {
                    if !selected.contains(&edge(a, b)) && next.insert(a, b).is_some() {
                        return Err(
                            "The selected loops cross or surround an ambiguous region".into()
                        );
                    }
                }
            }
            let &start = next
                .keys()
                .next()
                .ok_or("These loops would remove an entire surface")?;
            let mut boundary = vec![start];
            let mut v = next[&start];
            while v != start {
                if boundary.contains(&v) {
                    return Err("The dissolved boundary branches".into());
                }
                boundary.push(v);
                v = *next.get(&v).ok_or("The dissolved boundary is incomplete")?;
            }
            if boundary.len() != next.len() {
                return Err(
                    "The selected loops surround a hole; dissolve fewer loops at once".into(),
                );
            }
            let corners: Vec<_> = boundary
                .iter()
                .copied()
                .filter(|v| !degree.contains_key(v) || retained.contains(v))
                .collect();
            if corners.len() != 4 && !corners.iter().any(|v| retained.contains(v)) {
                return Err("These loops cannot be dissolved into regular quads".into());
            }
            // Follow each old boundary path to carry its crease onto the new edge.
            for i in 0..corners.len() {
                let a = corners[i];
                let b = corners[(i + 1) % corners.len()];
                let mut v = a;
                let mut creased = false;
                while v != b {
                    let w = next[&v];
                    creased |= self.creases.binary_search(&edge(v, w)).is_ok();
                    v = w;
                }
                if creased {
                    out.creases.push(edge(a, b));
                }
            }
            if corners.iter().any(|v| retained.contains(v)) {
                // A fan from the centre retains collinear end vertices; ear
                // clipping may discard them and leave a T junction at the stop.
                let weight = 1. / corners.len() as f32;
                let center = corners
                    .iter()
                    .fold(Vec3::default(), |p, &v| p + self.vertices[v] * weight);
                let weights: Vec<_> = corners.iter().map(|&v| (v, weight)).collect();
                let mid = out.append_blend(self, &weights, center);
                for i in 0..corners.len() {
                    out.push_polygon(
                        &[corners[i], corners[(i + 1) % corners.len()], mid],
                        group,
                        false,
                    );
                }
            } else {
                out.push_polygon(&corners, group, false);
            }
        }
        out.creases.extend(
            self.creases
                .iter()
                .copied()
                .filter(|e| !degree.contains_key(&e[0]) && !degree.contains_key(&e[1])),
        );
        out.retain_creases();
        out.preserve_model_uvs(self, &BTreeMap::new());
        out.compact_model_vertices();
        manifold(&out)?;
        // Every old boundary stays a boundary; closed input must remain closed.
        if closed {
            out.validate_closed()?;
        }
        Ok(out)
    }
}
