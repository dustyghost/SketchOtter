//! Bounded edge strips and vertex corner patches on closed surface fans.
use super::*;

/// Sample the quadratic corner profile used by rounded component bevels.
///
/// Keeping this curve independent of topology lets other modeling tools,
/// including generated panel borders, sample the same rounded profile.
pub(crate) fn rounded_bevel_profile(start: Vec3, corner: Vec3, end: Vec3, t: f32) -> Vec3 {
    let t = t.clamp(0., 1.);
    let u = 1. - t;
    start * (u * u) + corner * (2. * u * t) + end * (t * t)
}

impl Mesh {
    /// Replace selected faces, edges, or vertices with the perimeter edges of
    /// their connected, coplanar face islands.
    pub(crate) fn expand_flat_island_components(
        &self,
        selected_edges: &[[usize; 2]],
        selected_vertices: &[usize],
        selected_faces: &[usize],
    ) -> Result<Vec<[usize; 2]>, String> {
        let polygons = self.polygons();
        let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
        for (face, polygon) in polygons.iter().enumerate() {
            for (a, b) in polygon.edges() {
                incidence.entry(edge(a, b)).or_default().push(face);
            }
        }
        let normals: Vec<_> = polygons.iter().map(|p| normal(self, p)).collect();
        let mut face_neighbors = vec![Vec::new(); polygons.len()];
        for faces in incidence.values() {
            if faces.len() != 2 {
                continue;
            }
            let (a, b) = (faces[0], faces[1]);
            let direction = normals[a];
            let origin = self.vertices[polygons[a].vertices[0]];
            let scale = polygons[a]
                .edges()
                .chain(polygons[b].edges())
                .map(|(u, v)| (self.vertices[u] - self.vertices[v]).len())
                .fold(0_f32, f32::max);
            let coplanar = direction.dot(normals[b]) >= 0.9999
                && polygons[b]
                    .vertices
                    .iter()
                    .all(|&v| (self.vertices[v] - origin).dot(direction).abs() <= scale * 1e-4);
            if coplanar {
                face_neighbors[a].push(b);
                face_neighbors[b].push(a);
            }
        }
        let mut seeds: BTreeSet<_> = selected_faces
            .iter()
            .copied()
            .filter(|&face| face < polygons.len())
            .collect();
        seeds.extend(selected_edges.iter().flat_map(|e| {
            incidence
                .get(&edge(e[0], e[1]))
                .into_iter()
                .flatten()
                .copied()
        }));
        let selected_vertices: BTreeSet<_> = selected_vertices.iter().copied().collect();
        if !selected_vertices.is_empty() {
            for (face, polygon) in polygons.iter().enumerate() {
                if polygon
                    .vertices
                    .iter()
                    .any(|vertex| selected_vertices.contains(vertex))
                {
                    seeds.insert(face);
                }
            }
        }
        if seeds.is_empty() {
            return Err("Select a surface face, edge, or vertex on a flat island".into());
        }
        let mut visited = BTreeSet::new();
        let mut perimeter = BTreeSet::new();
        for seed in seeds {
            if !visited.insert(seed) {
                continue;
            }
            let mut island = BTreeSet::from([seed]);
            let mut stack = vec![seed];
            while let Some(face) = stack.pop() {
                for &neighbor in &face_neighbors[face] {
                    if visited.insert(neighbor) {
                        island.insert(neighbor);
                        stack.push(neighbor);
                    }
                }
            }
            for &face in &island {
                for (a, b) in polygons[face].edges() {
                    let e = edge(a, b);
                    if incidence[&e]
                        .iter()
                        .filter(|&&other| island.contains(&other))
                        .count()
                        == 1
                    {
                        perimeter.insert(e);
                    }
                }
            }
        }
        Ok(perimeter.into_iter().collect())
    }

    /// Follow selected edges along their unambiguous loops. A selected vertex
    /// loop becomes an edge selection; isolated selected vertices remain corner
    /// bevels.
    pub(crate) fn follow_bevel_edge_loops(
        &self,
        edges: &[[usize; 2]],
        vertices: &[usize],
    ) -> Result<(Vec<[usize; 2]>, Vec<usize>), String> {
        let mut followed: BTreeSet<_> = self.expand_edge_loop(edges)?.into_iter().collect();
        let selected_vertices: BTreeSet<_> = vertices.iter().copied().collect();
        let vertex_edges: Vec<_> = self
            .model_edges()
            .into_iter()
            .filter(|edge| {
                selected_vertices.contains(&edge[0]) && selected_vertices.contains(&edge[1])
            })
            .collect();
        if !vertex_edges.is_empty() {
            followed.extend(self.expand_edge_loop(&vertex_edges)?);
        }
        let loop_vertices: BTreeSet<_> = followed.iter().flatten().copied().collect();
        let remaining_vertices = selected_vertices
            .difference(&loop_vertices)
            .copied()
            .collect();
        Ok((followed.into_iter().collect(), remaining_vertices))
    }

    /// One segment is a flat chamfer; additional segments round the cut.
    pub(crate) fn bevel_components(
        &self,
        edges: &[[usize; 2]],
        vertices: &[usize],
        width: f32,
        segments: u32,
        symmetry: &ModelingSymmetry,
    ) -> Result<Mesh, String> {
        manifold(self)?;
        if !width.is_finite() || width <= 0. || !(1..=8).contains(&segments) {
            return Err("Use a positive width and between one and eight segments".into());
        }
        if !self.uv.is_empty() && !self.uv.current(self) {
            return Err("Restore or clear inactive UVs before changing this topology".into());
        }
        let polygons = self.polygons();
        let selected_edges: BTreeSet<_> = symmetry.edges(edges).into_iter().collect();
        let selected_vertices: BTreeSet<_> = vertices
            .iter()
            .flat_map(|&v| symmetry.vertex_copies(v))
            .collect();
        let affected: BTreeSet<_> = selected_edges
            .iter()
            .flatten()
            .copied()
            .chain(selected_vertices.iter().copied())
            .collect();
        if affected.is_empty() || affected.iter().any(|&v| v >= self.vertices.len()) {
            return Err("Select surface edges or vertices first".into());
        }
        self.protect_model_vertices(&affected.iter().copied().collect::<Vec<_>>(), false)?;
        let mut incidence = BTreeMap::<_, Vec<usize>>::new();
        for (f, p) in polygons.iter().enumerate() {
            for (a, b) in p.edges() {
                incidence.entry(edge(a, b)).or_default().push(f);
            }
        }
        if selected_edges.iter().any(|e| !incidence.contains_key(e))
            || affected
                .iter()
                .any(|v| !incidence.keys().any(|e| e.contains(v)))
            || incidence
                .iter()
                .any(|(e, fs)| e.iter().any(|v| affected.contains(v)) && fs.len() != 2)
            || self
                .loose_edges
                .iter()
                .any(|e| e.iter().any(|v| affected.contains(v)))
        {
            return Err("Choose edges or vertices surrounded by a closed surface; open borders and loose edges cannot be beveled".into());
        }
        // Each source corner either becomes a shared cut on an old edge, or an
        // intersection of two offset lines inside that face.
        let mut cuts = BTreeMap::<(usize, usize), f32>::new();
        let mut inner = BTreeMap::<(usize, usize), Vec3>::new();
        for (f, p) in polygons.iter().enumerate() {
            let n = normal(self, p);
            for (i, &v) in p.vertices.iter().enumerate() {
                if !affected.contains(&v) {
                    continue;
                }
                let a = p.vertices[(i + p.vertices.len() - 1) % p.vertices.len()];
                let b = p.vertices[(i + 1) % p.vertices.len()];
                let da = (self.vertices[a] - self.vertices[v]).norm();
                let db = (self.vertices[b] - self.vertices[v]).norm();
                let sin = db.cross(da).dot(n);
                if sin < 1e-5 {
                    return Err(
                        "Bevel needs convex face corners; split a flat or concave corner first"
                            .into(),
                    );
                }
                let sa = selected_edges.contains(&edge(v, a));
                let sb = selected_edges.contains(&edge(v, b));
                if selected_vertices.contains(&v) {
                    cuts.insert((v, a), width);
                    cuts.insert((v, b), width);
                } else if sa && sb {
                    let bisector = (da + db).norm();
                    let distance = width / bisector.cross(db).len();
                    if distance
                        >= 0.45
                            * (self.vertices[a] - self.vertices[v])
                                .len()
                                .min((self.vertices[b] - self.vertices[v]).len())
                    {
                        return Err("Reduce the width: neighboring cuts would overlap".into());
                    }
                    inner.insert((f, v), self.vertices[v] + bisector * distance);
                } else if sa || sb {
                    let neighbor = if sa { b } else { a };
                    let distance = width / sin;
                    cuts.entry((v, neighbor))
                        .and_modify(|d| *d = d.max(distance))
                        .or_insert(distance);
                }
            }
        }
        if cuts
            .iter()
            .any(|(&(v, a), &d)| d >= 0.45 * (self.vertices[a] - self.vertices[v]).len())
        {
            return Err("Reduce the width: neighboring cuts would overlap".into());
        }
        let corners = polygons
            .iter()
            .map(|p| p.vertices.iter().filter(|v| affected.contains(v)).count())
            .sum::<usize>();
        let extra = corners
            .saturating_mul((segments as usize + 1).pow(2))
            .saturating_mul(4);
        if self.vertices.len().saturating_add(extra) > max_vertices()
            || self.triangles.len().saturating_add(extra) > max_triangles()
        {
            return Err(
                "Bevel would exceed the mesh size limit; use fewer components or segments".into(),
            );
        }
        let mut out = self.empty_topology();
        out.uv = Default::default();
        let mut owners: BTreeMap<usize, usize> = affected.iter().map(|&v| (v, v)).collect();
        let mut cut_ids = BTreeMap::new();
        for (&(v, a), &d) in &cuts {
            let t = d / (self.vertices[a] - self.vertices[v]).len();
            let id = out.append_blend(
                self,
                &[(v, 1. - t), (a, t)],
                self.vertices[v] * (1. - t) + self.vertices[a] * t,
            );
            cut_ids.insert((v, a), id);
            owners.insert(id, v);
        }
        let mut inner_ids = BTreeMap::new();
        for (&key, &position) in &inner {
            let id = out.append_blend(self, &[(key.1, 1.)], position);
            inner_ids.insert(key, id);
            owners.insert(id, key.1);
        }
        let mut corners = BTreeMap::<(usize, usize), Vec<usize>>::new();
        for (f, p) in polygons.iter().enumerate() {
            for (i, &v) in p.vertices.iter().enumerate() {
                let a = p.vertices[(i + p.vertices.len() - 1) % p.vertices.len()];
                let b = p.vertices[(i + 1) % p.vertices.len()];
                let mut path = if let Some(&id) = inner_ids.get(&(f, v)) {
                    vec![id]
                } else if selected_edges.contains(&edge(v, a)) {
                    vec![*cut_ids.get(&(v, b)).ok_or("Unsupported bevel junction")?]
                } else if selected_edges.contains(&edge(v, b)) {
                    vec![*cut_ids.get(&(v, a)).ok_or("Unsupported bevel junction")?]
                } else {
                    vec![
                        cut_ids.get(&(v, a)).copied().unwrap_or(v),
                        cut_ids.get(&(v, b)).copied().unwrap_or(v),
                    ]
                };
                path.dedup();
                if selected_vertices.contains(&v) && segments > 1 {
                    path = rounded_path(
                        self,
                        &mut out,
                        &mut owners,
                        v,
                        path[0],
                        *path.last().unwrap(),
                        segments,
                    );
                }
                corners.insert((f, v), path);
            }
        }
        // Preserve untouched triangle records and UV corners exactly. Edited
        // surfaces receive unmapped UVs, without invalidating the rest of the mesh.
        let source_triangles = crate::sculpt::uv::polygon_triangles(self);
        let mut changed = Vec::new();
        let mut source_records = Vec::new();
        for (f, p) in polygons.iter().enumerate() {
            let path: Vec<_> = p
                .vertices
                .iter()
                .flat_map(|&v| corners[&(f, v)].iter().copied())
                .collect();
            if path == p.vertices {
                if path.len() == 4 {
                    out.quads.push(path.clone().try_into().unwrap());
                }
                for &t in &source_triangles[f] {
                    out.triangles.push(self.triangles[t]);
                    out.polygroups.push(p.group);
                    out.hidden_faces.push(p.hidden);
                    source_records.push(t);
                }
            } else {
                changed.push((path, p));
            }
        }
        let unchanged_count = out.triangles.len();
        for (path, p) in changed {
            self.component_polygon(&mut out, &path, p, false);
        }
        let mut edge_paths = BTreeMap::new();
        for &[a, b] in &selected_edges {
            let fs = &incidence[&edge(a, b)];
            let f = fs[0];
            let g = fs[1];
            let (a, b) = polygons[f]
                .edges()
                .find(|&(x, y)| edge(x, y) == edge(a, b))
                .unwrap();
            let ca = corners[&(f, a)][0];
            let cb = corners[&(f, b)][0];
            let da = corners[&(g, a)][0];
            let db = corners[&(g, b)][0];
            // Consecutive selected edges can share a cross-section. Reuse its
            // rounded vertices so their strips meet, instead of closing two
            // coincident arcs with a degenerate corner cap.
            let [aa, bb] = [(a, ca, da), (b, cb, db)].map(|(v, c, d)| {
                let mut path = edge_paths
                    .entry((v, c.min(d), c.max(d)))
                    .or_insert_with(|| {
                        rounded_path(self, &mut out, &mut owners, v, c.min(d), c.max(d), segments)
                    })
                    .clone();
                if c > d {
                    path.reverse();
                }
                path
            });
            for i in 0..segments as usize {
                out.push_polygon(
                    &[bb[i], aa[i], aa[i + 1], bb[i + 1]],
                    if polygons[f].group == polygons[g].group {
                        polygons[f].group
                    } else {
                        0
                    },
                    false,
                );
            }
        }
        // Remaining boundary cycles belong to the original corner they replace.
        let mut boundaries = BTreeMap::<_, Vec<(usize, usize)>>::new();
        for p in out.polygons() {
            for (a, b) in p.edges() {
                boundaries.entry(edge(a, b)).or_default().push((a, b));
            }
        }
        let mut caps = BTreeMap::<usize, BTreeMap<usize, usize>>::new();
        for uses in boundaries.values().filter(|u| u.len() == 1) {
            let (a, b) = uses[0];
            if let (Some(&va), Some(&vb)) = (owners.get(&a), owners.get(&b))
                && va == vb
                && caps.entry(va).or_default().insert(b, a).is_some()
            {
                return Err("This corner has overlapping fans; select a smaller region".into());
            }
        }
        for (v, mut pending) in caps {
            let start = *pending.keys().next().ok_or("Missing bevel corner")?;
            let mut ring = vec![start];
            let mut next = start;
            loop {
                next = pending
                    .remove(&next)
                    .ok_or("The bevel corner is not a closed loop")?;
                if next == start {
                    break;
                }
                if ring.contains(&next) {
                    return Err("The bevel corner crosses itself".into());
                }
                ring.push(next);
            }
            if !pending.is_empty() || ring.len() < 3 {
                return Err("This corner has disconnected surface fans".into());
            }
            let source = polygons.iter().find(|p| p.vertices.contains(&v)).unwrap();
            if selected_vertices.contains(&v) && segments > 1 {
                rounded_cap(self, &mut out, v, &ring, source, segments);
            } else {
                let patch = Polygon {
                    vertices: ring.clone(),
                    ..source.clone()
                };
                let n = normal(&out, &patch);
                let origin = out.vertices[ring[0]];
                let scale = ring
                    .iter()
                    .map(|&v| (out.vertices[v] - origin).len())
                    .fold(0_f32, f32::max);
                let planar = n.len() > 0.9
                    && ring
                        .iter()
                        .all(|&v| (out.vertices[v] - origin).dot(n).abs() <= scale * 1e-5);
                let concave = (0..ring.len()).any(|i| {
                    let a = out.vertices[ring[i]];
                    let b = out.vertices[ring[(i + 1) % ring.len()]];
                    let c = out.vertices[ring[(i + 2) % ring.len()]];
                    (b - a).cross(c - b).dot(n) < -scale * scale * 1e-7
                });
                if planar && concave {
                    // A rounded strip ending along a straight cube edge leaves a
                    // concave cap. Its vertex average lies outside the patch;
                    // a center fan would cross the new bevel surface.
                    let (triangles, _) = topology::manual::triangulate_boundary(&out, &ring)?;
                    for t in triangles {
                        out.push_polygon(&t, source.group, source.hidden);
                    }
                } else {
                    let face_valence = polygons
                        .iter()
                        .filter(|polygon| polygon.vertices.contains(&v))
                        .count();
                    if segments == 1
                        || !bevel_corner_grid(self, &mut out, &ring, source, face_valence, segments)
                    {
                        self.component_polygon(&mut out, &ring, source, true);
                    }
                }
            }
        }
        if self.uv.current(self) {
            out.uv = self.uv.clone();
            out.uv.corners = source_records.iter().map(|&t| self.uv.corners[t]).collect();
            out.uv.corners.resize(out.triangles.len(), [[0.; 2]; 3]);
            out.uv.removed = source_records.iter().map(|&t| !self.uv.mapped(t)).collect();
            out.uv.removed.resize(out.triangles.len(), true);
            if !self.uv.masks.is_empty() {
                out.uv.masks = source_records.iter().map(|&t| self.uv.masks[t]).collect();
                out.uv.masks.resize(out.triangles.len(), [0.; 3]);
            }
            out.uv.topology = out.triangles.clone();
        }
        let valid: BTreeSet<_> = out.polygon_edges().into_iter().collect();
        out.creases = self
            .creases
            .iter()
            .copied()
            .filter(|e| valid.contains(e))
            .collect();
        out.uv.seams.retain(|e| valid.contains(e));
        out.set_shading_seams(
            self.shading_edges()
                .iter()
                .copied()
                .filter(|e| valid.contains(e))
                .collect(),
        );
        manifold(&out)?;
        crate::sculpt::fusion::validate_added_faces(&out, unchanged_count, &|| false).map_err(
            |_| "Reduce the width or selection: the bevel intersects nearby faces".to_string(),
        )?;
        out.compact_model_vertices();
        symmetry.validate_result(&out)?;
        Ok(out)
    }
}

/// Fill a rounded edge junction with a small structured grid instead of
/// connecting every boundary sample to one high-valence center pole.
fn bevel_corner_grid(
    source: &Mesh,
    out: &mut Mesh,
    boundary: &[usize],
    polygon: &Polygon,
    face_valence: usize,
    segments: u32,
) -> bool {
    let segments = segments as usize;
    if face_valence < 3 || segments < 2 || boundary.len() != face_valence * segments {
        return false;
    }
    let mean = boundary.iter().fold(Vec3::default(), |position, &vertex| {
        position + out.vertices[vertex]
    }) * (1. / boundary.len() as f32);
    let radii: Vec<_> = boundary
        .iter()
        .map(|&vertex| (out.vertices[vertex] - mean).len())
        .collect();
    // Each incident source face contributes one equally segmented arc. Find
    // the phase where those arcs meet at their outer corner samples.
    let offset = (0..segments)
        .max_by(|&a, &b| {
            let score = |phase: usize| {
                (0..face_valence)
                    .map(|side| radii[(phase + side * segments) % boundary.len()])
                    .sum::<f32>()
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(0);
    let anchors: Vec<_> = (0..face_valence)
        .map(|side| (offset + side * segments) % boundary.len())
        .collect();
    if anchors.iter().any(|&anchor| {
        radii[anchor] <= radii[(anchor + boundary.len() - 1) % boundary.len()]
            || radii[anchor] <= radii[(anchor + 1) % boundary.len()]
    }) {
        return false;
    }

    let weights: Vec<_> = polygon
        .vertices
        .iter()
        .map(|&vertex| (vertex, 1. / polygon.vertices.len() as f32))
        .collect();
    let center = out.append_blend(source, &weights, mean);
    for &anchor in &anchors {
        let previous = (anchor + boundary.len() - 1) % boundary.len();
        let next = (anchor + 1) % boundary.len();
        out.push_polygon(
            &[boundary[previous], boundary[anchor], boundary[next]],
            polygon.group,
            polygon.hidden,
        );
    }
    let inner: Vec<_> = (0..boundary.len())
        .filter(|index| !anchors.contains(index))
        .collect();
    for i in 0..inner.len() {
        let a = inner[i];
        let b = inner[(i + 1) % inner.len()];
        out.push_polygon(
            &[boundary[a], boundary[b], center],
            polygon.group,
            polygon.hidden,
        );
    }
    true
}

fn rounded_path(
    source: &Mesh,
    out: &mut Mesh,
    owners: &mut BTreeMap<usize, usize>,
    v: usize,
    a: usize,
    b: usize,
    segments: u32,
) -> Vec<usize> {
    let mut path = vec![a];
    for i in 1..segments {
        let t = i as f32 / segments as f32;
        let position =
            rounded_bevel_profile(out.vertices[a], source.vertices[v], out.vertices[b], t);
        let id = out.append_blend(source, &[(v, 1.)], position);
        owners.insert(id, v);
        path.push(id);
    }
    path.push(b);
    path
}
fn rounded_cap(
    source: &Mesh,
    out: &mut Mesh,
    v: usize,
    boundary: &[usize],
    polygon: &Polygon,
    segments: u32,
) {
    let mean = boundary
        .iter()
        .fold(Vec3::default(), |p, &i| p + out.vertices[i])
        * (1. / boundary.len() as f32);
    let center = (source.vertices[v] + mean) * 0.5;
    let normal = (source.vertices[v] - mean).norm();
    let middle = out.append_blend(source, &[(v, 1.)], center);
    let mut previous = Vec::new();
    for step in 1..=segments {
        let t = step as f32 / segments as f32;
        let ring: Vec<_> = boundary
            .iter()
            .map(|&b| {
                if step == segments {
                    b
                } else {
                    let d = out.vertices[b] - center;
                    let height = normal * d.dot(normal);
                    out.append_blend(
                        source,
                        &[(v, 1.)],
                        center + (d - height) * t + height * (t * t),
                    )
                }
            })
            .collect();
        for i in 0..ring.len() {
            let j = (i + 1) % ring.len();
            if previous.is_empty() {
                out.push_polygon(&[middle, ring[i], ring[j]], polygon.group, false);
            } else {
                out.push_polygon(
                    &[previous[i], ring[i], ring[j], previous[j]],
                    polygon.group,
                    false,
                );
            }
        }
        previous = ring;
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    #[test]
    fn flat_island_edge_expands_to_connected_coplanar_patch_perimeter() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Grid);
        primitive.divisions = [4, 4, 1];
        let mesh = primitive.mesh().unwrap();
        let polygons = mesh.polygons();
        let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
        for (face, polygon) in polygons.iter().enumerate() {
            for (a, b) in polygon.edges() {
                incidence.entry(edge(a, b)).or_default().push(face);
            }
        }
        let seed = incidence
            .iter()
            .find(|(_, faces)| faces.len() == 2)
            .map(|(&e, _)| e)
            .unwrap();
        let expected: BTreeSet<_> = incidence
            .iter()
            .filter(|(_, faces)| faces.len() == 1)
            .map(|(&e, _)| e)
            .collect();
        assert_eq!(
            mesh.expand_flat_island_components(&[seed], &[], &[])
                .unwrap()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            expected
        );
    }

    #[test]
    fn flat_island_edge_stops_at_sharp_face_boundaries() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let polygons = mesh.polygons();
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let top_faces: BTreeSet<_> = polygons
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.vertices
                    .iter()
                    .all(|&v| (mesh.vertices[v].1 - top).abs() < 1e-6)
            })
            .map(|(i, _)| i)
            .collect();
        let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
        for (face, polygon) in polygons.iter().enumerate() {
            for (a, b) in polygon.edges() {
                incidence.entry(edge(a, b)).or_default().push(face);
            }
        }
        let seed = incidence
            .iter()
            .find(|(_, faces)| faces.len() == 2 && faces.iter().all(|f| top_faces.contains(f)))
            .map(|(&e, _)| e)
            .unwrap();
        let expected: BTreeSet<_> = incidence
            .iter()
            .filter(|(_, faces)| faces.iter().filter(|&&f| top_faces.contains(&f)).count() == 1)
            .map(|(&e, _)| e)
            .collect();
        let expanded: BTreeSet<_> = mesh
            .expand_flat_island_components(&[seed], &[], &[])
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(expanded, expected);

        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        mesh.bevel_components(
            &expanded.into_iter().collect::<Vec<_>>(),
            &[],
            0.05,
            1,
            &symmetry,
        )
        .unwrap()
        .validate_closed()
        .unwrap();
    }

    #[test]
    fn flat_island_edge_uses_same_perimeter_from_face_edge_or_interior_vertex() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let polygons = mesh.polygons();
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let top_faces: BTreeSet<_> = polygons
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.vertices
                    .iter()
                    .all(|&v| (mesh.vertices[v].1 - top).abs() < 1e-6)
            })
            .map(|(i, _)| i)
            .collect();
        let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
        let mut top_vertex_faces = BTreeMap::<usize, usize>::new();
        for (face, polygon) in polygons.iter().enumerate() {
            for (a, b) in polygon.edges() {
                incidence.entry(edge(a, b)).or_default().push(face);
            }
            if top_faces.contains(&face) {
                for &vertex in &polygon.vertices {
                    *top_vertex_faces.entry(vertex).or_default() += 1;
                }
            }
        }
        let face = *top_faces.iter().next().unwrap();
        let edge = incidence
            .iter()
            .find(|(_, faces)| faces.len() == 2 && faces.iter().all(|f| top_faces.contains(f)))
            .map(|(&e, _)| e)
            .unwrap();
        let vertex = top_vertex_faces
            .iter()
            .find(|(_, count)| **count == 4)
            .map(|(&vertex, _)| vertex)
            .unwrap();
        let expected: BTreeSet<_> = incidence
            .iter()
            .filter(|(_, faces)| faces.iter().filter(|&&f| top_faces.contains(&f)).count() == 1)
            .map(|(&e, _)| e)
            .collect();

        let from_edge = mesh
            .expand_flat_island_components(&[edge], &[], &[])
            .unwrap();
        let from_face = mesh
            .expand_flat_island_components(&[], &[], &[face])
            .unwrap();
        let from_vertex = mesh
            .expand_flat_island_components(&[], &[vertex], &[])
            .unwrap();
        let as_set = |edges: Vec<[usize; 2]>| edges.into_iter().collect::<BTreeSet<_>>();
        assert_eq!(as_set(from_edge.clone()), expected);
        assert_eq!(as_set(from_face.clone()), expected);
        assert_eq!(as_set(from_vertex.clone()), expected);

        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let bevel = |edges: &[[usize; 2]]| {
            mesh.bevel_components(edges, &[], 0.05, 1, &symmetry)
                .unwrap()
        };
        let edge_result = bevel(&from_edge);
        let face_result = bevel(&from_face);
        let vertex_result = bevel(&from_vertex);
        assert_eq!(edge_result.vertices, face_result.vertices);
        assert_eq!(edge_result.vertices, vertex_result.vertices);
        assert_eq!(edge_result.triangles, face_result.triangles);
        assert_eq!(edge_result.triangles, vertex_result.triangles);
        edge_result.validate_closed().unwrap();
    }

    #[test]
    fn follow_edge_loop_connects_vertex_loops_and_keeps_isolated_vertices() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let right = mesh
            .vertices
            .iter()
            .map(|v| v.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let front = mesh
            .vertices
            .iter()
            .map(|v| v.2)
            .fold(f32::NEG_INFINITY, f32::max);
        let loop_edges: Vec<_> = mesh
            .polygon_edges()
            .into_iter()
            .filter(|e| {
                e.iter().all(|&v| {
                    let p = mesh.vertices[v];
                    (p.1 - top).abs() < 1e-6
                        && ((p.0.abs() - right).abs() < 1e-6 || (p.2.abs() - front).abs() < 1e-6)
                })
            })
            .collect();
        assert_eq!(loop_edges.len(), 12);
        let loop_vertices: Vec<_> = loop_edges.iter().flatten().copied().collect();
        let (followed, remaining) = mesh.follow_bevel_edge_loops(&[], &loop_vertices).unwrap();
        assert_eq!(
            followed.into_iter().collect::<BTreeSet<_>>(),
            loop_edges.into_iter().collect()
        );
        assert!(remaining.is_empty());

        let isolated = mesh.vertices.len() - 1;
        let (followed, remaining) = mesh.follow_bevel_edge_loops(&[], &[isolated]).unwrap();
        assert!(followed.is_empty());
        assert_eq!(remaining, vec![isolated]);
    }

    #[test]
    fn follow_edge_loop_expands_selected_edges_and_preserves_manifold_bevels() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let right = mesh
            .vertices
            .iter()
            .map(|v| v.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let seed = mesh
            .polygon_edges()
            .into_iter()
            .find(|e| {
                e.iter().all(|&v| {
                    let p = mesh.vertices[v];
                    (p.1 - top).abs() < 1e-6 && (p.0 - right).abs() < 1e-6
                })
            })
            .unwrap();
        let (followed, points) = mesh.follow_bevel_edge_loops(&[seed], &[]).unwrap();
        assert!(followed.len() > 1);
        assert!(points.is_empty());
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        mesh.bevel_components(&followed, &[], 0.05, 3, &symmetry)
            .unwrap()
            .validate_closed()
            .unwrap();
    }

    #[test]
    fn component_bevel_follows_subdivided_edge_chains_and_loops() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.1)
            .fold(f32::NEG_INFINITY, f32::max);
        let right = mesh
            .vertices
            .iter()
            .map(|v| v.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let front = mesh
            .vertices
            .iter()
            .map(|v| v.2)
            .fold(f32::NEG_INFINITY, f32::max);
        let edges = mesh.polygon_edges();
        let chain: Vec<_> = edges
            .iter()
            .copied()
            .filter(|e| {
                e.iter().all(|&v| {
                    let p = mesh.vertices[v];
                    (p.1 - top).abs() < 1e-6 && (p.0 - right).abs() < 1e-6
                })
            })
            .collect();
        let ring: Vec<_> = edges
            .iter()
            .copied()
            .filter(|e| {
                e.iter().all(|&v| {
                    let p = mesh.vertices[v];
                    (p.1 - top).abs() < 1e-6
                        && ((p.0.abs() - right).abs() < 1e-6 || (p.2.abs() - front).abs() < 1e-6)
                })
            })
            .collect();
        assert_eq!(chain.len(), 3);
        assert_eq!(ring.len(), 12);
        for (name, selected) in [("chain", chain), ("loop", ring)] {
            for segments in [1, 2, 3, 8] {
                let out = mesh
                    .bevel_components(&selected, &[], 0.05, segments, &symmetry)
                    .unwrap_or_else(|err| panic!("{name}, segments {segments}: {err}"));
                out.validate_closed().unwrap();
            }
        }
    }
    #[test]
    fn review_subdivided_cube_bevels_every_sharp_edge() {
        let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        p.divisions = [3; 3];
        let mesh = p.mesh().unwrap();
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let polygons = mesh.polygons();
        for e in mesh.polygon_edges() {
            let faces: Vec<_> = polygons
                .iter()
                .filter(|p| p.edges().any(|(a, b)| edge(a, b) == e))
                .collect();
            if normal(&mesh, faces[0]).dot(normal(&mesh, faces[1])) > 0.9 {
                continue;
            }
            for segments in [2, 8] {
                let out = mesh
                    .bevel_components(&[e], &[], 0.05, segments, &symmetry)
                    .unwrap_or_else(|err| panic!("edge {e:?}, segments {segments}: {err}"));
                out.validate_closed().unwrap();
            }
        }
    }
    fn cube() -> Mesh {
        let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        p.divisions = [1; 3];
        p.mesh().unwrap()
    }
    #[test]
    fn component_bevel_chamfers_and_rounds_edges_vertices_and_junctions() {
        let mesh = cube();
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let edges = mesh.polygon_edges();
        for selection in [
            vec![edges[0]],
            edges.iter().copied().filter(|e| e.contains(&0)).collect(),
            edges.clone(),
        ] {
            for segments in [1, 3, 8] {
                let out = mesh
                    .bevel_components(&selection, &[], 0.1, segments, &symmetry)
                    .unwrap();
                manifold(&out).unwrap();
                let mut counts = BTreeMap::new();
                for p in out.polygons() {
                    for (a, b) in p.edges() {
                        *counts.entry(edge(a, b)).or_insert(0) += 1;
                    }
                }
                assert!(counts.values().all(|&n| n == 2));
                assert!(out.vertices.len() > mesh.vertices.len());
            }
        }
        for selected in [vec![0], vec![0, 1], (0..mesh.vertices.len()).collect()] {
            for segments in [1, 3, 8] {
                let out = mesh
                    .bevel_components(&[], &selected, 0.1, segments, &symmetry)
                    .unwrap();
                manifold(&out).unwrap();
                assert!(!out.vertices.contains(&mesh.vertices[0]));
            }
        }
    }

    #[test]
    fn flat_island_bevel_corner_cap_avoids_a_nine_way_pole() {
        let mut primitive = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        let mesh = primitive.mesh().unwrap();
        let faces: Vec<_> = (0..mesh.polygons().len()).collect();
        let edges = mesh
            .expand_flat_island_components(&[], &[], &faces)
            .unwrap();
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let width = 0.178_076_6;
        let out = mesh
            .bevel_components(&edges, &[], width, 3, &symmetry)
            .unwrap();
        out.validate_closed().unwrap();

        let mut neighbors = vec![BTreeSet::new(); out.vertices.len()];
        for [a, b] in out.polygon_edges() {
            neighbors[a].insert(b);
            neighbors[b].insert(a);
        }
        let source_corner = Vec3(-1., -1., -1.);
        let local_max_valence = out
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, position)| (**position - source_corner).len() < width * 2.)
            .map(|(vertex, _)| neighbors[vertex].len())
            .max()
            .unwrap();
        assert!(
            local_max_valence <= 6,
            "corner cap should continue its three boundary loops instead of converging to a nine-way pole; local maximum valence was {local_max_valence}"
        );
    }

    #[test]
    fn component_bevel_mirrors_corner_and_edge_topology() {
        let mesh = cube();
        let settings = crate::sculpt::Symmetry {
            enabled: true,
            mirror: [true, false, false],
            ..Default::default()
        };
        let symmetry = ModelingSymmetry::new(&mesh, &settings).unwrap();
        for segments in [1, 3] {
            let edges = mesh.polygon_edges();
            for (edges, vertices) in [(&edges[..1], &[][..]), (&[][..], &[0][..])] {
                let out = mesh
                    .bevel_components(edges, vertices, 0.1, segments, &symmetry)
                    .unwrap();
                ModelingSymmetry::new(&out, &settings).unwrap();
                out.validate_closed().unwrap();
            }
        }
    }
    #[test]
    fn component_bevel_preserves_untouched_uvs_groups_and_protection() {
        let mut mesh = cube();
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = vec![[[0., 0.], [1., 0.], [1., 1.]]; mesh.triangles.len()];
        let sym = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let out = mesh.bevel_components(&[], &[0], 0.1, 3, &sym).unwrap();
        assert!(out.uv.current(&out));
        assert!(out.uv.removed.iter().any(|&x| x));
        assert!(out.uv.removed.iter().any(|&x| !x));
        mesh.mask[0] = 1.;
        assert!(mesh.check_model_protection(&out, true).is_err());
        assert!(mesh.bevel_components(&[], &[0], 10., 3, &sym).is_err());
        let mut open = mesh.clone();
        open.triangles.clear();
        open.quads.clear();
        open.uv = Default::default();
        assert!(open.bevel_components(&[], &[0], 0.1, 3, &sym).is_err());
    }

    #[test]
    fn rounded_bevel_profile_uses_clamped_quadratic_samples() {
        let a = Vec3(0., 0., 0.);
        let corner = Vec3(0., 1., 0.);
        let b = Vec3(1., 1., 0.);
        assert_eq!(rounded_bevel_profile(a, corner, b, 0.), a);
        assert_eq!(rounded_bevel_profile(a, corner, b, 1.), b);
        assert_eq!(
            rounded_bevel_profile(a, corner, b, 0.5),
            Vec3(0.25, 0.75, 0.)
        );
        assert_eq!(rounded_bevel_profile(a, corner, b, -1.), a);
        assert_eq!(rounded_bevel_profile(a, corner, b, 2.), b);
    }

    #[test]
    fn interior_edge_bevel_uses_neutral_group_at_mixed_group_junctions() {
        let mut mesh = cube();
        let polygons = mesh.polygons();
        let selected = polygons[0].edges().next().unwrap();
        let adjacent: Vec<_> = polygons
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.edges()
                    .any(|(a, b)| edge(a, b) == edge(selected.0, selected.1))
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(adjacent.len(), 2);

        let mut groups = vec![1; mesh.triangles.len()];
        for (face, polygon) in polygons.iter().enumerate() {
            let group = if face == adjacent[0] {
                1
            } else if face == adjacent[1] {
                2
            } else {
                1
            };
            for (i, triangle) in mesh.triangles.iter().enumerate() {
                if triangle.iter().all(|v| polygon.vertices.contains(v)) {
                    groups[i] = group;
                }
            }
        }
        mesh.polygroups = groups;
        mesh.vertex_groups = (1..=2)
            .map(|id| crate::sculpt::VertexGroup {
                purpose: crate::sculpt::GroupPurpose::Selection,
                visible: true,
                name: format!("group {id}"),
                weights: vec![0.; mesh.vertices.len()],
            })
            .collect();
        let untouched_face = polygons
            .iter()
            .find(|p| {
                !p.vertices
                    .iter()
                    .any(|v| [selected.0, selected.1].contains(v))
            })
            .unwrap();
        let other_crease = untouched_face
            .edges()
            .map(|(a, b)| edge(a, b))
            .next()
            .unwrap();
        mesh.creases = vec![edge(selected.0, selected.1), other_crease];
        mesh.creases.sort_unstable();

        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let out = mesh
            .bevel_components(&[edge(selected.0, selected.1)], &[], 0.1, 3, &symmetry)
            .unwrap();
        assert!(
            out.polygroups.contains(&0),
            "mixed-group bevel strip is neutral"
        );
        assert!(
            out.polygroups.contains(&1) && out.polygroups.contains(&2),
            "edited source faces retain their original group labels"
        );
        let crease_connects = |mesh: &Mesh, a: Vec3, b: Vec3| {
            mesh.creases.iter().any(|&[u, v]| {
                (mesh.vertices[u] == a && mesh.vertices[v] == b)
                    || (mesh.vertices[u] == b && mesh.vertices[v] == a)
            })
        };
        let other_points = other_crease.map(|v| mesh.vertices[v]);
        let selected_points = [mesh.vertices[selected.0], mesh.vertices[selected.1]];
        assert!(crease_connects(&out, other_points[0], other_points[1]));
        assert!(!crease_connects(
            &out,
            selected_points[0],
            selected_points[1]
        ));
    }

    #[test]
    fn vertex_bevel_handles_an_irregular_valence_fan() {
        let mut mesh = Mesh {
            vertices: vec![Vec3(0., 1., 0.), Vec3(0., -1., 0.)],
            ..Default::default()
        };
        let sides = 5;
        for i in 0..sides {
            let angle = std::f32::consts::TAU * i as f32 / sides as f32;
            mesh.vertices.push(Vec3(angle.cos(), 0., angle.sin()));
        }
        for i in 0..sides {
            let a = 2 + i;
            let b = 2 + (i + 1) % sides;
            mesh.triangles.extend([[0, b, a], [1, a, b]]);
        }
        mesh.mask.resize(mesh.vertices.len(), 0.);
        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let out = mesh.bevel_components(&[], &[0], 0.1, 3, &symmetry).unwrap();
        out.validate_closed().unwrap();
    }

    #[test]
    fn vertex_bevel_rejects_a_concave_face_corner() {
        let mut mesh = cube();
        let polygon = mesh
            .polygons()
            .into_iter()
            .find(|p| p.vertices.len() == 4)
            .unwrap();
        let v = polygon.vertices[0];
        let a = polygon.vertices[polygon.vertices.len() - 1];
        let b = polygon.vertices[1];
        let n = normal(&mesh, &polygon);
        let midpoint = (mesh.vertices[a] + mesh.vertices[b]) * 0.5;
        let direction = (mesh.vertices[b] - mesh.vertices[a]).cross(n).norm();
        let offset = (mesh.vertices[b] - mesh.vertices[a]).len() * 0.1;
        let concave = [1., -1.]
            .into_iter()
            .map(|sign| midpoint + direction * (offset * sign))
            .find(|candidate| {
                let da = mesh.vertices[a] - *candidate;
                let db = mesh.vertices[b] - *candidate;
                db.cross(da).dot(n) < -1e-5
            })
            .unwrap();
        mesh.vertices[v] = concave;

        let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
        let error = mesh
            .bevel_components(&[], &[v], 0.1, 3, &symmetry)
            .unwrap_err();
        assert!(error.contains("convex face corners"), "{error}");
    }
}
