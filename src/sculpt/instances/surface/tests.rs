use super::*;
fn guide() -> Mesh {
    let mut m = Mesh::default();
    m.vertices = vec![
        Vec3(-2., -1., 0.),
        Vec3(2., -1., 0.),
        Vec3(2., 1., 0.),
        Vec3(-2., 1., 0.),
        Vec3(4., 0., 0.),
        Vec3(5., 0., 0.),
        Vec3(4., 1., 0.),
    ];
    m.triangles = vec![[0, 1, 2], [0, 2, 3], [4, 5, 6]];
    m.quads = vec![[0, 1, 2, 3]];
    m.mask = vec![0.; 7];
    m
}
fn part() -> Mesh {
    let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
    p.divisions = [1; 3];
    p.mesh().unwrap()
}
fn array(faces: Vec<usize>) -> SurfaceArray {
    SurfaceArray::new(
        1,
        Piece {
            source: Source::Asset(1),
            pivot: Vec3::default(),
            forward: 0,
            up: 2,
        },
        &guide(),
        faces,
    )
    .unwrap()
}
fn near(a: Vec3, b: Vec3) {
    assert!((a - b).len() < 1e-4, "{a:?} != {b:?}");
}
#[test]
fn surface_quad_has_one_copy_and_contact_axes_follow_face() {
    let mut a = array(vec![0, 1]);
    a.piece.pivot = Vec3(0.5, 0.2, 0.3);
    a.offset = 0.2;
    let out = a.slots(&guide(), &part()).unwrap();
    assert_eq!(out.len(), 2);
    let p = out[0].unwrap();
    near(p.point(a.piece.pivot), Vec3(0., 0., 0.2));
    near(p.transform.up, axis(2));
    near(p.transform.forward, axis(0));
    a.rotation = 90.;
    let p = a.slots(&guide(), &part()).unwrap()[0].unwrap();
    near(p.transform.forward, axis(1));
}
#[test]
fn surface_moves_with_vertices_but_never_reassigns_changed_topology() {
    let a = array(vec![0]);
    let mut m = guide();
    for v in &mut m.vertices {
        *v = *v + Vec3(3., 2., 1.);
    }
    near(
        a.slots(&m, &part()).unwrap()[0].unwrap().transform.origin,
        Vec3(3., 2., 1.),
    );
    m.triangles.swap(0, 1);
    assert!(
        a.slots(&m, &part())
            .unwrap_err()
            .contains("topology changed")
    );
    let mut reassigned = a;
    reassigned.select(&m, vec![0]).unwrap();
    assert!(reassigned.slots(&m, &part()).is_ok());
}
#[test]
fn surface_fit_and_captured_axes_preserve_contact_and_normal() {
    let mut a = array(vec![0]);
    a.piece.forward = 1;
    a.piece.up = 0;
    a.fit = SurfaceFit::Stretch;
    let m = part();
    let p = a.slots(&guide(), &m).unwrap()[0].unwrap();
    let extents = m
        .vertices
        .iter()
        .map(|&v| p.point(v))
        .fold([0_f32; 3], |e, v| {
            [
                e[0].max(v.0.abs()),
                e[1].max(v.1.abs()),
                e[2].max(v.2.abs()),
            ]
        });
    assert!((extents[0] - 2.).abs() < 1e-5);
    assert!((extents[1] - 1.).abs() < 1e-5);
    near(p.point(axis(0)).norm(), axis(2));
    a.fit = SurfaceFit::Uniform;
    let p = a.slots(&guide(), &m).unwrap()[0].unwrap();
    assert!((p.transform.vector(axis(0)).len() - p.transform.vector(axis(2)).len()).abs() < 1e-5);
}
#[test]
fn surface_replace_and_bake_keep_unassigned_and_hidden_faces() {
    let mut a = array(vec![0]);
    let mut m = guide();
    m.hidden_faces = vec![false, false, true];
    assert_eq!(a.bake_guide(&m).unwrap(), m);
    a.replace = true;
    assert_eq!(a.display_guide(&m).unwrap().hidden_face_count(), 3);
    assert_eq!(m.hidden_face_count(), 1);
    let baked = a.bake_guide(&m).unwrap();
    assert_eq!(baked.triangles.len(), 1);
    assert_eq!(baked.hidden_face_count(), 1);
    assert!(baked.quads.is_empty());
}
#[test]
fn surface_rejects_bad_regions_degenerate_faces_and_settings() {
    let mut a = array(vec![0]);
    assert!(a.select(&guide(), vec![]).is_err());
    assert!(a.select(&guide(), vec![2]).is_err());
    let mut m = guide();
    for v in &mut m.vertices {
        v.1 = 0.;
    }
    assert!(a.slots(&m, &part()).is_err());
    a.scale = f32::NAN;
    assert!(a.validate().is_err());
}
#[test]
fn surface_hidden_guide_face_keeps_stable_slot_numbers() {
    let a = array(vec![0, 1]);
    let mut m = guide();
    m.hidden_faces = vec![true, true, false];
    let slots = a.slots(&m, &part()).unwrap();
    assert!(slots[0].is_none());
    assert!(slots[1].is_some());
}
