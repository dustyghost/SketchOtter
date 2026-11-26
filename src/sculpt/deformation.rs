//! Keep brush proposals from folding or crushing incident triangles.
use super::{Mesh, Vec3};

type Vector = [f64; 3];
fn difference(a: Vec3, b: Vec3) -> Vector {
    [
        a.0 as f64 - b.0 as f64,
        a.1 as f64 - b.1 as f64,
        a.2 as f64 - b.2 as f64,
    ]
}
fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: Vector, b: Vector) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn quality(a: Vector, b: Vector) -> f64 {
    let c = std::array::from_fn(|i| b[i] - a[i]);
    let normal = cross(a, b);
    2. * 3_f64.sqrt() * dot(normal, normal).sqrt() / (dot(a, a) + dot(b, b) + dot(c, c))
}

pub(super) fn safe_face(before: [Vec3; 3], after: [Vec3; 3], direction: Option<Vec3>) -> bool {
    let a = difference(before[1], before[0]);
    let b = difference(before[2], before[0]);
    let n = cross(a, b);
    let area_squared = dot(n, n);
    if area_squared == 0. {
        return true;
    } // Existing degenerate faces need topology repair.
    // A directional relief can turn a steep side past the old face normal
    // without folding: its footprint in the stamp plane remains unchanged.
    // Check winding in that plane for front-facing faces. Keep the original
    // guard at the silhouette and on the back, where this projection is unsafe.
    let axis = direction.map(|v| [v.0 as f64, v.1 as f64, v.2 as f64]);
    let n = axis
        .filter(|&axis| dot(n, axis) > 0.1 * (area_squared * dot(axis, axis)).sqrt())
        .unwrap_or(n);
    let projected_area = dot(cross(a, b), n);
    let x = difference(after[1], after[0]);
    let y = difference(after[2], after[0]);
    let dx = std::array::from_fn(|i| x[i] - a[i]);
    let dy = std::array::from_fn(|i| y[i] - b[i]);
    // Projected area during linear movement is 1 + linear*t + quadratic*t².
    // Check its minimum, not only the endpoint: two vertices could otherwise
    // cross and leave an apparently correctly oriented triangle at release.
    let quadratic = dot(cross(dx, dy), n) / projected_area;
    let linear = (dot(cross(a, dy), n) + dot(cross(dx, b), n)) / projected_area;
    let mut minimum = (1. + linear + quadratic).min(1.);
    if quadratic > 0. {
        let t = -linear / (2. * quadratic);
        if (0. ..1.).contains(&t) {
            minimum = minimum.min(1. + linear * t + quadratic * t * t);
        }
    }
    minimum.is_finite() && minimum >= 0.05 && quality(x, y) >= quality(a, b).min(0.02) * (1. - 1e-6)
}

fn face_normal(points: [Vec3; 3]) -> Vector {
    cross(
        difference(points[1], points[0]),
        difference(points[2], points[0]),
    )
}

fn normal_agreement(a: Vector, b: Vector) -> f64 {
    let length = (dot(a, a) * dot(b, b)).sqrt();
    if length == 0. { 1. } else { dot(a, b) / length }
}

impl Mesh {
    pub(super) fn limit_brush_deformation(
        &mut self,
        before: &[Vec3],
        direction: Option<Vec3>,
        observe: &mut impl FnMut(usize, Vec3, Vec3),
    ) -> bool {
        // An adaptive quad may have a valid but very thin alternate corner.
        // Pin a repeatedly rejected logical face locally, as for smoothing,
        // rather than cancelling unrelated valid parts of the whole dab.
        // Preserve the established triangle-only reduction path.
        self.limit_deformation(before, false, direction, !self.quads.is_empty(), observe)
    }

    pub(super) fn limit_projection_deformation(&mut self, before: &[Vec3]) -> bool {
        self.limit_deformation(before, true, None, false, &mut |_, _, _| {})
    }

    pub(super) fn limit_local_projection_deformation(&mut self, before: &[Vec3]) -> bool {
        self.limit_deformation(before, true, None, true, &mut |_, _, _| {})
    }

    pub(super) fn limit_smoothing_deformation(&mut self, before: &[Vec3]) -> bool {
        self.limit_deformation(before, false, None, true, &mut |_, _, _| {})
    }

    fn limit_deformation(
        &mut self,
        before: &[Vec3],
        prevent_folds: bool,
        direction: Option<Vec3>,
        local_rejection: bool,
        observe: &mut impl FnMut(usize, Vec3, Vec3),
    ) -> bool {
        // Each vertex belongs to several faces. Compare its position once,
        // then discover incident faces using the compact flags.
        let changed: Vec<_> = before
            .iter()
            .zip(&self.vertices)
            .map(|(a, b)| a != b)
            .collect();
        let active: Vec<_> = self
            .polygons()
            .into_iter()
            .filter(|p| p.vertices.iter().any(|&v| changed[v]))
            .collect();
        let safe_polygon = |p: &super::edges::Polygon, vertices: &[Vec3]| {
            p.symmetric_triangles().all(|(t, _)| {
                let old = t.map(|v| before[v]);
                // Adaptive quads can have three collinear boundary corners.
                // The alternate diagonal then creates a virtual zero-area
                // triangle. f32 midpoint rounding leaves a tiny, unstable
                // normal: do not use it to reject an otherwise valid quad.
                // Keep the original guard for triangle-only faces and every
                // meaningful triangle of either quad triangulation.
                (p.vertices.len() == 4
                    && quality(difference(old[1], old[0]), difference(old[2], old[0]))
                        <= 8. * f32::EPSILON as f64)
                    || safe_face(old, t.map(|v| vertices[v]), direction)
            })
        };
        #[cfg(test)]
        super::stamp_profile::mark("guard_active_faces");
        if active.is_empty() {
            return changed.iter().any(|&v| v);
        }
        // Individually well-oriented faces can still fold back against their
        // neighbour. Check shared edges too when moving onto a reference surface.
        // Preserve existing sharp folds, but never create a new opposing pair.
        let mut pairs = Vec::new();
        if prevent_folds {
            let mut edges = std::collections::HashMap::new();
            for (face, &[a, b, c]) in self.triangles.iter().enumerate() {
                for (a, b) in [(a, b), (b, c), (c, a)] {
                    let edge = if a < b { (a, b) } else { (b, a) };
                    if let Some(other) = edges.insert(edge, face) {
                        let first = self.triangles[other];
                        let second = self.triangles[face];
                        if first.iter().chain(&second).any(|&v| changed[v]) {
                            let agreement = normal_agreement(
                                face_normal(first.map(|v| before[v])),
                                face_normal(second.map(|v| before[v])),
                            );
                            pairs.push((other, face, agreement.min(0.)));
                        }
                    }
                }
            }
        }
        let safe_pair = |vertices: &[Vec3], &(a, b, minimum): &(usize, usize, f64)| {
            normal_agreement(
                face_normal(self.triangles[a].map(|v| vertices[v])),
                face_normal(self.triangles[b].map(|v| vertices[v])),
            ) >= minimum - 1e-6
        };
        #[cfg(test)]
        super::stamp_profile::mark("guard_pairs");
        drop(changed);
        let mut reduce = vec![false; self.vertices.len()];
        let mut pending = Vec::new();
        for pass in 0..if local_rejection { 20 } else { 12 } {
            let mut unsafe_face = false;
            for p in &active {
                if !safe_polygon(p, &self.vertices) {
                    unsafe_face = true;
                    // A quad is one logical face. Reduce all its corners,
                    // whichever of its two triangulations rejected the move.
                    for &v in &p.vertices {
                        if !reduce[v] {
                            reduce[v] = true;
                            pending.push(v);
                        }
                    }
                }
            }
            for pair in &pairs {
                if !safe_pair(&self.vertices, pair) {
                    unsafe_face = true;
                    for v in self.triangles[pair.0]
                        .into_iter()
                        .chain(self.triangles[pair.1])
                    {
                        if !reduce[v] {
                            reduce[v] = true;
                            pending.push(v);
                        }
                    }
                }
            }
            #[cfg(test)]
            super::stamp_profile::mark("guard_checks");
            if !unsafe_face {
                return self.vertices.iter().zip(before).any(|(a, b)| a != b);
            }
            // Reductions depend only on this vertex's old/current positions;
            // their order cannot affect another vertex in the same pass.
            for v in pending.drain(..) {
                let reduced = if local_rejection && pass >= 8 {
                    // A few pre-existing slivers must not cancel smoothing
                    // everywhere else. Pin failed proposals, then recheck
                    // neighboring faces against the resulting mixed update.
                    before[v]
                } else {
                    before[v] + (self.vertices[v] - before[v]) * 0.5
                };
                observe(v, self.vertices[v], reduced);
                self.vertices[v] = reduced;
                reduce[v] = false;
            }
            #[cfg(test)]
            super::stamp_profile::mark("guard_reduce");
        }
        #[cfg(test)]
        super::stamp_profile::mark("guard_last_reduction");
        // Never commit an unchecked last reduction; pinning one vertex can
        // affect its neighbours. Reject a pathological dab without changing positions.
        if active.iter().any(|p| !safe_polygon(p, &self.vertices))
            || pairs.iter().any(|pair| !safe_pair(&self.vertices, pair))
        {
            self.vertices.copy_from_slice(before);
            return false;
        }
        self.vertices.iter().zip(before).any(|(a, b)| a != b)
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;

    #[test]
    fn quad_brush_rejection_pins_a_sliver_without_cancelling_a_valid_patch() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3(0., 0., 0.),
                Vec3(1., 0., 0.),
                Vec3(1., 0.004, 0.),
                Vec3(0., 1., 0.),
                Vec3(3., 0., 0.),
                Vec3(4., 0., 0.),
                Vec3(4., 1., 0.),
                Vec3(3., 1., 0.),
            ],
            quads: vec![[0, 1, 2, 3], [4, 5, 6, 7]],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            ..Default::default()
        };
        let before = mesh.vertices.clone();
        mesh.vertices[2].1 = 0.001;
        for p in &mut mesh.vertices[4..] {
            p.2 += 0.1;
        }
        let proposed = mesh.vertices.clone();
        let mut observed = proposed.clone();
        assert!(
            mesh.limit_brush_deformation(&before, None, &mut |v, old, new| {
                assert_eq!(observed[v], old);
                observed[v] = new;
            })
        );
        assert_eq!(&mesh.vertices[..4], &before[..4]);
        assert_eq!(&mesh.vertices[4..], &proposed[4..]);
        assert_eq!(observed, mesh.vertices);
        for p in mesh.polygons() {
            for (t, _) in p.symmetric_triangles() {
                assert!(safe_face(
                    t.map(|v| before[v]),
                    t.map(|v| mesh.vertices[v]),
                    None
                ));
            }
        }
    }

    #[test]
    fn quad_guard_ignores_roundoff_area_at_collinear_boundary_corners() {
        for scale in [0.01, 1., 100.] {
            for reflected in [false, true] {
                for alternate in [false, true] {
                    let mut mesh = Mesh {
                        vertices: vec![
                            Vec3(0., 1e-7, 0.),
                            Vec3(1., 0., 0.),
                            Vec3(0., 1., 0.),
                            Vec3(-1., 0., 0.),
                        ],
                        quads: vec![[0, 1, 2, 3]],
                        triangles: if alternate {
                            vec![[0, 1, 3], [1, 2, 3]]
                        } else {
                            vec![[0, 1, 2], [0, 2, 3]]
                        },
                        ..Default::default()
                    };
                    for p in &mut mesh.vertices {
                        *p = *p * scale;
                        if reflected {
                            p.0 = -p.0;
                        }
                    }
                    let before = mesh.vertices.clone();
                    mesh.vertices[0].1 = -0.01 * scale;
                    let proposed = mesh.vertices.clone();
                    assert!(mesh.limit_brush_deformation(&before, None, &mut |_, _, _| {}));
                    assert_eq!(mesh.vertices, proposed);
                    // Both nondegenerate rendering triangles remain valid.
                    for t in [[0, 1, 2], [0, 2, 3]] {
                        assert!(safe_face(
                            t.map(|v| before[v]),
                            t.map(|v| mesh.vertices[v]),
                            None
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn quad_deformation_reductions_do_not_depend_on_the_render_diagonal() {
        let mut mesh = Mesh {
            vertices: vec![
                Vec3(-1., -1., 0.),
                Vec3(1., -1., 0.),
                Vec3(1., 1., 0.),
                Vec3(-1., 1., 0.),
            ],
            mask: vec![0.; 4],
            quads: vec![[0, 1, 2, 3]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..Default::default()
        };
        let before = mesh.vertices.clone();
        mesh.vertices[0].0 = -0.8;
        mesh.vertices[1].0 = 0.8;
        mesh.vertices[2].0 = 0.001;
        mesh.vertices[3].0 = -0.001;
        assert!(mesh.limit_brush_deformation(&before, None, &mut |_, _, _| {}));
        for [a, b] in [[0, 1], [3, 2]] {
            let p = mesh.vertices[a];
            assert!(
                (mesh.vertices[b] - Vec3(-p.0, p.1, p.2)).len() < 1e-6,
                "a symmetric proposal must receive matching guard reductions"
            );
        }
        for t in &mesh.triangles {
            assert!(safe_face(
                t.map(|i| before[i]),
                t.map(|i| mesh.vertices[i]),
                None
            ));
        }
    }

    #[test]
    fn smoothing_does_not_make_existing_slivers_worse() {
        let before = vec![Vec3(0., 0., 0.), Vec3(1., 0., 0.), Vec3(0.5, 0.004, 0.)];
        let mut mesh = Mesh {
            vertices: before.clone(),
            triangles: vec![[0, 1, 2]],
            mask: vec![0.; 3],
            ..Default::default()
        };
        mesh.vertices[2].1 = 0.001;
        mesh.limit_smoothing_deformation(&before);
        let q = |v: &[Vec3]| quality(difference(v[1], v[0]), difference(v[2], v[0]));
        assert!(q(&mesh.vertices) >= q(&before) * (1. - 1e-6));
    }

    #[test]
    fn smoothing_allows_valid_steep_dihedrals_without_pinning_a_safe_patch() {
        let before = vec![
            Vec3(0., -1., 0.),
            Vec3(0., 1., 0.),
            Vec3(-1., 0., 1.),
            Vec3(1., 0., 1.),
            Vec3(3., 0., 0.),
            Vec3(4., 0., 0.),
            Vec3(3., 1., 0.),
        ];
        let mut mesh = Mesh {
            vertices: before.clone(),
            triangles: vec![[0, 1, 2], [1, 0, 3], [4, 5, 6]],
            mask: vec![0.; 7],
            ..Default::default()
        };
        mesh.vertices[2].0 = -0.5;
        mesh.vertices[3].0 = 0.5;
        for p in &mut mesh.vertices[4..] {
            p.2 += 0.1;
        }
        let safe = mesh.vertices[4..].to_vec();
        assert!(mesh.limit_smoothing_deformation(&before));
        assert_eq!(&mesh.vertices[4..], safe);
        // A sharper V has opposing normals but no triangle inversion or
        // intersection. A dihedral sign test must not lock fairing in place.
        assert_eq!(mesh.vertices[2].0, -0.5);
        assert_eq!(mesh.vertices[3].0, 0.5);
        for t in &mesh.triangles {
            assert!(safe_face(
                t.map(|i| before[i]),
                t.map(|i| mesh.vertices[i]),
                None
            ));
        }
    }

    #[test]
    fn directional_stamp_allows_steep_relief_but_rejects_projected_collapse() {
        for scale in [0.001, 1., 1000.] {
            let before = [Vec3(0., 0., 0.), Vec3(1., 0., 1.), Vec3(0., 1., 0.)].map(|p| p * scale);
            let mut after = before;
            after[1].2 = -3. * scale;
            let direction = Some(Vec3(0., 0., 1.));
            assert!(!safe_face(before, after, None));
            assert!(safe_face(before, after, direction));
            // A tangent/back-facing projection cannot override the original guard.
            assert!(!safe_face(before, after, Some(Vec3(0., 1., 0.))));
            assert!(!safe_face(before, after, Some(Vec3(0., 0., -1.))));
            // Correct winding at release does not excuse crossing zero area
            // midway through the motion, even with a stamp projection.
            after[1] = before[1] * -1.;
            after[2] = before[2] * -1.;
            assert!(!safe_face(before, after, direction));
            after = before;
            after[1] = before[0];
            assert!(!safe_face(before, after, direction));
            // Unchanged projected area still needs the triangle quality check.
            after = before;
            after[1].2 = 1000. * scale;
            assert!(!safe_face(before, after, direction));
        }
    }

    #[test]
    fn projection_fold_guard_is_local_and_preserves_existing_sharp_edges() {
        for scale in [0.01, 1., 100.] {
            let before = [
                Vec3(0., -1., 0.),
                Vec3(0., 1., 0.),
                Vec3(-1., 0., 0.),
                Vec3(1., 0., 0.),
                Vec3(3., 0., 0.),
                Vec3(4., 0., 0.),
                Vec3(3., 1., 0.),
            ]
            .map(|p| p * scale);
            let mut mesh = Mesh {
                vertices: before.into(),
                triangles: vec![[0, 1, 2], [1, 0, 3], [4, 5, 6]],
                mask: vec![0.; 7],
                ..Mesh::default()
            };
            mesh.vertices[2] = Vec3(-0.25, 0., 1.) * scale;
            mesh.vertices[3] = Vec3(0.25, 0., 1.) * scale;
            for v in &mut mesh.vertices[4..] {
                v.2 += scale;
            }
            assert!(
                mesh.triangles.iter().all(|&t| safe_face(
                    t.map(|v| before[v]),
                    t.map(|v| mesh.vertices[v]),
                    None
                )),
                "A face-only check misses the fold"
            );
            let independent_patch = mesh.vertices[4..].to_vec();
            assert!(mesh.limit_projection_deformation(&before));
            assert!(
                normal_agreement(
                    face_normal(mesh.triangles[0].map(|v| mesh.vertices[v])),
                    face_normal(mesh.triangles[1].map(|v| mesh.vertices[v])),
                ) >= 0.
            );
            assert_eq!(&mesh.vertices[4..], independent_patch);

            // An already sharp edge is allowed to translate without flattening.
            mesh.vertices[2] = Vec3(-0.25, 0., 1.) * scale;
            mesh.vertices[3] = Vec3(0.25, 0., 1.) * scale;
            let sharp = mesh.vertices.clone();
            for v in &mut mesh.vertices {
                v.2 += scale;
            }
            let translated = mesh.vertices.clone();
            assert!(mesh.limit_projection_deformation(&sharp));
            assert_eq!(mesh.vertices, translated);
        }
    }

    #[test]
    fn oversized_snake_pull_cannot_fold_the_surface() {
        use crate::sculpt::{Brush, Dab, Kind};
        let before = Mesh::sphere();
        let brush = Brush::new(Kind::Snake);
        let dab = Dab {
            center: Vec3(0., 1., 0.),
            normal: Vec3(0., 1., 0.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 0., 1.),
            radius: 0.3,
            pressure: 1.,
            delta: Vec3(0.2, 0., 0.),
            invert: false,
        };
        let proposed: Vec<_> = before
            .vertices
            .iter()
            .map(|&p| p + brush.snake_motion(dab) * brush.snake_weight(dab, p - dab.center))
            .collect();
        assert!(
            before.triangles.iter().any(|&t| {
                let [a, b, c] = t.map(|v| before.vertices[v]);
                let [x, y, z] = t.map(|v| proposed[v]);
                (b - a).cross(c - a).dot((y - x).cross(z - x)) <= 0.
            }),
            "The unrestricted pull reproduces a folded face"
        );
        let mut guarded = before.clone();
        guarded.stamp(&brush, dab);
        assert!(before.triangles.iter().all(|&t| safe_face(
            t.map(|v| before.vertices[v]),
            t.map(|v| guarded.vertices[v]),
            None,
        )));
        guarded.validate_closed().unwrap();
    }

    #[test]
    fn unsafe_motion_is_reduced_locally_and_cannot_cross_a_collapsed_face() {
        for scale in [0.001, 1., 1000.] {
            let mut before: Vec<_> = [
                Vec3(0., 0., 0.),
                Vec3(1., 0., 0.),
                Vec3(0., 1., 0.),
                Vec3(3., 0., 0.),
                Vec3(4., 0., 0.),
                Vec3(3., 1., 0.),
            ]
            .map(|p| (p + Vec3(100., 0., 0.)) * scale)
            .into();
            // The reduction also visits a stationary corner. Its arithmetic
            // can change a signed zero, which a position journal must observe.
            before[0].2 = -0.0;
            let mut mesh = Mesh {
                vertices: before.clone(),
                triangles: vec![[0, 1, 2], [3, 4, 5]],
                mask: vec![0.; 6],
                quads: Vec::new(),
                loose_edges: Vec::new(),
                loose_vertices: Vec::new(),
                uv: Default::default(),
                creases: Vec::new(),
                shading_seams: None,
                materials: Vec::new(),
                vertex_groups: Vec::new(),
                polygroups: Vec::new(),
                hidden_faces: Vec::new(),
            };
            // The endpoint is correctly wound, but reaching it crosses zero area.
            mesh.vertices[1] = before[0] - (before[1] - before[0]);
            mesh.vertices[2] = before[0] - (before[2] - before[0]);
            for (v, &original) in before.iter().enumerate().skip(3) {
                mesh.vertices[v] = original + Vec3(0., 0., scale);
            }
            let safe_patch = mesh.vertices[3..].to_vec();
            let mut saw_corner_write = false;
            assert!(
                mesh.limit_brush_deformation(&before, None, &mut |i, old, new| {
                    if i == 0 && old.2.to_bits() == (-0.0_f32).to_bits() && new.2.to_bits() == 0 {
                        saw_corner_write = true;
                    }
                })
            );
            assert!(saw_corner_write);
            assert_eq!(&mesh.vertices[3..], &safe_patch);
            assert!(mesh.vertices[1].0 > before[0].0);
            assert!(mesh.vertices[2].1 > before[0].1);
            assert!(
                mesh.vertices[1].0 < before[1].0,
                "Keep useful movement instead of cancelling the whole dab"
            );
            assert_eq!(mesh.mask, vec![0.; 6]);
            mesh.validate().unwrap();
        }
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod scan_tests;
