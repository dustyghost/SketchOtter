//! Extract unit cells by joining only adjacent source-triangle fragments.
//! Overlapping UV charts never weld distant surface sheets.
use super::atlas::{Atlas, rotate};
use crate::{Result, Surface, Vec3, topology::Dsu};
use glam::DVec2;
use std::collections::{BTreeMap, BTreeSet};

fn barycentric(p: DVec2, t: [DVec2; 3]) -> [f64; 3] {
    let b = (p - t[0]).perp_dot(t[2] - t[0]) / (t[1] - t[0]).perp_dot(t[2] - t[0]);
    let c = (t[1] - t[0]).perp_dot(p - t[0]) / (t[1] - t[0]).perp_dot(t[2] - t[0]);
    [1. - b - c, b, c]
}

fn clipped_area(t: [DVec2; 3], cell: DVec2) -> f64 {
    let mut polygon = t.to_vec();
    for axis in 0..2 {
        for side in 0..2 {
            let mut next = Vec::new();
            let distance = |p: DVec2| {
                if side == 0 {
                    p[axis] - cell[axis]
                } else {
                    cell[axis] + 1. - p[axis]
                }
            };
            for i in 0..polygon.len() {
                let a = polygon[i];
                let b = polygon[(i + 1) % polygon.len()];
                let da = distance(a);
                let db = distance(b);
                if da >= 0. {
                    next.push(a);
                }
                if (da < 0.) != (db < 0.) {
                    next.push(a + (b - a) * (da / (da - db)));
                }
            }
            polygon = next;
        }
    }
    (0..polygon.len())
        .map(|i| polygon[i].perp_dot(polygon[(i + 1) % polygon.len()]))
        .sum::<f64>()
        .abs()
        * 0.5
}

fn edge_interval(a: DVec2, b: DVec2, cell: DVec2) -> Option<(f64, f64)> {
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    for axis in 0..2 {
        let d = b[axis] - a[axis];
        if d.abs() < 1e-12 {
            if a[axis] < cell[axis] - 1e-8 || a[axis] > cell[axis] + 1. + 1e-8 {
                return None;
            }
        } else {
            let u = (cell[axis] - a[axis]) / d;
            let v = (cell[axis] + 1. - a[axis]) / d;
            lo = lo.max(u.min(v));
            hi = hi.min(u.max(v));
        }
    }
    (hi - lo > 1e-8).then_some((lo, hi))
}

struct Rotations {
    parent: Vec<usize>,
    rotation: Vec<i32>,
}
impl Rotations {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rotation: vec![0; n],
        }
    }
    fn find(&mut self, v: usize) -> (usize, i32) {
        let mut root = v;
        let mut rotation = 0;
        while self.parent[root] != root {
            rotation += self.rotation[root];
            root = self.parent[root];
        }
        let mut p = v;
        let mut left = rotation;
        while self.parent[p] != p {
            let next = self.parent[p];
            let r = self.rotation[p];
            self.parent[p] = root;
            self.rotation[p] = left.rem_euclid(4);
            p = next;
            left -= r;
        }
        (root, rotation.rem_euclid(4))
    }
    fn join(&mut self, a: usize, b: usize, rotation: i32) -> Result<()> {
        let (ra, ka) = self.find(a);
        let (rb, kb) = self.find(b);
        if ra == rb {
            if (rotation + kb - ka).rem_euclid(4) != 0 {
                return Err("Integer-grid cell has inconsistent seam rotation".into());
            }
        } else {
            self.parent[ra] = rb;
            self.rotation[ra] = (rotation + kb - ka).rem_euclid(4);
        }
        Ok(())
    }
}

struct Fragment {
    cell: [i32; 2],
    corners: Vec<(usize, usize)>,
}

pub(super) fn extract(
    source: &Surface,
    atlas: &Atlas,
    x: &[f64],
    limit: usize,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<Vec3>, Vec<[usize; 4]>)> {
    let coordinates: Vec<_> = (0..source.triangles.len())
        .map(|f| atlas.coordinates(x, f))
        .collect();
    let mut fragments = Vec::<Fragment>::new();
    let mut cell_ids = BTreeMap::new();
    let mut points = BTreeMap::<(usize, i32, i32), usize>::new();
    let mut positions = Vec::<Vec3>::new();
    let mut face_fragments = vec![Vec::new(); source.triangles.len()];
    let mut face_points = vec![Vec::new(); source.triangles.len()];
    let offsets = [[0, 0], [1, 0], [1, 1], [0, 1]];
    let mut visits = 0usize;
    for (f, &t) in coordinates.iter().enumerate() {
        crate::check(cancel)?;
        if t.iter()
            .any(|p| !p.is_finite() || p.abs().max_element() > 1_000_000.)
        {
            return Err("Integer-grid coordinates exceed extraction limits".into());
        }
        let min = t
            .into_iter()
            .fold(DVec2::splat(f64::INFINITY), DVec2::min)
            .floor();
        let max = t
            .into_iter()
            .fold(DVec2::splat(f64::NEG_INFINITY), DVec2::max)
            .floor();
        for u in min.x as i32..=max.x as i32 {
            for v in min.y as i32..=max.y as i32 {
                visits += 1;
                if visits % 1024 == 0 {
                    crate::check(cancel)?;
                }
                if visits > limit.saturating_mul(64) {
                    return Err("Integer-grid extraction exceeded its cell budget".into());
                }
                if clipped_area(t, DVec2::new(u as f64, v as f64)) < 1e-10 {
                    continue;
                }
                let mut corners = Vec::new();
                for (k, [du, dv]) in offsets.into_iter().enumerate() {
                    let p = DVec2::new((u + du) as f64, (v + dv) as f64);
                    let b = barycentric(p, t);
                    if b.iter().any(|&b| b < -1e-7) {
                        continue;
                    }
                    let key = (f, u + du, v + dv);
                    let id = if let Some(&id) = points.get(&key) {
                        id
                    } else {
                        if positions.len() > limit.saturating_mul(12) {
                            return Err("Integer-grid extraction exceeded its vertex budget".into());
                        }
                        let id = positions.len();
                        positions.push(
                            (0..3)
                                .map(|k| source.positions[source.triangles[f][k]] * b[k])
                                .sum(),
                        );
                        points.insert(key, id);
                        face_points[f].push((p, id));
                        id
                    };
                    corners.push((k, id));
                }
                let id = fragments.len();
                cell_ids.insert((f, u, v), id);
                face_fragments[f].push(id);
                fragments.push(Fragment {
                    cell: [u, v],
                    corners,
                });
            }
        }
    }
    let mut cells = Rotations::new(fragments.len());
    let mut vertices = Dsu::new(positions.len());
    for (id, e) in atlas.edges.iter().enumerate() {
        if id % 256 == 0 {
            crate::check(cancel)?;
        }
        let [(f, k), (g, _)] = e.sides;
        let shift = e
            .translation
            .map(|i| DVec2::new(x[i].round(), x[i + 1].round()))
            .unwrap_or(DVec2::ZERO);
        let transform = |p| rotate(p, e.rotation) + shift;
        let a = coordinates[f][k];
        let b = coordinates[f][(k + 1) % 3];
        for &(p, id) in &face_points[f] {
            let bary = barycentric(p, coordinates[f]);
            if bary[(k + 2) % 3].abs() > 1e-7 {
                continue;
            }
            let q = transform(p).round();
            let Some(&other) = points.get(&(g, q.x as i32, q.y as i32)) else {
                return Err("Integer-grid seam has an unmatched vertex".into());
            };
            vertices.join(id, other);
        }
        for &id in &face_fragments[f] {
            let [u, v] = fragments[id].cell;
            let cell = DVec2::new(u as f64, v as f64);
            if edge_interval(a, b, cell).is_none() {
                continue;
            }
            let q = transform(cell + DVec2::splat(0.5)).floor();
            if let Some(&other) = cell_ids.get(&(g, q.x as i32, q.y as i32)) {
                cells.join(id, other, e.rotation)?;
            }
        }
    }
    // Distinct source singularities must survive as distinct grid vertices.
    // Check source provenance, not global UV coordinates: unrelated charts
    // can legitimately overlap. This is a rejection check, not the IGM
    // paper's separator constraints during continuous optimization.
    let mut singular_roots = BTreeSet::new();
    for &(face, corner) in &atlas.singularities {
        crate::check(cancel)?;
        let p = coordinates[face][corner].round();
        let Some(&vertex) = points.get(&(face, p.x as i32, p.y as i32)) else {
            return Err("Integer-grid extraction lost a source singularity".into());
        };
        if !singular_roots.insert(vertices.find(vertex)) {
            return Err("Integer-grid extraction merged distinct source singularities".into());
        }
    }
    let mut compact = BTreeMap::new();
    let mut output = Vec::<Vec3>::new();
    let mut counts = Vec::<usize>::new();
    for (v, p) in positions.into_iter().enumerate() {
        if v % 1024 == 0 {
            crate::check(cancel)?;
        }
        let root = vertices.find(v);
        let id = *compact.entry(root).or_insert_with(|| {
            output.push(Vec3::ZERO);
            counts.push(0);
            output.len() - 1
        });
        output[id] += p;
        counts[id] += 1;
    }
    for (p, n) in output.iter_mut().zip(counts) {
        *p /= n as f64;
    }
    if output.len() > limit {
        return Err("Integer-grid output exceeds the vertex budget".into());
    }
    let mut quads = BTreeMap::<usize, [Option<usize>; 4]>::new();
    for (i, fragment) in fragments.iter().enumerate() {
        if i % 1024 == 0 {
            crate::check(cancel)?;
        }
        let (root, rotation) = cells.find(i);
        let quad = quads.entry(root).or_insert([None; 4]);
        for &(corner, vertex) in &fragment.corners {
            let slot = (corner + rotation as usize) % 4;
            let v = compact[&vertices.find(vertex)];
            if quad[slot].is_some_and(|old| old != v) {
                return Err("Integer-grid cell corners disagree across a seam".into());
            }
            quad[slot] = Some(v);
        }
    }
    let mut output_quads = Vec::with_capacity(quads.len());
    for quad in quads.into_values() {
        let [Some(a), Some(b), Some(c), Some(d)] = quad else {
            return Err("Integer-grid extraction found an incomplete cell".into());
        };
        let q = [a, b, c, d];
        if (0..4).any(|i| (i + 1..4).any(|j| q[i] == q[j])) {
            return Err("Integer-grid extraction collapsed a quad".into());
        }
        output_quads.push(q);
    }
    Ok((output, output_quads))
}
