use super::features::Features;
use super::quadratic::{Problem, Row};
use crate::{
    Result, Vec3,
    field::Field,
    topology::{self, Dsu},
};
use glam::DVec2;
use std::collections::{BTreeMap, VecDeque};

pub(super) fn rotate(v: DVec2, k: i32) -> DVec2 {
    match k.rem_euclid(4) {
        0 => v,
        1 => DVec2::new(-v.y, v.x),
        2 => -v,
        _ => DVec2::new(v.y, -v.x),
    }
}

pub(super) struct Edge {
    pub ends: [usize; 2],
    /// (face, local outgoing edge index), with opposite directions.
    pub sides: [(usize, usize); 2],
    pub rotation: i32,
    pub translation: Option<usize>,
}

pub(super) struct Atlas {
    pub uv_variables: usize,
    pub corners: Vec<[usize; 3]>,
    /// One (face, local corner) for each singularity or sharp-feature corner.
    pub singularities: Vec<(usize, usize)>,
    pub features: Features,
    pub edges: Vec<Edge>,
    pub reference: Vec<[DVec2; 3]>,
    pub problem: Problem,
}

impl Atlas {
    pub fn new(
        field: &Field,
        features: Features,
        side_directions: Option<&[[Vec3; 3]]>,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self> {
        let surface = &field.surface;
        let mut frames = Vec::with_capacity(surface.triangles.len());
        let mut normals = Vec::with_capacity(surface.triangles.len());
        for (i, &[a, b, c]) in surface.triangles.iter().enumerate() {
            if i % 1024 == 0 {
                crate::check(cancel)?;
            }
            let n = (surface.positions[b] - surface.positions[a])
                .cross(surface.positions[c] - surface.positions[a])
                .normalize();
            let directions =
                side_directions.map_or_else(|| [a, b, c].map(|v| field.directions[v]), |d| d[i]);
            let mut q = directions[0] - n * directions[0].dot(n);
            if q.length_squared() < 1e-12 {
                q = surface.positions[b] - surface.positions[a];
            }
            q = q.normalize();
            let mut sum = Vec3::ZERO;
            for direction in directions {
                let u = (direction - n * direction.dot(n)).normalize_or_zero();
                let w = n.cross(u);
                let best = [u, w, -u, -w]
                    .into_iter()
                    .max_by(|a, b| a.dot(q).total_cmp(&b.dot(q)))
                    .unwrap();
                sum += best;
            }
            let u = sum.normalize_or_zero();
            if u.length_squared() < 0.5 {
                return Err("Cannot form an integer-grid face frame".into());
            }
            let mut u = u;
            let sharp: Vec<_> = [
                topology::edge(a, b),
                topology::edge(b, c),
                topology::edge(c, a),
            ]
            .into_iter()
            .filter(|e| features.edges.contains(e))
            .collect();
            if sharp.len() > 1 {
                return Err("Sharp feature domain needs interior support".into());
            }
            if let Some(&[a, b]) = sharp.first() {
                let t = (surface.positions[b] - surface.positions[a]).normalize();
                u = [t, n.cross(t), -t, -n.cross(t)]
                    .into_iter()
                    .max_by(|a, b| a.dot(u).total_cmp(&b.dot(u)))
                    .unwrap();
            }
            frames.push([u, n.cross(u)]);
            normals.push(n);
        }
        let mut incidence = BTreeMap::<[usize; 2], Vec<(usize, usize)>>::new();
        for (f, t) in surface.triangles.iter().enumerate() {
            if f % 1024 == 0 {
                crate::check(cancel)?;
            }
            for k in 0..3 {
                incidence
                    .entry(topology::edge(t[k], t[(k + 1) % 3]))
                    .or_default()
                    .push((f, k));
            }
        }
        let mut edges = Vec::with_capacity(incidence.len());
        let mut adjacency = vec![Vec::new(); frames.len()];
        for (ends, sides) in incidence {
            let [(a, ka), (b, kb)] = sides.as_slice() else {
                return Err(
                    "Experimental integer-grid mapping currently requires closed surfaces".into(),
                );
            };
            let direction = surface.positions[ends[1]] - surface.positions[ends[0]];
            let angle = |f: usize| {
                direction
                    .dot(frames[f][1])
                    .atan2(direction.dot(frames[f][0]))
            };
            let rotation = ((angle(*b) - angle(*a)) / std::f64::consts::FRAC_PI_2).round() as i32;
            let id = edges.len();
            edges.push(Edge {
                ends,
                sides: [(*a, *ka), (*b, *kb)],
                rotation: rotation.rem_euclid(4),
                translation: None,
            });
            adjacency[*a].push((id, *b));
            adjacency[*b].push((id, *a));
        }
        // A dual spanning forest first opens each component into a disk.
        // Combing removes quarter-turn jumps across the tree. Pruning the cut
        // graph below keeps its handles and all singular endpoints.
        let mut comb = vec![None; frames.len()];
        let mut cut = vec![true; edges.len()];
        let mut anchors = Vec::new();
        let feature_vertices: std::collections::BTreeSet<_> =
            features.edges.iter().flatten().copied().collect();
        for root in 0..frames.len() {
            if comb[root].is_some() {
                continue;
            }
            let mut anchor = (root, 0);
            let mut anchor_rank = 0;
            comb[root] = Some(0i32);
            let mut queue = VecDeque::from([root]);
            while let Some(f) = queue.pop_front() {
                crate::check(cancel)?;
                // Fix the translation gauge on a feature when one exists.
                // Anchoring a nearby interior vertex to (0,0) can otherwise
                // force its boundary onto that same line during rounding.
                for (k, &v) in surface.triangles[f].iter().enumerate() {
                    let rank = if features.corners.contains(&v) {
                        2
                    } else {
                        usize::from(feature_vertices.contains(&v))
                    };
                    if rank > anchor_rank {
                        anchor = (f, k);
                        anchor_rank = rank;
                    }
                }
                for &(e, g) in &adjacency[f] {
                    if comb[g].is_none() {
                        let k = if edges[e].sides[0].0 == f {
                            edges[e].rotation
                        } else {
                            -edges[e].rotation
                        };
                        comb[g] = Some((comb[f].unwrap() - k).rem_euclid(4));
                        cut[e] = false;
                        queue.push_back(g);
                    }
                }
            }
            anchors.push(anchor);
        }
        for (f, frame) in frames.iter_mut().enumerate() {
            let [u, v] = *frame;
            let x = rotate(DVec2::X, comb[f].unwrap());
            *frame = [u * x.x - v * x.y, u * x.y + v * x.x];
        }
        let mut holonomy = vec![0i32; surface.positions.len()];
        let mut vertex_edges = vec![Vec::new(); surface.positions.len()];
        for (id, e) in edges.iter_mut().enumerate() {
            let [(f, k), (g, _)] = e.sides;
            e.rotation = (e.rotation + comb[g].unwrap() - comb[f].unwrap()).rem_euclid(4);
            let [a, b] = [surface.triangles[f][k], surface.triangles[f][(k + 1) % 3]];
            holonomy[a] += e.rotation;
            holonomy[b] -= e.rotation;
            for v in e.ends {
                vertex_edges[v].push(id);
            }
        }
        let singular: Vec<_> = holonomy.iter().map(|k| k.rem_euclid(4) != 0).collect();
        let mut degree: Vec<_> = vertex_edges
            .iter()
            .map(|es| es.iter().filter(|&&e| cut[e]).count())
            .collect();
        let mut queue: VecDeque<_> = (0..degree.len())
            .filter(|&v| degree[v] == 1 && !singular[v])
            .collect();
        while let Some(v) = queue.pop_front() {
            if degree[v] != 1 || singular[v] {
                continue;
            }
            let e = *vertex_edges[v].iter().find(|&&e| cut[e]).unwrap();
            // A nonzero transition must remain a seam even if numerical field
            // ambiguity created a full-turn singularity.
            if edges[e].rotation != 0 {
                continue;
            }
            cut[e] = false;
            for u in edges[e].ends {
                degree[u] -= 1;
                if degree[u] == 1 && !singular[u] {
                    queue.push_back(u);
                }
            }
        }
        let mut dsu = Dsu::new(3 * frames.len());
        for (id, e) in edges.iter().enumerate() {
            if cut[id] {
                continue;
            }
            let [(f, k), (g, l)] = e.sides;
            dsu.join(3 * f + k, 3 * g + (l + 1) % 3);
            dsu.join(3 * f + (k + 1) % 3, 3 * g + l);
        }
        let mut compact = BTreeMap::new();
        let mut corners = Vec::with_capacity(frames.len());
        for f in 0..frames.len() {
            corners.push(std::array::from_fn(|k| {
                let root = dsu.find(3 * f + k);
                let next = compact.len();
                *compact.entry(root).or_insert(next)
            }));
        }
        let uv_variables = 2 * compact.len();
        let mut variables = uv_variables;
        for (i, e) in edges.iter_mut().enumerate() {
            if cut[i] {
                e.translation = Some(variables);
                variables += 2;
            }
        }
        let mut problem = Problem::new(variables);
        for e in &edges {
            let Some(t) = e.translation else {
                continue;
            };
            problem.integers.extend([t, t + 1]);
            let [(f, k), (g, l)] = e.sides;
            for (a, b) in [
                (corners[f][k], corners[g][(l + 1) % 3]),
                (corners[f][(k + 1) % 3], corners[g][l]),
            ] {
                for axis in 0..2 {
                    let mut row = vec![(2 * b + axis, 1.), (t + axis, -1.)];
                    for source_axis in 0..2 {
                        let r = rotate(
                            if source_axis == 0 { DVec2::X } else { DVec2::Y },
                            e.rotation,
                        );
                        if r[axis] != 0. {
                            row.push((2 * a + source_axis, -r[axis]));
                        }
                    }
                    problem.equalities.push((row, 0.));
                }
            }
        }
        let mut marked = vec![false; singular.len()];
        let mut singularities = Vec::new();
        for (f, t) in surface.triangles.iter().enumerate() {
            for k in 0..3 {
                if (singular[t[k]] || features.corners.contains(&t[k])) && !marked[t[k]] {
                    marked[t[k]] = true;
                    singularities.push((f, k));
                    problem
                        .integers
                        .extend([2 * corners[f][k], 2 * corners[f][k] + 1]);
                }
            }
        }
        // MIQ §5.2: the field is aligned above; now place each feature edge
        // on a constant integer u or v line on both sides of every seam.
        for e in &edges {
            if !features.edges.contains(&e.ends) {
                continue;
            }
            let tangent = surface.positions[e.ends[1]] - surface.positions[e.ends[0]];
            for &(f, k) in &e.sides {
                let axis = if tangent.dot(frames[f][0]).abs() >= tangent.dot(frames[f][1]).abs() {
                    1
                } else {
                    0
                };
                let a = 2 * corners[f][k] + axis;
                let b = 2 * corners[f][(k + 1) % 3] + axis;
                problem.equalities.push((vec![(a, 1.), (b, -1.)], 0.));
                problem.integers.extend([a, b]);
            }
        }
        for (f, k) in anchors {
            for axis in 0..2 {
                problem
                    .equalities
                    .push((vec![(2 * corners[f][k] + axis, 1.)], 0.));
            }
        }
        let mut reference = Vec::with_capacity(frames.len());
        for (f, &t) in surface.triangles.iter().enumerate() {
            if f % 1024 == 0 {
                crate::check(cancel)?;
            }
            let p = t.map(|v| surface.positions[v]);
            let area2 = (p[1] - p[0]).cross(p[2] - p[0]).length();
            let gradient: [Vec3; 3] =
                std::array::from_fn(|k| normals[f].cross(p[(k + 2) % 3] - p[(k + 1) % 3]) / area2);
            let scale = (area2 * 0.5).sqrt();
            let h = t.iter().map(|&v| field.spacing[v]).sum::<f64>() / 3.;
            for axis in 0..2 {
                for (direction, frame) in frames[f].iter().enumerate() {
                    let row = (0..3)
                        .map(|k| (2 * corners[f][k] + axis, gradient[k].dot(*frame) * scale))
                        .collect();
                    problem
                        .energy
                        .push((row, if axis == direction { scale / h } else { 0. }));
                }
            }
            reference.push(p.map(|point| {
                DVec2::new(
                    (point - p[0]).dot(frames[f][0]),
                    (point - p[0]).dot(frames[f][1]),
                ) / h
            }));
        }
        Ok(Self {
            uv_variables,
            corners,
            singularities,
            features,
            edges,
            reference,
            problem,
        })
    }

    pub fn coordinates(&self, x: &[f64], f: usize) -> [DVec2; 3] {
        self.corners[f].map(|v| DVec2::new(x[2 * v], x[2 * v + 1]))
    }

    /// Integer seam translations and quarter turns propagate integrality to
    /// equivalent chart coordinates. Canonicalize only those proven integer;
    /// an unrelated coordinate close to a grid line must retain its fraction.
    pub fn canonicalize_integers(&self, x: &mut [f64]) -> Result<()> {
        let mut equivalent = Dsu::new(self.uv_variables);
        for e in &self.edges {
            let [(f, k), (g, l)] = e.sides;
            for (a, b) in [
                (self.corners[f][k], self.corners[g][(l + 1) % 3]),
                (self.corners[f][(k + 1) % 3], self.corners[g][l]),
            ] {
                for axis in 0..2 {
                    equivalent.join(2 * a + axis, 2 * b + (axis + e.rotation as usize) % 2);
                }
            }
        }
        let mut integral = vec![false; self.uv_variables];
        for &i in &self.problem.integers {
            if i < self.uv_variables {
                integral[equivalent.find(i)] = true;
            }
        }
        for i in 0..self.uv_variables {
            if integral[equivalent.find(i)] {
                if (x[i] - x[i].round()).abs() > 1e-6 {
                    return Err("Integer-grid equivalent coordinate is not integral".into());
                }
                x[i] = x[i].round();
            }
        }
        for &i in &self.problem.integers {
            x[i] = x[i].round();
        }
        Ok(())
    }

    /// Quarter-turn holonomy alone cannot distinguish a regular vertex from
    /// a full-turn pole (for example, valence eight). Once every map triangle
    /// is positive, its cone angle identifies these additional singularities.
    pub fn complete_singularities(
        &mut self,
        source: &crate::Surface,
        x: &[f64],
        cancel: &dyn Fn() -> bool,
    ) -> Result<()> {
        let mut angles = vec![0.; source.positions.len()];
        let mut representatives = vec![(0, 0); source.positions.len()];
        for (f, t) in source.triangles.iter().enumerate() {
            if f % 1024 == 0 {
                crate::check(cancel)?;
            }
            let p = self.coordinates(x, f);
            for k in 0..3 {
                let a = p[(k + 1) % 3] - p[k];
                let b = p[(k + 2) % 3] - p[k];
                angles[t[k]] += a.perp_dot(b).atan2(a.dot(b));
                representatives[t[k]] = (f, k);
            }
        }
        let mut marked = vec![false; angles.len()];
        for &(f, k) in &self.singularities {
            marked[source.triangles[f][k]] = true;
        }
        for (v, angle) in angles.into_iter().enumerate() {
            if !marked[v] && (angle / std::f64::consts::FRAC_PI_2).round() != 4. {
                let (f, k) = representatives[v];
                self.singularities.push((f, k));
                self.problem
                    .integers
                    .extend([2 * self.corners[f][k], 2 * self.corners[f][k] + 1]);
            }
        }
        Ok(())
    }

    pub fn orientation_constraints(&self, f: usize) -> crate::Result<Vec<(Row, f64)>> {
        let reference = self.reference[f];
        let center = reference.iter().sum::<DVec2>() / 3.;
        let direction = reference.map(|p| (p - center).normalize());
        let rays: [DVec2; 3] =
            std::array::from_fn(|k| (direction[k] + direction[(k + 1) % 3]).normalize());
        let mut rows = Vec::new();
        for k in 0..3 {
            for n in [rays[(k + 2) % 3].perp(), -rays[k].perp()] {
                let scale = (reference[k] - center).dot(n);
                if !scale.is_finite() || scale <= 0. {
                    return Err("Integer-grid orientation reference is degenerate".into());
                }
                let mut row = Vec::new();
                for j in 0..3 {
                    for axis in 0..2 {
                        row.push((
                            2 * self.corners[f][j] + axis,
                            n[axis] * (if j == k { 2. / 3. } else { -1. / 3. }) / scale,
                        ));
                    }
                }
                // Normalize the same half-space so tiny cap triangles do not
                // put its positive margin below absolute solver tolerances.
                rows.push((row, 0.001));
            }
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certified_integer_roundoff_propagates_across_rotated_seams_only() {
        let mut atlas = Atlas {
            uv_variables: 12,
            corners: vec![[0, 1, 2], [3, 4, 5]],
            singularities: vec![],
            features: Default::default(),
            reference: vec![],
            edges: vec![Edge {
                ends: [0, 1],
                sides: [(0, 0), (1, 0)],
                rotation: 1,
                translation: Some(12),
            }],
            problem: Problem::new(14),
        };
        atlas.problem.integers.extend([0, 12, 13]);
        let mut x = vec![
            1. + 1e-15,
            4. + 1e-12,
            0.2,
            0.3,
            0.4,
            0.5,
            2.7,
            6.2,
            -1. - 1e-12,
            7. + 1e-15,
            0.6,
            0.7,
            3.,
            6.,
        ];
        let fraction = x[1];
        let equivalent_fraction = x[8];
        atlas.canonicalize_integers(&mut x).unwrap();
        assert_eq!(x[0], 1.);
        assert_eq!(x[9], 7.);
        assert_eq!(x[1], fraction);
        assert_eq!(x[8], equivalent_fraction);
        x[9] = 7.01;
        assert!(
            atlas
                .canonicalize_integers(&mut x)
                .unwrap_err()
                .contains("not integral")
        );
    }

    #[test]
    fn thin_triangle_orientation_bounds_remain_above_solver_tolerances() {
        for tip in [
            DVec2::new(0.5, 0.8),
            DVec2::new(0.3, 0.01),
            DVec2::new(0.3, 1e-7),
        ] {
            let reference = [DVec2::ZERO, DVec2::X, tip];
            let atlas = Atlas {
                uv_variables: 6,
                corners: vec![[0, 1, 2]],
                singularities: vec![],
                features: Default::default(),
                edges: vec![],
                reference: vec![reference],
                problem: Problem::new(6),
            };
            let rows = atlas.orientation_constraints(0).unwrap();
            let x: Vec<_> = reference.iter().flat_map(|p| [p.x, p.y]).collect();
            let shrunk: Vec<_> = x.iter().map(|v| v * 0.01).collect();
            let reflected: Vec<_> = reference.iter().flat_map(|p| [p.x, -p.y]).collect();
            for (row, b) in &rows {
                assert_eq!(*b, 0.001);
                assert!((super::super::quadratic::evaluate(row, &x) - 1.).abs() < 1e-8);
                assert!(super::super::quadratic::evaluate(row, &shrunk) >= *b);
            }
            assert!(
                rows.iter()
                    .any(|(row, b)| super::super::quadratic::evaluate(row, &reflected) < b - 1e-7)
            );
        }
    }

    #[test]
    fn full_turn_poles_are_integer_constraints_even_with_trivial_holonomy() {
        // Two eight-triangle fans form a closed bipyramid. Each map fan wraps
        // twice: its center has cone angle 4π, although rotation modulo 2π is 0.
        let triangles: Vec<_> = (0..8)
            .map(|k| [0, k + 1, (k + 1) % 8 + 1])
            .chain((0..8).map(|k| [9, (k + 1) % 8 + 1, k + 1]))
            .collect();
        let source = crate::Surface {
            positions: std::iter::once(Vec3::Z)
                .chain((0..8).map(|k| {
                    let a = k as f64 * std::f64::consts::FRAC_PI_4;
                    Vec3::new(a.cos(), a.sin(), 0.)
                }))
                .chain(std::iter::once(-Vec3::Z))
                .collect(),
            triangles,
        };
        crate::topology::validate(&source).unwrap();
        let mut x = Vec::new();
        for k in 0..16 {
            let a = k as f64 * std::f64::consts::FRAC_PI_2;
            let b = a + std::f64::consts::FRAC_PI_2;
            x.extend([0., 0., a.cos(), a.sin(), b.cos(), b.sin()]);
        }
        let mut atlas = Atlas {
            uv_variables: x.len(),
            corners: (0..16).map(|f| [3 * f, 3 * f + 1, 3 * f + 2]).collect(),
            singularities: Vec::new(),
            features: Default::default(),
            edges: Vec::new(),
            reference: Vec::new(),
            problem: Problem::new(x.len()),
        };
        atlas
            .complete_singularities(&source, &x, &|| false)
            .unwrap();
        let vertices: Vec<_> = atlas
            .singularities
            .iter()
            .map(|&(f, k)| source.triangles[f][k])
            .collect();
        assert!(vertices.contains(&0) && vertices.contains(&9));
        for &(f, k) in &atlas.singularities {
            assert!(atlas.problem.integers.contains(&(2 * atlas.corners[f][k])));
            assert!(
                atlas
                    .problem
                    .integers
                    .contains(&(2 * atlas.corners[f][k] + 1))
            );
        }
    }
}
