//! Tiny fixtures and the independent surface-fan oracle introduced in #170.
use super::{Mesh, Vec3};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn shells(closed: bool) -> Mesh {
    let (vertices, triangles) = if closed {
        (
            vec![
                Vec3(-0.1, 0., 0.),
                Vec3(-1., -1., -1.),
                Vec3(-1., 1., -1.),
                Vec3(-1., 0., 1.),
                Vec3(0.1, 0., 0.),
                Vec3(1., -1., -1.),
                Vec3(1., 1., -1.),
                Vec3(1., 0., 1.),
            ],
            vec![
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 1],
                [1, 3, 2],
                [4, 6, 5],
                [4, 7, 6],
                [4, 5, 7],
                [5, 6, 7],
            ],
        )
    } else {
        (
            vec![
                Vec3(-0.1, 0., 0.),
                Vec3(-1., -1., 0.),
                Vec3(-1., 1., 0.),
                Vec3(0.1, 0., 0.),
                Vec3(1., 1., 0.),
                Vec3(1., -1., 0.),
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
    };
    Mesh {
        mask: vec![0.; vertices.len()],
        vertices,
        triangles,
        ..Default::default()
    }
}

pub(crate) fn fan(sides: usize) -> Mesh {
    let mut vertices = vec![Vec3::default()];
    for i in 0..sides {
        let angle = std::f32::consts::TAU * i as f32 / sides as f32;
        vertices.push(Vec3(angle.cos(), angle.sin(), 0.));
    }
    Mesh {
        mask: vec![0.; vertices.len()],
        vertices,
        triangles: (0..sides)
            .map(|i| [0, i + 1, (i + 1) % sides + 1])
            .collect(),
        ..Default::default()
    }
}

// Independent of Mesh::validate, validate_closed and the production corner/link
// walkers. Build undirected links directly from triangles, then flood each fan.
pub(crate) fn surface_error(mesh: &Mesh) -> Option<&'static str> {
    if mesh
        .vertices
        .iter()
        .any(|p| !p.0.is_finite() || !p.1.is_finite() || !p.2.is_finite())
    {
        return Some("nonfinite position");
    }
    let mut faces = BTreeSet::new();
    let mut oriented = BTreeSet::new();
    let mut incidence = BTreeMap::<[usize; 2], (usize, i32)>::new();
    let mut links = vec![BTreeMap::<usize, BTreeSet<usize>>::new(); mesh.vertices.len()];
    for &t in &mesh.triangles {
        if t.iter().any(|&v| v >= mesh.vertices.len()) {
            return Some("invalid index");
        }
        let mut key = t;
        key.sort_unstable();
        if key.windows(2).any(|p| p[0] == p[1]) {
            return Some("repeated corner");
        }
        if !faces.insert(key) {
            return Some("duplicate face");
        }
        oriented.insert(cycle(t));
        let [a, b, c] = t.map(|v| mesh.vertices[v]);
        if (b - a).cross(c - a).len() <= 1e-12 {
            return Some("zero area");
        }
        for i in 0..3 {
            let (v, a, b) = (t[i], t[(i + 1) % 3], t[(i + 2) % 3]);
            let entry = incidence.entry([v.min(a), v.max(a)]).or_default();
            entry.0 += 1;
            entry.1 += if v < a { 1 } else { -1 };
            links[v].entry(a).or_default().insert(b);
            links[v].entry(b).or_default().insert(a);
        }
    }
    if incidence
        .values()
        .any(|&(n, w)| n > 2 || (n == 2 && w != 0))
    {
        return Some("edge incidence/winding");
    }
    for link in links {
        if link.values().any(|n| n.len() > 2) {
            return Some("branched fan");
        }
        let ends = link.values().filter(|n| n.len() == 1).count();
        if ends != 0 && ends != 2 {
            return Some("invalid fan boundary");
        }
        if let Some(&start) = link.keys().next() {
            let mut visited = BTreeSet::new();
            let mut stack = vec![start];
            while let Some(v) = stack.pop() {
                if visited.insert(v) {
                    stack.extend(&link[&v]);
                }
            }
            if visited.len() != link.len() {
                return Some("disconnected surface fan");
            }
        }
    }
    // A metadata quad must own two distinct, correctly wound render triangles.
    // Do not infer quads from triangle count or call the production polygon API.
    let mut owned = BTreeSet::new();
    for &[a, b, c, d] in &mesh.quads {
        if [a, b, c, d].into_iter().collect::<BTreeSet<_>>().len() != 4 {
            return Some("repeated quad corner");
        }
        let pair = [[[a, b, c], [a, c, d]], [[a, b, d], [b, c, d]]]
            .into_iter()
            .map(|ts| ts.map(cycle))
            .find(|ts| ts.iter().all(|t| oriented.contains(t)));
        let Some(pair) = pair else {
            return Some("quad without oriented triangles");
        };
        if !pair.into_iter().all(|t| owned.insert(t)) {
            return Some("overlapping quad metadata");
        }
    }
    None
}
fn cycle(t: [usize; 3]) -> [usize; 3] {
    [t, [t[1], t[2], t[0]], [t[2], t[0], t[1]]]
        .into_iter()
        .min()
        .unwrap()
}
pub(crate) fn surface_invariants(mesh: &Mesh) -> bool {
    surface_error(mesh).is_none()
}
pub(crate) fn closed_surface(mesh: &Mesh) -> bool {
    let mut edges = BTreeMap::<[usize; 2], usize>::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry([a.min(b), a.max(b)]).or_default() += 1;
        }
    }
    !edges.is_empty() && edges.values().all(|&n| n == 2)
}

#[test]
fn independent_oracle_rejects_known_invalid_surfaces_and_quad_metadata() {
    let valid = fan(8);
    assert!(surface_invariants(&valid));
    let mut bad = valid.clone();
    bad.triangles[0][0] = bad.vertices.len();
    assert_eq!(surface_error(&bad), Some("invalid index"));
    let mut bad = valid.clone();
    bad.vertices[0].0 = f32::NAN;
    assert_eq!(surface_error(&bad), Some("nonfinite position"));
    let mut bad = valid.clone();
    bad.triangles.push(bad.triangles[0]);
    assert_eq!(surface_error(&bad), Some("duplicate face"));
    let mut bad = valid.clone();
    bad.triangles[0].swap(0, 1);
    assert_eq!(surface_error(&bad), Some("edge incidence/winding"));
    let mut bad = valid.clone();
    bad.vertices[1] = bad.vertices[0];
    assert_eq!(surface_error(&bad), Some("zero area"));
    let mut bad = valid;
    bad.quads.push([0, 1, 2, 7]);
    assert_eq!(surface_error(&bad), Some("quad without oriented triangles"));
    for closed in [false, true] {
        let mut bad = shells(closed);
        assert!(surface_invariants(&bad));
        let other = if closed { 4 } else { 3 };
        for t in &mut bad.triangles {
            *t = t.map(|v| if v == other { 0 } else { v });
        }
        assert_eq!(
            surface_error(&bad),
            Some(if closed {
                "disconnected surface fan"
            } else {
                "invalid fan boundary"
            })
        );
    }
    let mut bad = super::Primitive::for_kind(super::PrimitiveKind::Cube)
        .mesh()
        .unwrap();
    assert!(surface_invariants(&bad));
    bad.quads.push(bad.quads[0]);
    assert_eq!(surface_error(&bad), Some("overlapping quad metadata"));
}
