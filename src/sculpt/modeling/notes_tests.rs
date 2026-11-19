use super::*;
use crate::sculpt::{Primitive, PrimitiveKind, Symmetry};

fn grid() -> Mesh {
    let mut primitive = Primitive::for_kind(PrimitiveKind::Grid);
    primitive.divisions = [5, 5, 1];
    primitive.mesh().unwrap()
}

#[test]
fn modeling_notes_transforms_and_slides_leave_protected_vertices_fixed() {
    let mut mesh = grid();
    mesh.mask[14] = 1.;
    mesh.hidden_faces = vec![false; mesh.triangles.len()];
    mesh.hidden_faces[0] = true;
    mesh.hidden_faces[1] = true;
    let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
    let selected = (0..mesh.vertices.len()).collect::<Vec<_>>();
    let moved = mesh
        .transform_components(
            &selected,
            Vec3::default(),
            ComponentTransform::Move(Vec3(0.02, 0., 0.)),
            true,
            &symmetry,
        )
        .unwrap();
    let blocked = mesh.protected_model_vertices(true);
    for v in 0..mesh.vertices.len() {
        assert_eq!(moved.vertices[v] == mesh.vertices[v], blocked.contains(&v));
    }
    mesh.check_model_protection(&moved, true).unwrap();
    let corners = mesh.quads[12].to_vec();
    let slide = mesh
        .slide_components(
            &corners,
            Vec3::default(),
            Vec3(0.05, 0., 0.03),
            true,
            &symmetry,
        )
        .unwrap();
    assert_eq!(slide.vertices[14], mesh.vertices[14]);
    assert_ne!(slide.vertices[15], mesh.vertices[15]);
    assert!(slide.vertices.iter().all(|v| v.2.abs() < 1e-6));
    assert_eq!(slide.triangles, mesh.triangles);
}

#[test]
fn modeling_notes_direct_delete_keeps_or_removes_unused_vertices() {
    let mesh = Primitive::for_kind(PrimitiveKind::Plane).mesh().unwrap();
    let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
    for keep in [false, true] {
        for (faces, edges) in [(vec![0], vec![]), (vec![], vec![mesh.polygon_edges()[0]])] {
            let out = mesh
                .delete_model_components(&faces, &edges, &[], keep, true, &symmetry)
                .unwrap();
            assert!(out.triangles.is_empty());
            assert_eq!(out.vertices.len(), if keep { 4 } else { 0 });
            assert_eq!(out.loose_vertices.len(), if keep { 4 } else { 0 });
            out.validate().unwrap();
        }
    }
    let mut protected = mesh.clone();
    protected.mask[0] = 1.;
    let out = protected
        .delete_model_components(&[0], &[], &[], false, true, &symmetry)
        .unwrap();
    assert_eq!(out, protected);
}

#[test]
fn modeling_notes_loop_cut_stops_before_masked_and_hidden_geometry() {
    for hidden in [false, true] {
        let mut mesh = grid();
        if hidden {
            mesh.hidden_faces = vec![false; mesh.triangles.len()];
            for (i, t) in mesh.triangles.iter().enumerate() {
                mesh.hidden_faces[i] = t.iter().any(|&v| mesh.vertices[v].1 > 0.8);
            }
        } else {
            for (v, m) in mesh.vertices.iter().zip(&mut mesh.mask) {
                if v.1 > 0.8 {
                    *m = 1.;
                }
            }
        }
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        for count in [1, 3] {
            let out = mesh
                .insert_protected_edge_loops([1, 2], count, 0.5, true, &symmetry)
                .unwrap()
                .mesh;
            assert!(out.vertices.len() > mesh.vertices.len());
            mesh.check_model_protection(&out, true).unwrap();
            manifold(&out).unwrap();
            // Every internal triangle edge is shared, including where the cut ends.
            let mut incidence = BTreeMap::<_, usize>::new();
            for t in &out.triangles {
                for i in 0..3 {
                    *incidence.entry(edge(t[i], t[(i + 1) % 3])).or_default() += 1;
                }
            }
            for ([a, b], count) in incidence {
                if count == 1 {
                    let a = out.vertices[a];
                    let b = out.vertices[b];
                    assert!((a.0.abs() == 1. && b.0 == a.0) || (a.1.abs() == 1. && b.1 == a.1));
                }
            }
        }
    }
}

#[test]
fn modeling_notes_loop_removal_stops_at_protection_without_cracks() {
    for hidden in [false, true] {
        for gap in [false, true] {
            let mut mesh = grid();
            if hidden {
                mesh.hidden_faces = mesh
                    .triangles
                    .iter()
                    .map(|t| t.iter().any(|&v| mesh.vertices[v].1 > 0.8))
                    .collect();
            } else {
                for (p, m) in mesh.vertices.iter().zip(&mut mesh.mask) {
                    if p.1 > 0.8 {
                        *m = 1.;
                    }
                }
            }
            let edges: Vec<_> = mesh
                .polygon_edges()
                .into_iter()
                .filter(|e| e.iter().all(|&v| (mesh.vertices[v].0 - 0.2).abs() < 1e-5))
                .collect();
            let sym = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
            let out = mesh
                .remove_protected_loops(&edges, gap, true, &sym)
                .unwrap();
            assert_ne!(out, mesh);
            mesh.check_model_protection(&out, true).unwrap();
            manifold(&out).unwrap();
            if !gap {
                let mut incidence = BTreeMap::<_, usize>::new();
                for t in &out.triangles {
                    for i in 0..3 {
                        *incidence.entry(edge(t[i], t[(i + 1) % 3])).or_default() += 1;
                    }
                }
                for ([a, b], count) in incidence {
                    if count != 1 {
                        continue;
                    }
                    let a = out.vertices[a];
                    let b = out.vertices[b];
                    assert!((a.0.abs() == 1. && b.0 == a.0) || (a.1.abs() == 1. && b.1 == a.1));
                }
            }
        }
    }
}

#[test]
fn modeling_notes_asymmetric_protection_skips_whole_symmetry_orbits() {
    let mut mesh = grid();
    mesh.mask[14] = 1.;
    let sym = ModelingSymmetry::new(
        &mesh,
        &Symmetry {
            enabled: true,
            ..Default::default()
        },
    )
    .unwrap();
    let selected: Vec<_> = (0..mesh.vertices.len()).collect();
    let out = mesh
        .transform_components(
            &selected,
            Vec3::default(),
            ComponentTransform::Move(Vec3(0., 0., 0.02)),
            true,
            &sym,
        )
        .unwrap();
    for v in sym.vertex_copies(14) {
        assert_eq!(out.vertices[v], mesh.vertices[v]);
    }
    assert_ne!(out, mesh);
    let faces: Vec<_> = (0..mesh.polygons().len()).collect();
    let out = mesh
        .delete_model_components(&faces, &[], &[], false, true, &sym)
        .unwrap();
    assert_ne!(out, mesh);
    mesh.check_model_protection(&out, true).unwrap();
}

#[test]
fn modeling_notes_loop_slide_keeps_protected_vertices_fixed() {
    for hidden in [false, true] {
        let mut mesh = grid();
        let edges: Vec<_> = mesh
            .polygon_edges()
            .into_iter()
            .filter(|e| e.iter().all(|&v| (mesh.vertices[v].0 - 0.2).abs() < 1e-5))
            .collect();
        if hidden {
            mesh.hidden_faces = mesh
                .triangles
                .iter()
                .map(|t| t.iter().any(|&v| mesh.vertices[v].1 > 0.8))
                .collect();
        } else {
            mesh.mask[edges[0][0]] = 1.;
        }
        let symmetry = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let out = mesh
            .slide_model_edges(
                &edges,
                0.2,
                Vec3(1., 0., 0.),
                Vec3::default(),
                true,
                &symmetry,
            )
            .unwrap();
        assert_ne!(out.vertices, mesh.vertices);
        assert_eq!(out.triangles, mesh.triangles);
        mesh.check_model_protection(&out, true).unwrap();
    }
}
