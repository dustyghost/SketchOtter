//! Open chains have a unique correspondence: reverse the second directed rim.
//! Reversing only one endpoint pair would break shared-edge winding.
use super::*;

impl Mesh {
    pub fn bridge_model_chains(
        &self,
        selected: &[[usize; 2]],
        segments: u32,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        if !(1..=16).contains(&segments) {
            return Err("Bridge segments must be between 1 and 16".into());
        }
        if selected.is_empty() || selected.iter().flatten().any(|&v| v >= self.vertices.len()) {
            return Err("Select two current open boundary chains".into());
        }
        manifold(self)?;
        crate::sculpt::fusion::validate_surface(self)?;
        if !self.uv.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear the inactive UV map before bridging".into());
        }
        let selected = symmetry.edges(selected);
        if selected.len() > 2048 {
            return Err("Bridge at most 2048 selected boundary edges at once".into());
        }
        let mut incidence = BTreeMap::<_, Vec<_>>::new();
        for p in self.polygons() {
            for (a, b) in p.edges() {
                incidence
                    .entry(edge(a, b))
                    .or_default()
                    .push((a, b, p.visible));
            }
        }
        let mut next = BTreeMap::new();
        let mut previous = BTreeSet::new();
        for e in &selected {
            let fs = incidence.get(e).ok_or("The edge selection is stale")?;
            if fs.len() != 1 || !fs[0].2 {
                return Err("Partial chains must contain only visible open boundary edges".into());
            }
            let (a, b, _) = fs[0];
            if next.insert(a, b).is_some() || !previous.insert(b) {
                return Err(
                    "Selected boundary chains branch; select separate simple chains".into(),
                );
            }
        }
        let starts: Vec<_> = next
            .keys()
            .filter(|v| !previous.contains(v))
            .copied()
            .collect();
        let mut chains = Vec::new();
        for start in starts {
            let mut chain = vec![start];
            while let Some(v) = next.remove(chain.last().unwrap()) {
                chain.push(v);
            }
            chains.push(chain);
        }
        if !next.is_empty() {
            return Err(
                "Closed loops use Bridge boundaries; select partial chains with two endpoints each"
                    .into(),
            );
        }
        if chains.len() < 2 {
            return Err("Select two separate open chains; they must not share an endpoint".into());
        }
        // Require one pair and its exact symmetry copies. Do not guess arbitrary
        // pairings between independently selected chains.
        let keys: Vec<BTreeSet<_>> = chains.iter().map(|c| c.iter().copied().collect()).collect();
        let mut candidates = BTreeSet::new();
        for other in 1..chains.len() {
            let mut pairs = BTreeSet::new();
            for map in &symmetry.vertices {
                let locate = |i: usize| {
                    let key: BTreeSet<_> = chains[i].iter().map(|&v| map[v]).collect();
                    keys.iter().position(|k| *k == key)
                };
                if let (Some(a), Some(b)) = (locate(0), locate(other)) {
                    if a != b {
                        pairs.insert(edge(a, b));
                    }
                }
            }
            let used: Vec<_> = pairs.iter().flatten().copied().collect();
            if used.len() == chains.len()
                && used.iter().copied().collect::<BTreeSet<_>>().len() == chains.len()
            {
                candidates.insert(pairs);
            }
        }
        if candidates.len() != 1 {
            return Err("Select exactly two chains and their unambiguous symmetry copies".into());
        }
        let pairs = candidates.pop_first().unwrap();
        let mut added_vertices = 0;
        let mut added_triangles = 0;
        for &[a, b] in &pairs {
            if chains[a].len() != chains[b].len() {
                return Err("Partial chains need equal edge counts; split edges or change the selection before bridging".into());
            }
            added_vertices += chains[a].len() * (segments as usize - 1);
            added_triangles += (chains[a].len() - 1) * segments as usize * 2;
        }
        if self.vertices.len() + added_vertices > max_vertices()
            || self.triangles.len() + added_triangles > max_triangles()
        {
            return Err("The bridge exceeds mesh capacity; reduce segments or chain length".into());
        }
        self.protect_model_vertices(
            &selected.iter().flatten().copied().collect::<Vec<_>>(),
            false,
        )?;
        let mut out = self.clone();
        out.polygroups.resize(self.triangles.len(), 0);
        out.hidden_faces.resize(self.triangles.len(), false);
        for [first, second] in pairs {
            let a = &chains[first];
            let b: Vec<_> = chains[second].iter().rev().copied().collect();
            let mut rows = vec![a.clone()];
            for step in 1..segments {
                let t = step as f32 / segments as f32;
                rows.push(
                    a.iter()
                        .zip(&b)
                        .map(|(&a, &b)| {
                            out.append_blend(
                                self,
                                &[(a, 1. - t), (b, t)],
                                self.vertices[a] * (1. - t) + self.vertices[b] * t,
                            )
                        })
                        .collect(),
                );
            }
            rows.push(b);
            for rows in rows.windows(2) {
                for i in 0..a.len() - 1 {
                    let q = [rows[0][i + 1], rows[0][i], rows[1][i], rows[1][i + 1]];
                    // A folded/bow-tie quad is invalid even if its two triangles
                    // independently have nonzero area.
                    let [p, q1, r, s] = q.map(|v| out.vertices[v]);
                    if (q1 - p).cross(r - p).dot((r - p).cross(s - p)) <= 0. {
                        return Err(
                            "The bridge twists or folds; adjust the chain endpoints or alignment"
                                .into(),
                        );
                    }
                    out.push_polygon(&q, 0, false);
                }
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
        if !self.shading_edges().is_empty() {
            out.set_shading_seams(self.shading_edges().to_vec());
        }
        manifold(&out)?;
        crate::sculpt::fusion::validate_surface(&out)?;
        crate::sculpt::fusion::validate_added_faces(&out, self.triangles.len(), &|| false)
            .map_err(|_| {
                "The bridge crosses itself or an existing surface; change the chains or alignment"
                    .to_string()
            })?;
        symmetry.validate_result(&out)?;
        Ok(out)
    }
}
