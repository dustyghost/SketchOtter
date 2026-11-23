//! Sharp feature graph and shape-preserving preparation for MIQ §5.2.
use crate::{Options, Result, Surface, check, topology::edge};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default)]
pub(super) struct Features {
    pub edges: BTreeSet<[usize; 2]>,
    /// Ordered vertex chains; a closed chain repeats its first vertex at the end.
    pub chains: Vec<Vec<usize>>,
    pub corners: BTreeSet<usize>,
}

impl Features {
    pub fn detect(s: &Surface, threshold: Option<f64>, cancel: &dyn Fn() -> bool) -> Result<Self> {
        let Some(threshold) = threshold else {
            return Ok(Self::default());
        };
        let mut incidence = BTreeMap::<_, Vec<_>>::new();
        for (f, &[a, b, c]) in s.triangles.iter().enumerate() {
            if f % 1024 == 0 {
                check(cancel)?;
            }
            let n = (s.positions[b] - s.positions[a])
                .cross(s.positions[c] - s.positions[a])
                .normalize();
            for e in [edge(a, b), edge(b, c), edge(c, a)] {
                incidence.entry(e).or_default().push(n);
            }
        }
        let mut edges: BTreeSet<_> = incidence
            .iter()
            .filter_map(|(&e, ns)| (ns.len() == 2 && ns[0].dot(ns[1]) <= threshold).then_some(e))
            .collect();
        // A nearly collinear cap ear can have an unstable, reversed normal.
        // When all three sides appear sharp, its longest side duplicates the
        // two shorter sides to within 1e-5 of its length. Keep the detailed
        // boundary route, avoiding a spurious microscopic feature cycle. The
        // opposite normals on the chord distinguish this from a real corner.
        let mut redundant = BTreeSet::new();
        for (f, &[a, b, c]) in s.triangles.iter().enumerate() {
            if f % 1024 == 0 {
                check(cancel)?;
            }
            let es = [edge(a, b), edge(b, c), edge(c, a)];
            if !es.iter().all(|e| edges.contains(e)) {
                continue;
            }
            let longest = *es
                .iter()
                .max_by(|a, b| {
                    s.positions[a[0]]
                        .distance_squared(s.positions[a[1]])
                        .total_cmp(&s.positions[b[0]].distance_squared(s.positions[b[1]]))
                })
                .unwrap();
            let d = s.positions[longest[1]] - s.positions[longest[0]];
            let third = *[a, b, c].iter().find(|v| !longest.contains(v)).unwrap();
            let t = (s.positions[third] - s.positions[longest[0]]).dot(d) / d.length_squared();
            let height = (s.positions[third] - s.positions[longest[0]])
                .cross(d)
                .length()
                / d.length();
            let ns = &incidence[&longest];
            if (0. ..1.).contains(&t) && height < 1e-5 * d.length() && ns[0].dot(ns[1]) < -0.99 {
                redundant.insert(longest);
            }
        }
        edges.retain(|e| !redundant.contains(e));
        Self::from_edges(s, edges, cancel)
    }

    pub fn from_edges(
        s: &Surface,
        edges: BTreeSet<[usize; 2]>,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Self> {
        let mut adjacent = BTreeMap::<usize, Vec<usize>>::new();
        for &[a, b] in &edges {
            adjacent.entry(a).or_default().push(b);
            adjacent.entry(b).or_default().push(a);
        }
        let corners: BTreeSet<_> = adjacent
            .iter()
            .filter_map(|(&v, ns)| {
                // Preserve endpoints/junctions and turns of 45 degrees or more.
                // Gentle turns stay continuous curves, not one grid vertex per input edge.
                let corner = ns.len() != 2 || {
                    let [a, b] =
                        [ns[0], ns[1]].map(|u| (s.positions[u] - s.positions[v]).normalize());
                    a.dot(b) >= -std::f64::consts::FRAC_1_SQRT_2
                };
                corner.then_some(v)
            })
            .collect();
        let mut unused = edges.clone();
        let mut chains = Vec::new();
        let starts: Vec<_> = corners
            .iter()
            .flat_map(|&v| adjacent[&v].iter().map(move |&u| (v, u)))
            .chain(edges.iter().map(|&[a, b]| (a, b)))
            .collect();
        for (start, mut next) in starts {
            check(cancel)?;
            if !unused.remove(&edge(start, next)) {
                continue;
            }
            let mut chain = vec![start];
            let mut prev = start;
            loop {
                if chain.len() % 1024 == 0 {
                    check(cancel)?;
                }
                chain.push(next);
                if next == start || corners.contains(&next) {
                    break;
                }
                let onward = *adjacent[&next].iter().find(|&&v| v != prev).unwrap();
                if !unused.remove(&edge(next, onward)) {
                    return Err("Sharp feature chain is inconsistent".into());
                }
                (prev, next) = (next, onward);
            }
            chains.push(chain);
        }
        Ok(Self {
            edges,
            chains,
            corners,
        })
    }
}

/// A triangle with three vertices on one feature curve cannot stay positive
/// when that curve is mapped to a grid line. Split its interior chords first;
/// existing feature edges and all source positions remain unchanged.
pub(super) fn prepare(
    source: &Surface,
    options: &Options,
    cancel: &dyn Fn() -> bool,
) -> Result<(Surface, Options, BTreeSet<[usize; 2]>)> {
    let features = Features::detect(source, options.sharp_threshold(), cancel)?;
    let vertices: BTreeSet<_> = features.edges.iter().flatten().copied().collect();
    let mut mid = BTreeMap::new();
    let mut centers = BTreeSet::new();
    for (f, t) in source.triangles.iter().enumerate() {
        if f % 1024 == 0 {
            check(cancel)?;
        }
        let es = std::array::from_fn::<_, 3, _>(|k| edge(t[k], t[(k + 1) % 3]));
        if es.iter().all(|e| features.edges.contains(e)) {
            centers.insert(f);
        }
        for e in es {
            if e.iter().all(|v| vertices.contains(v)) && !features.edges.contains(&e) {
                mid.insert(e, 0);
            }
        }
    }
    if source.positions.len() + mid.len() + centers.len() > options.max_vertices {
        return Err("Sharp feature preparation exceeds the vertex budget".into());
    }
    let mut mesh = source.clone();
    let mut options = options.clone();
    if options.detail.is_empty() {
        options.detail = vec![0.; mesh.positions.len()];
    }
    for (&[a, b], id) in &mut mid {
        *id = mesh.positions.len();
        mesh.positions
            .push((mesh.positions[a] + mesh.positions[b]) * 0.5);
        options
            .detail
            .push((options.detail[a] + options.detail[b]) * 0.5);
    }
    for &f in &centers {
        check(cancel)?;
        let [a, b, c] = source.triangles[f];
        let v = mesh.positions.len();
        mesh.positions
            .push((mesh.positions[a] + mesh.positions[b] + mesh.positions[c]) / 3.);
        options
            .detail
            .push((options.detail[a] + options.detail[b] + options.detail[c]) / 3.);
        mesh.triangles[f] = [a, b, v];
        mesh.triangles.extend([[b, c, v], [c, a, v]]);
    }
    mesh.triangles = crate::field::split_triangles(&mesh.triangles, &mid, cancel)?;
    Ok((mesh, options, features.edges))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::Vec3;

    pub fn cube() -> Surface {
        let positions = vec![
            Vec3::new(-1., -1., -1.),
            Vec3::new(1., -1., -1.),
            Vec3::new(1., 1., -1.),
            Vec3::new(-1., 1., -1.),
            Vec3::new(-1., -1., 1.),
            Vec3::new(1., -1., 1.),
            Vec3::new(1., 1., 1.),
            Vec3::new(-1., 1., 1.),
        ];
        let quads = [
            [0, 3, 2, 1],
            [4, 5, 6, 7],
            [0, 1, 5, 4],
            [3, 7, 6, 2],
            [0, 4, 7, 3],
            [1, 2, 6, 5],
        ];
        Surface {
            positions,
            triangles: quads
                .into_iter()
                .flat_map(|[a, b, c, d]| [[a, b, c], [a, c, d]])
                .collect(),
        }
    }

    #[test]
    fn sharp_graph_and_preparation_keep_cube_geometry_and_junctions() {
        let s = cube();
        let o = Options {
            target_quads: 96,
            detail: s
                .positions
                .iter()
                .map(|p| 0.3 * p.x + 0.1 * p.y + 0.2 * p.z)
                .collect(),
            ..Options::default()
        };
        let g = Features::detect(&s, o.sharp_threshold(), &|| false).unwrap();
        assert_eq!(
            (g.edges.len(), g.chains.len(), g.corners.len()),
            (12, 12, 8)
        );
        let (p, op, _) = prepare(&s, &o, &|| false).unwrap();
        assert_eq!(p.positions.len(), 14);
        assert_eq!(p.triangles.len(), 24);
        assert_eq!(&p.positions[..8], s.positions.as_slice());
        assert_eq!(op.detail.len(), 14);
        assert_eq!(&op.detail[..8], o.detail.as_slice());
        for (p, &d) in p.positions.iter().zip(&op.detail).skip(8) {
            let ends: Vec<_> = s
                .positions
                .iter()
                .enumerate()
                .filter(|(_, a)| a.distance(*p) < 1.5)
                .collect();
            // The face center is the midpoint of a coplanar diagonal.
            assert_eq!(ends.len(), 4);
            let expected = ends.iter().map(|(v, _)| o.detail[*v]).sum::<f64>() / 4.;
            assert!((d - expected).abs() < 1e-12);
        }
        assert_eq!(
            crate::topology::validate(&p).unwrap(),
            crate::topology::validate(&s).unwrap()
        );
        let h = Features::detect(&p, o.sharp_threshold(), &|| false).unwrap();
        assert_eq!(g.edges, h.edges);
        assert!(p.positions.iter().all(|v| v.abs().max_element() == 1.));
        assert!(
            Features::detect(&s, Some(-1.), &|| false)
                .unwrap()
                .edges
                .is_empty()
        );
        assert!(
            prepare(
                &s,
                &Options {
                    max_vertices: 8,
                    ..o.clone()
                },
                &|| false
            )
            .unwrap_err()
            .contains("vertex budget")
        );
        assert!(prepare(&s, &o, &|| true).unwrap_err().contains("cancelled"));
    }

    #[test]
    fn sharp_cube_reconstruction_keeps_all_twelve_edge_chains() {
        for narrow_first_face in [false, true] {
            let mut s = cube();
            if narrow_first_face {
                let [a, b, c] = s.triangles[0];
                let v = s.positions.len();
                s.positions
                    .push(s.positions[a] * 0.98 + (s.positions[b] + s.positions[c]) * 0.01);
                s.triangles[0] = [v, a, b];
                s.triangles.extend([[v, b, c], [v, c, a]]);
            }
            let before = s.clone();
            let o = Options {
                target_quads: 96,
                ..Options::default()
            };
            let result =
                crate::reconstruct_integer_grid_with_progress(&s, &o, &|| false, &|_| {}).unwrap();
            assert!(result.quads.len() > 20);
            assert_eq!(s.positions, before.positions);
            assert_eq!(s.triangles, before.triangles);
            for v in s.positions.iter().take(8) {
                assert!(
                    result.positions.iter().any(|p| p.distance(*v) < 1e-7),
                    "lost cube corner {v}"
                );
            }
            let original = Features::detect(&s, o.sharp_threshold(), &|| false).unwrap();
            let edges: BTreeSet<_> = result
                .quads
                .iter()
                .flat_map(|q| (0..4).map(|k| edge(q[k], q[(k + 1) % 4])))
                .collect();
            for [a, b] in original.edges {
                let [a, b] = [s.positions[a], s.positions[b]];
                let direction = b - a;
                let on_line = |p: crate::Vec3| {
                    let t = (p - a).dot(direction) / direction.length_squared();
                    (0. - 1e-7..=1. + 1e-7).contains(&t) && (p - a).cross(direction).length() < 1e-7
                };
                let length: f64 = edges
                    .iter()
                    .filter(|&&[u, v]| on_line(result.positions[u]) && on_line(result.positions[v]))
                    .map(|&[u, v]| result.positions[u].distance(result.positions[v]))
                    .sum();
                assert!(
                    (length - direction.length()).abs() < 1e-6,
                    "missing cube feature length {length}"
                );
            }
        }
    }

    fn cylinder(n: usize) -> Surface {
        let mut positions: Vec<_> = [-1., 1.]
            .into_iter()
            .flat_map(|z| {
                (0..n).map(move |i| {
                    let a = std::f64::consts::TAU * i as f64 / n as f64;
                    Vec3::new(a.cos(), a.sin(), z)
                })
            })
            .collect();
        positions.extend([Vec3::new(0., 0., -1.), Vec3::new(0., 0., 1.)]);
        let triangles = (0..n)
            .flat_map(|i| {
                let j = (i + 1) % n;
                [
                    [i, j, j + n],
                    [i, j + n, i + n],
                    [2 * n, j, i],
                    [2 * n + 1, i + n, j + n],
                ]
            })
            .collect();
        Surface {
            positions,
            triangles,
        }
    }

    #[test]
    fn nearly_collinear_cap_ear_does_not_create_a_second_feature_route() {
        let mut s = cylinder(24);
        let [a, b, c] = [24, 25, 26];
        let p = (s.positions[a] + s.positions[c]) * 0.5;
        s.positions[b] = p - Vec3::new(p.x, p.y, 0.) * 1e-8;
        s.triangles.retain(|t| *t != [49, a, b] && *t != [49, b, c]);
        s.triangles.extend([[a, b, c], [49, a, c]]);
        let o = Options {
            target_quads: 128,
            ..Options::default()
        };
        let graph = Features::detect(&s, o.sharp_threshold(), &|| false).unwrap();
        assert_eq!(graph.edges.len(), 48);
        assert!(!graph.edges.contains(&edge(a, c)));
        assert!(graph.edges.contains(&edge(a, b)) && graph.edges.contains(&edge(b, c)));
        assert_eq!(graph.chains.len(), 2);
        assert!(graph.corners.is_empty());
        let (field, graph) = crate::miq::solve_field(&s, &o, &|| false, &|_| {}).unwrap();
        assert!(field.surface.positions.len() > s.positions.len());
        assert_eq!(graph.chains.len(), 2);
        assert!(graph.corners.is_empty());
        assert!(graph.chains.iter().all(|c| c.first() == c.last()));
    }

    #[test]
    fn curved_sharp_rims_form_two_closed_quad_edge_loops() {
        let s = cylinder(24);
        let o = Options {
            target_quads: 128,
            ..Options::default()
        };
        let graph = Features::detect(&s, o.sharp_threshold(), &|| false).unwrap();
        assert_eq!(graph.edges.len(), 48);
        assert_eq!(graph.chains.len(), 2);
        assert!(graph.corners.is_empty());
        assert!(graph.chains.iter().all(|c| c.first() == c.last()));
        let result =
            crate::reconstruct_integer_grid_with_progress(&s, &o, &|| false, &|_| {}).unwrap();
        let edges: BTreeSet<_> = result
            .quads
            .iter()
            .flat_map(|q| (0..4).map(|k| edge(q[k], q[(k + 1) % 4])))
            .collect();
        for z in [-1., 1.] {
            let on_rim = |p: Vec3| (p.z - z).abs() < 1e-7 && p.truncate().length() > 0.98;
            let mut adjacent = BTreeMap::<usize, Vec<usize>>::new();
            for &[a, b] in &edges {
                if on_rim(result.positions[a]) && on_rim(result.positions[b]) {
                    adjacent.entry(a).or_default().push(b);
                    adjacent.entry(b).or_default().push(a);
                }
            }
            assert!(adjacent.len() >= 8);
            assert!(adjacent.values().all(|n| n.len() == 2));
            let mut visited = BTreeSet::new();
            let mut queue = vec![*adjacent.keys().next().unwrap()];
            while let Some(v) = queue.pop() {
                if visited.insert(v) {
                    queue.extend(&adjacent[&v]);
                }
            }
            assert_eq!(
                visited.len(),
                adjacent.len(),
                "rim must be one connected loop"
            );
        }
        assert_eq!(
            crate::topology::validate(&Surface {
                positions: result.positions,
                triangles: result.triangles
            })
            .unwrap(),
            crate::topology::validate(&s).unwrap()
        );
    }
}
