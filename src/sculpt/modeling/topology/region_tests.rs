use super::*;
use crate::sculpt::{Material, Symmetry, VertexGroup};

fn strips() -> Mesh {
    let mut mesh = Mesh::default();
    for x in [-2., 1.] {
        let start = mesh.vertices.len();
        for y in [-1., 0., 1.] {
            mesh.vertices.extend([Vec3(x, y, 0.), Vec3(x + 1., y, 0.)]);
        }
        for row in 0..2 {
            let a = start + row * 2;
            mesh.push_polygon(&[a, a + 1, a + 3, a + 2], 0, false);
        }
    }
    mesh.mask = vec![0.; mesh.vertices.len()];
    mesh
}
fn fan() -> Mesh {
    let mut mesh = Mesh {
        vertices: vec![
            Vec3(-1., -1., 0.),
            Vec3(1., -1., 0.),
            Vec3(1., 1., 0.),
            Vec3(-1., 1., 0.),
            Vec3(0., 0., 0.),
        ],
        mask: vec![0.; 5],
        ..Default::default()
    };
    for i in 0..4 {
        mesh.push_polygon(&[i, (i + 1) % 4, 4], 0, false);
    }
    mesh
}
fn mapped(mesh: &mut Mesh) {
    mesh.uv.topology = mesh.triangles.clone();
    mesh.uv.corners = mesh
        .triangles
        .iter()
        .map(|t| {
            t.map(|v| {
                [
                    mesh.vertices[v].0 * 0.1 + 0.5,
                    mesh.vertices[v].1 * 0.1 + 0.5,
                ]
            })
        })
        .collect();
    mesh.uv.masks = vec![[0.25; 3]; mesh.triangles.len()];
}
fn sym(mesh: &Mesh) -> ModelingSymmetry {
    ModelingSymmetry::new(mesh, &Symmetry::default()).unwrap()
}
fn dissolve(mesh: &Mesh, faces: &[usize]) -> Result<Mesh, String> {
    mesh.edit_model_topology(TopologyEdit::DissolveFaces, faces, &[], &[], &sym(mesh))
}
const CHAINS: [[usize; 2]; 4] = [[1, 3], [3, 5], [6, 8], [8, 10]];

#[test]
fn topology_regions_bridge_segments_preserve_source_and_interpolate_attributes() {
    let mut mesh = strips();
    mapped(&mut mesh);
    mesh.mask[6] = 1.;
    mesh.creases = vec![[0, 1]];
    mesh.set_shading_seams(vec![[0, 1]]);
    let before = mesh.clone();
    for count in [1, 2, 16] {
        let out = mesh
            .bridge_model_chains(&CHAINS, count, &sym(&mesh))
            .unwrap();
        assert_eq!(&out.vertices[..mesh.vertices.len()], &mesh.vertices);
        assert_eq!(&out.triangles[..mesh.triangles.len()], &mesh.triangles);
        assert_eq!(out.quads.len(), mesh.quads.len() + 2 * count as usize);
        assert_eq!(
            out.vertices.len(),
            mesh.vertices.len() + 3 * (count as usize - 1)
        );
        assert_eq!(&out.uv.corners[..mesh.triangles.len()], &mesh.uv.corners);
        assert!(out.uv.current(&out));
        assert!(out.uv.removed[mesh.triangles.len()..].iter().all(|v| *v));
        assert_eq!(out.creases, mesh.creases);
        assert_eq!(out.shading_edges(), mesh.shading_edges());
        if count > 1 {
            assert!((out.mask[12] - 1. / count as f32).abs() < 1e-6);
        }
        assert_eq!(crate::sculpt::fusion::boundaries(&out).unwrap().len(), 1);
    }
    assert_eq!(mesh, before);
}

#[test]
fn topology_regions_bridge_rejects_wrong_counts_loops_stale_protection_and_obstructions() {
    let mesh = strips();
    for edges in [
        &CHAINS[..1],
        &CHAINS[..3],
        &[[0, 1], [1, 3], [2, 3], [0, 2]],
        &[[1, 99], [6, 8]],
        &[[2, 3], [6, 8]],
    ] {
        assert!(mesh.bridge_model_chains(edges, 1, &sym(&mesh)).is_err());
    }
    for count in [0, 17, u32::MAX] {
        assert!(
            mesh.bridge_model_chains(&CHAINS, count, &sym(&mesh))
                .is_err()
        );
    }
    let mut hidden = mesh.clone();
    hidden.hidden_faces[0] = true;
    assert!(
        hidden
            .bridge_model_chains(&CHAINS, 1, &sym(&hidden))
            .is_err()
    );
    let mut blocked = mesh.clone();
    blocked
        .vertices
        .extend([Vec3(0., -0.5, -1.), Vec3(0., 0.5, -1.), Vec3(0., 0., 1.)]);
    blocked.mask.resize(15, 0.);
    blocked.push_polygon(&[12, 13, 14], 0, false);
    assert!(
        blocked
            .bridge_model_chains(&CHAINS, 2, &sym(&blocked))
            .unwrap_err()
            .contains("crosses")
    );
    let mut twisted = mesh.clone();
    for p in &mut twisted.vertices[6..] {
        p.1 = -p.1;
    }
    assert!(
        twisted
            .bridge_model_chains(&CHAINS, 1, &sym(&twisted))
            .is_err()
    );
}

#[test]
fn topology_regions_bridge_and_dissolve_respect_exact_symmetry() {
    let settings = Symmetry {
        enabled: true,
        mirror: [true, false, false],
        ..Default::default()
    };
    let mesh = strips();
    let maps = ModelingSymmetry::new(&mesh, &settings).unwrap();
    let out = mesh.bridge_model_chains(&CHAINS[..2], 3, &maps).unwrap();
    maps.validate_result(&out).unwrap();
    let mesh = fan();
    let maps = ModelingSymmetry::new(&mesh, &settings).unwrap();
    let out = mesh
        .edit_model_topology(TopologyEdit::DissolveFaces, &[0, 1, 2, 3], &[], &[], &maps)
        .unwrap();
    assert_eq!(out.quads.len(), 1);
    maps.validate_result(&out).unwrap();
}

#[test]
fn topology_regions_flip_whole_shell_preserves_uv_corner_identity_and_is_reversible() {
    let mut mesh = fan();
    mapped(&mut mesh);
    mesh.creases = vec![[0, 1]];
    mesh.set_shading_seams(vec![[0, 1]]);
    let flip = |mesh: &Mesh, faces: &[usize]| {
        mesh.edit_model_topology(TopologyEdit::FlipFaces, faces, &[], &[], &sym(mesh))
    };
    assert!(
        flip(&mesh, &[0])
            .unwrap_err()
            .contains("complete connected")
    );
    let out = flip(&mesh, &[0, 1, 2, 3]).unwrap();
    assert_eq!(out.vertices, mesh.vertices);
    assert!(out.uv.current(&out));
    for (f, t) in out.triangles.iter().enumerate() {
        for (i, &v) in t.iter().enumerate() {
            let old = mesh.triangles[f].iter().position(|&w| w == v).unwrap();
            assert_eq!(out.uv.corners[f][i], mesh.uv.corners[f][old]);
        }
    }
    assert_eq!(flip(&out, &[0, 1, 2, 3]).unwrap(), mesh);
    let mut quad = dissolve(&fan(), &[0, 1, 2, 3]).unwrap();
    mapped(&mut quad);
    assert_eq!(flip(&flip(&quad, &[0]).unwrap(), &[0]).unwrap(), quad);
}

#[test]
fn topology_regions_dissolve_removes_planar_interior_and_preserves_unrelated_attributes() {
    let mut mesh = fan();
    mesh.vertices
        .extend([Vec3(3., 0., 0.), Vec3(4., 0., 0.), Vec3(3., 1., 0.)]);
    mesh.mask.resize(8, 0.);
    mesh.push_polygon(&[5, 6, 7], 0, false);
    mesh.materials = vec![Material::default(); 8];
    mesh.materials[5] = Material::from_srgb([1., 0., 0.], 0.2, 0.4);
    mapped(&mut mesh);
    mesh.creases = vec![[0, 1], [5, 6]];
    mesh.uv.seams = mesh.creases.clone();
    mesh.set_shading_seams(mesh.creases.clone());
    let original = mesh.clone();
    let out = dissolve(&mesh, &[0, 1, 2, 3]).unwrap();
    assert_eq!(out.vertices.len(), 7);
    assert_eq!(out.quads, vec![[0, 1, 2, 3]]);
    assert_eq!(out.triangles[0], [4, 5, 6]);
    assert_eq!(out.materials[4], mesh.materials[5]);
    assert_eq!(out.uv.corners[0], mesh.uv.corners[4]);
    assert_eq!(out.shading_edges(), &[[0, 1], [4, 5]]);
    assert_eq!(out.uv.seams, vec![[0, 1], [4, 5]]);
    assert!(out.uv.current(&out));
    assert_eq!(mesh, original);
    let reopened: Mesh = serde_json::from_slice(&serde_json::to_vec(&out).unwrap()).unwrap();
    assert_eq!(reopened, out);
    reopened.validate().unwrap();
}

#[test]
fn topology_regions_dissolve_rejects_detail_seams_groups_hidden_and_nonplanarity() {
    for kind in 0..9 {
        let mut mesh = fan();
        mapped(&mut mesh);
        match kind {
            0 => mesh.vertices[4].2 = 0.01,
            1 => mesh.creases = vec![[0, 4]],
            2 => mesh.uv.seams = vec![[0, 4]],
            3 => mesh.set_shading_seams(vec![[0, 4]]),
            4 => mesh.mask[4] = 0.5,
            5 => {
                mesh.materials = vec![Material::default(); 5];
                mesh.materials[4].roughness = 0.2;
            }
            6 => {
                mesh.vertex_groups.push(VertexGroup {
                    purpose: Default::default(),
                    visible: true,
                    name: "Patch".into(),
                    weights: vec![1.; 5],
                });
                mesh.polygroups[0] = 1;
            }
            7 => mesh.hidden_faces[0] = true,
            8 => mesh.uv.corners[0][0][0] += 0.1,
            _ => unreachable!(),
        }
        let before = mesh.clone();
        assert!(dissolve(&mesh, &[0, 1, 2, 3]).is_err(), "kind {kind}");
        assert_eq!(mesh, before);
    }
    let mut mesh = fan();
    mapped(&mut mesh);
    for (f, t) in mesh.triangles.iter().enumerate() {
        for (i, &v) in t.iter().enumerate() {
            if v == 4 {
                mesh.uv.corners[f][i][0] += 0.1;
            }
        }
    }
    assert!(
        dissolve(&mesh, &[0, 1, 2, 3])
            .unwrap_err()
            .contains("distort")
    );
    assert!(dissolve(&mesh, &[usize::MAX]).is_err());
    assert!(dissolve(&fan(), &[0]).is_err());
}

#[test]
fn topology_regions_dissolve_tessellates_concavity_and_rejects_holes() {
    let mut mesh = Mesh {
        vertices: vec![
            Vec3(0., 0., 0.),
            Vec3(2., 0., 0.),
            Vec3(2., 1., 0.),
            Vec3(1., 1., 0.),
            Vec3(1., 2., 0.),
            Vec3(0., 2., 0.),
        ],
        mask: vec![0.; 6],
        triangles: vec![[0, 1, 3], [1, 2, 3], [0, 3, 5], [3, 4, 5]],
        ..Default::default()
    };
    mapped(&mut mesh);
    let out = dissolve(&mesh, &[0, 1, 2, 3]).unwrap();
    assert_eq!(out.vertices, mesh.vertices);
    assert!(out.quads.is_empty());
    assert_eq!(out.triangles.len(), 4);
    assert!(out.uv.current(&out));
    assert_eq!(
        crate::sculpt::fusion::boundaries(&out).unwrap()[0]
            .vertices
            .len(),
        6
    );
    let mut ring = Mesh::default();
    for y in 0..4 {
        for x in 0..4 {
            ring.vertices.push(Vec3(x as f32, y as f32, 0.));
        }
    }
    ring.mask = vec![0.; 16];
    for y in 0..3 {
        for x in 0..3 {
            if x == 1 && y == 1 {
                continue;
            }
            let a = y * 4 + x;
            ring.push_polygon(&[a, a + 1, a + 5, a + 4], 0, false);
        }
    }
    assert!(
        dissolve(&ring, &(0..8).collect::<Vec<_>>())
            .unwrap_err()
            .contains("holes")
    );
}

#[test]
fn topology_regions_dissolve_protects_internal_quad_seams_and_mapped_boundaries() {
    let mut mesh = strips();
    mapped(&mut mesh);
    mesh.uv.seams = vec![[0, 3]]; // A quad's triangulation diagonal.
    assert!(dissolve(&mesh, &[0, 1]).unwrap_err().contains("seam"));
    mesh.uv.seams.clear();
    mesh.uv.removed = vec![false; mesh.triangles.len()];
    mesh.uv.removed[0] = true;
    assert!(
        dissolve(&mesh, &[0, 1])
            .unwrap_err()
            .contains("mapped and unmapped")
    );
    mesh.uv.removed.fill(true);
    // Independent complete patches are processed together, preserving their separation.
    let a = fan();
    let mut b = a.clone();
    for p in &mut b.vertices {
        p.0 += 4.;
    }
    let combined = a.merged_with(&b).unwrap();
    let out = dissolve(&combined, &(0..8).collect::<Vec<_>>()).unwrap();
    assert_eq!(out.quads.len(), 2);
    assert_eq!(crate::sculpt::fusion::boundaries(&out).unwrap().len(), 2);
}
