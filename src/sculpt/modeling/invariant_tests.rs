use super::*;
use crate::sculpt::{Primitive, PrimitiveKind, Symmetry};

use crate::sculpt::test_support::{fan, shells, surface_invariants};

#[test]
fn modeling_fan_gate_rejects_closed_and_open_pinched_welds() {
    for closed in [false, true] {
        let mesh = shells(closed);
        let other = if closed { 4 } else { 3 };
        assert!(surface_invariants(&mesh));
        manifold(&mesh).unwrap();
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let error = mesh
            .edit_model_topology(TopologyEdit::Weld, &[], &[], &[0, other], &symmetry)
            .unwrap_err();
        assert!(error.contains("connected surface fan"), "{error}");

        // Demonstrate that the independent oracle detects the old accepted result.
        let mut pinched = mesh.clone();
        pinched.vertices[0] = Vec3::default();
        for t in &mut pinched.triangles {
            *t = t.map(|v| if v == other { 0 } else { v });
        }
        assert!(!surface_invariants(&pinched));
        assert!(
            manifold(&pinched)
                .unwrap_err()
                .contains("connected surface fan")
        );
    }
}

#[test]
fn modeling_fan_gate_keeps_open_boundary_welds_and_explicit_wire_branches() {
    let mesh = Mesh {
        vertices: vec![
            Vec3(-1., -1., 0.),
            Vec3(1., -1., 0.),
            Vec3(1., 1., 0.),
            Vec3(-1., 1., 0.),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        mask: vec![0.; 4],
        ..Default::default()
    };
    let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
    let result = mesh
        .edit_model_topology(TopologyEdit::Weld, &[], &[], &[0, 1], &symmetry)
        .unwrap();
    assert_eq!(result.triangles.len(), 1);
    assert!(surface_invariants(&result));
    manifold(&result).unwrap();

    let mut mixed = mesh;
    mixed.vertices.extend([
        Vec3(2., 2., 0.),
        Vec3(3., 2., 0.),
        Vec3(2., 3., 0.),
        Vec3(4., 4., 0.),
    ]);
    mixed.mask.resize(8, 0.);
    mixed.loose_edges = vec![[2, 4], [4, 5], [4, 6]];
    mixed.loose_vertices = vec![7];
    manifold(&mixed).unwrap();
    mixed.triangles.clear();
    mixed.loose_vertices = vec![0, 1, 3, 7];
    manifold(&mixed).unwrap();
}

#[test]
fn modeling_bounded_edit_sequences_preserve_independent_surface_invariants() {
    let mut grid = Primitive::for_kind(PrimitiveKind::Grid);
    grid.divisions = [3, 3, 1];
    let sources = [
        Primitive::for_kind(PrimitiveKind::Cube).mesh().unwrap(),
        grid.mesh().unwrap(),
        fan(32),
    ];
    // 36 tiny cases, six edits each: closed/open, ordinary/high valence,
    // reversed face order, three scales and exact mirror correspondence.
    for (fixture, source) in sources.into_iter().enumerate() {
        for scale in [0.01, 1., 100.] {
            for reverse in [false, true] {
                for mirrored in [false, true] {
                    let mut mesh = source.clone();
                    for p in &mut mesh.vertices {
                        *p = *p * scale;
                    }
                    if reverse {
                        mesh.triangles.reverse();
                        mesh.quads.reverse();
                    }
                    mesh.mask.fill(0.25);
                    mesh.create_vertex_group("Sequence".into(), 0., true)
                        .unwrap();
                    let settings = Symmetry {
                        enabled: mirrored,
                        ..Default::default()
                    };
                    let mut seed = 31 + fixture;
                    let mut accepted = 0;
                    for step in 0..6 {
                        mesh.validate().unwrap();
                        assert!(surface_invariants(&mesh));
                        let symmetry = ModelingSymmetry::new(&mesh, &settings).unwrap();
                        let edges = mesh.polygon_edges();
                        seed = (seed * 17 + 11) % 997;
                        let edge = edges[seed % edges.len()];
                        let edit = if step % 2 == 0 {
                            TopologyEdit::Split
                        } else {
                            TopologyEdit::Weld
                        };
                        if let Ok(next) =
                            mesh.edit_model_topology(edit, &[], &[edge], &edge, &symmetry)
                        {
                            next.validate().unwrap();
                            assert!(
                                surface_invariants(&next),
                                "fixture={fixture}, scale={scale}, reverse={reverse}, mirrored={mirrored}, step={step}"
                            );
                            assert!(next.mask.iter().all(|&m| m == 0.25));
                            assert!(next.vertex_groups[0].weights.iter().all(|&w| w == 1.));
                            let restored: Mesh =
                                serde_json::from_slice(&serde_json::to_vec(&next).unwrap())
                                    .unwrap();
                            assert_eq!(restored, next);
                            assert!(surface_invariants(&restored));
                            mesh = next;
                            accepted += 1;
                        }
                    }
                    assert!(
                        accepted >= 1,
                        "each generated case must exercise a successful edit"
                    );
                }
            }
        }
    }
}
