//! Exercise the same dense wire path as the startup sphere with a known diagonal.
use super::{ui_capture::Headless, *};
use crate::sculpt::{Geometry, Mesh};

#[test]
#[ignore = "requires Vulkan; compare quad boundaries, diagonals and wire fallbacks"]
fn gpu_dense_wire_draws_quad_boundaries_and_optional_diagonals() {
    let headless = Headless::new();
    let mut gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
    let n = 128;
    let mut mesh = Mesh::default();
    for y in 0..=n {
        for x in 0..=n {
            mesh.vertices
                .push(Vec3(x as f32 / 64. - 1., y as f32 / 64. - 1., 0.));
            mesh.mask.push(0.);
        }
    }
    for y in 0..n {
        for x in 0..n {
            let a = y * (n + 1) + x;
            let [b, c, d] = [a + 1, a + n + 2, a + n + 1];
            mesh.quads.push([a, b, c, d]);
            mesh.triangles.extend([[a, b, c], [a, c, d]]);
        }
    }
    let mut doc = Document::default();
    doc.replace_geometry(Geometry::new(mesh)).unwrap();
    let display = Arc::new(DisplayMesh::new(&doc));
    gpu.upload(&display);
    let preview = Preview::default();
    assert!(gpu.use_wire_surface(false, &preview));
    assert!(!gpu.use_wire_surface(
        false,
        &Preview {
            backface_opacity: 0.35,
            ..Default::default()
        }
    ));
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128., 128.));
    let p = Params::new(
        rect,
        Vec3(1., 0., 0.),
        Vec3(0., 1., 0.),
        Vec3(0., 0., 1.),
        1536.,
        rect.center(),
        display.depth_extent,
    );
    let mut draw = |enabled, mode, triangles| {
        gpu.paint(&p, [128, 128], enabled, mode, triangles, &preview);
        gpu.read()
    };
    let solid = draw(false, WireMode::Visible, false);
    let quad = draw(true, WireMode::Visible, false);
    let tri = draw(true, WireMode::Visible, true);
    let pixel = |image: &[u8], x: usize, y: usize| {
        let id: usize = (128 * y + x) * 4;
        image[id + 1]
    };
    let grid = (64..88).flat_map(|x| (64..88).map(move |y| (x, y)));
    let (mut painted_edges, mut painted_diagonal) = (0, 0);
    for (x, y) in grid {
        let shade = pixel(&solid, x, y);
        let q = pixel(&quad, x, y);
        let t = pixel(&tri, x, y);
        if (66..86).contains(&x) && (66..86).contains(&y) {
            assert_eq!(q, shade, "quad interior has a false diagonal");
            if x == y {
                painted_diagonal += usize::from(t.saturating_add(8) < shade);
            }
            if x.abs_diff(y) > 3 {
                assert_eq!(q, t);
            }
        } else if x == 64 || y == 64 {
            painted_edges += usize::from(q.saturating_add(8) < shade);
        }
    }
    assert!(
        painted_edges > 24 && painted_diagonal > 12,
        "edges={painted_edges}, diagonal={painted_diagonal}"
    );
    assert!(gpu.edges.is_none() && gpu.triangle_edges.is_none());
    gpu.paint(&p, [128, 128], true, WireMode::Transparent, false, &preview);
    assert!(gpu.edges.is_some() && gpu.triangle_edges.is_none());
    gpu.paint(&p, [128, 128], true, WireMode::Front, true, &preview);
    assert!(gpu.triangle_edges.is_some());

    let mut changed = doc.geometry_snapshot();
    changed.mesh.triangles.swap(0, 1);
    doc.replace_geometry(changed).unwrap();
    gpu.upload(&Arc::new(DisplayMesh::new(&doc)));
    assert!(
        !gpu.use_wire_surface(false, &preview),
        "reordered triangles require explicit edges"
    );
    assert!(gpu.use_wire_surface(true, &preview));
}

#[test]
#[ignore = "requires GPU; keep Visible wireframe on through sphere subdivision and level changes"]
fn gpu_dense_wire_stays_enabled_as_sphere_levels_change() {
    let headless = Headless::new();
    let mut gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
    let mut doc = Document::default();
    let preview = Preview::default();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128., 128.));
    let (r, u, v) = crate::viewport::Camera::default().basis();
    let mut draw = |doc: &Document, dense: bool| {
        let display = Arc::new(DisplayMesh::new(doc));
        gpu.upload(&display);
        let params = Params::new(rect, r, u, v, 50., rect.center(), display.depth_extent);
        gpu.paint(
            &params,
            [128, 128],
            true,
            WireMode::Visible,
            false,
            &preview,
        );
        assert_eq!(gpu.edges.is_none(), dense);
        assert!(gpu.triangle_edges.is_none());
    };
    draw(&doc, false);
    let refined =
        crate::sculpt::ValidatedGeometry::subdivided(doc.geometry_snapshot(), &|| false).unwrap();
    doc.commit_validated_geometry(refined, "Subdivide", None);
    draw(&doc, true);
    doc.set_mesh_level(1).unwrap();
    draw(&doc, false);
    doc.set_mesh_level(2).unwrap();
    draw(&doc, true);
}
