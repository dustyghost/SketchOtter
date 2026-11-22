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
    /// One (face, local corner) for each distinct source singularity.
    pub singularities: Vec<(usize, usize)>,
    pub edges: Vec<Edge>,
    pub reference: Vec<[DVec2; 3]>,
    pub problem: Problem,
}

impl Atlas {
    pub fn new(field: &Field, cancel: &dyn Fn() -> bool) -> Result<Self> {
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
            let mut q = field.directions[a] - n * field.directions[a].dot(n);
            if q.length_squared() < 1e-12 {
                q = surface.positions[b] - surface.positions[a];
            }
            q = q.normalize();
            let mut sum = Vec3::ZERO;
            for v in [a, b, c] {
                let u = (field.directions[v] - n * field.directions[v].dot(n)).normalize_or_zero();
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
        for root in 0..frames.len() {
            if comb[root].is_some() {
                continue;
            }
            anchors.push(root);
            comb[root] = Some(0i32);
            let mut queue = VecDeque::from([root]);
            while let Some(f) = queue.pop_front() {
                crate::check(cancel)?;
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
                if singular[t[k]] && !marked[t[k]] {
                    marked[t[k]] = true;
                    singularities.push((f, k));
                    problem
                        .integers
                        .extend([2 * corners[f][k], 2 * corners[f][k] + 1]);
                }
            }
        }
        for f in anchors {
            for axis in 0..2 {
                problem
                    .equalities
                    .push((vec![(2 * corners[f][0] + axis, 1.)], 0.));
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
            edges,
            reference,
            problem,
        })
    }

    pub fn coordinates(&self, x: &[f64], f: usize) -> [DVec2; 3] {
        self.corners[f].map(|v| DVec2::new(x[2 * v], x[2 * v + 1]))
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

    pub fn orientation_constraints(&self, f: usize) -> Vec<(Row, f64)> {
        let reference = self.reference[f];
        let center = reference.iter().sum::<DVec2>() / 3.;
        let direction = reference.map(|p| (p - center).normalize());
        let rays: [DVec2; 3] =
            std::array::from_fn(|k| (direction[k] + direction[(k + 1) % 3]).normalize());
        let mut rows = Vec::new();
        for k in 0..3 {
            for n in [rays[(k + 2) % 3].perp(), -rays[k].perp()] {
                let mut row = Vec::new();
                for j in 0..3 {
                    for axis in 0..2 {
                        row.push((
                            2 * self.corners[f][j] + axis,
                            n[axis] * (if j == k { 2. / 3. } else { -1. / 3. }),
                        ));
                    }
                }
                rows.push((row, (reference[k] - center).dot(n) * 0.001));
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
