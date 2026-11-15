use super::*;
use crate::{
    command::InstanceCommand as C,
    sculpt::{Geometry, Vec3, instances::*},
    viewport::{Component, ComponentMode},
};
#[test]
#[ignore = "requires Vulkan; clicks surface arrays and progressive rotation controls"]
fn surface_array_controls_offscreen_review() {
    let directory = std::env::var_os("CLAY_GUIDE_PREVIEW")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/tmp/clay-surface-arrays".into());
    std::fs::create_dir_all(&directory).unwrap();
    let mut g = Guide::new(&directory, [1440, 1050]);
    g.interactive = true;
    g.panel(Tool::Mesh, Dock::Left);
    g.panel(Tool::Submodels, Dock::Right);
    g.app.ui.preferences.right_width = 360.;
    let mut p = Primitive::for_kind(PrimitiveKind::Cube);
    p.divisions = [1; 3];
    p.size = Vec3(0.52, 0.38, 0.16);
    let mut mesh = p.mesh().unwrap();
    mesh.materials =
        vec![crate::sculpt::Material::from_srgb([0.85, 0.51, 0.19], 0.5, 0.2); mesh.vertices.len()];
    g.app
        .document
        .replace_geometry(Geometry::new(mesh))
        .unwrap();
    let asset = g
        .app
        .document
        .capture_object("Panel".into(), Vec3(0., 0., -0.08), 0, 2)
        .unwrap();
    let mut geometry = g.app.document.geometry_snapshot();
    p.kind = PrimitiveKind::Plane;
    p.size = Vec3(2.4, 2.4, 1.);
    p.divisions = [3, 3, 1];
    geometry.mesh = p.mesh().unwrap();
    geometry.name = "Panel guide".into();
    g.app.document.replace_geometry(geometry).unwrap();
    g.app.ui.instances.asset = Some(asset);
    g.app.ui.modeling.active = true;
    g.app.ui.modeling.mode = ComponentMode::Face;
    g.app.viewport.modeling.sync(&g.app.document);
    g.app.viewport.modeling.selected = [0, 1, 3, 4].map(Component::Face).into_iter().collect();
    g.app.camera.yaw = 0.35;
    g.app.camera.pitch = -0.45;
    g.app.camera.zoom = 0.65;
    g.frame(vec![]);
    click_guide_label(&mut g, "Surface array from selected faces…");
    assert!(
        g.app.document.instance_preview_active(),
        "{}",
        g.app.ui.status
    );
    assert_eq!(g.app.document.instance_placements().unwrap().len(), 4);
    g.save("surface-keep");
    click_guide_label(&mut g, "Replace faces");
    let resolved = g.app.document.instance_placements().unwrap();
    assert_eq!(
        resolved
            .source_meshes
            .values()
            .next()
            .unwrap()
            .hidden_face_count(),
        8
    );
    g.save("surface-replace");
    click_guide_label(&mut g, "Apply instances");
    let saved = g.app.document.geometry_snapshot();
    click_guide_label(&mut g, "Edit guide / select new region");
    assert!(g.app.document.instance_placements().unwrap().is_empty());
    g.app.viewport.modeling.sync(&g.app.document);
    g.app.viewport.modeling.selected = [4, 5].map(Component::Face).into_iter().collect();
    g.frame(vec![]);
    click_guide_label(&mut g, "Use selected faces for surface array");
    assert_eq!(g.app.document.instance_placements().unwrap().len(), 2);
    click_guide_label(&mut g, "Cancel preview");
    assert_eq!(g.app.document.geometry_snapshot(), saved);
    click_guide_label(&mut g, "Make editable…");
    click_guide_label(&mut g, "Bake now");
    finish_mesh_job(&mut g.app);
    assert!(
        g.app.document.instances().sets.is_empty(),
        "{}",
        g.app.ui.status
    );
    assert_eq!(g.app.document.mesh().triangles.len(), 18 - 8 + 4 * 12);
    g.app.dispatch(Command::Undo, &g.ctx);
    assert_eq!(g.app.document.geometry_snapshot(), saved);

    // A second small scene makes the signed angle and individual pivots legible.
    let mut g = Guide::new(&directory, [1200, 1000]);
    g.interactive = true;
    g.panel(Tool::Submodels, Dock::Right);
    g.app.ui.preferences.right_width = 360.;
    p.kind = PrimitiveKind::Cube;
    p.size = Vec3(0.6, 0.12, 0.07);
    p.divisions = [1; 3];
    let mut geo = Geometry::new(p.mesh().unwrap());
    geo.scene.active_metadata.visible = false;
    g.app.document.replace_geometry(geo).unwrap();
    g.app
        .dispatch(Command::Instances(C::NewLegacyArray), &g.ctx);
    let mut set = g.app.ui.instances.editor.clone().unwrap();
    let Layout::Array(a) = &mut set.layout else {
        panic!()
    };
    a.count = 9;
    a.spacing = Vec3(0., 0., 0.15);
    a.center = Vec3(0., 0., -0.6);
    g.app.ui.instances.editor = Some(set.clone());
    g.app
        .dispatch(Command::Instances(C::Preview(Box::new(set))), &g.ctx);
    g.app.ui.instances.focus_array = true;
    g.frame(vec![]);
    g.frame(vec![]);
    let rect = g
        .ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("rotation_per_copy", 2usize))))
        .unwrap();
    let point = rect.center();
    assert!(point.y < 1000.);
    g.frame(vec![
        egui::Event::PointerMoved(point),
        primary_event(point, true),
    ]);
    g.frame(vec![egui::Event::PointerMoved(point + Vec2::new(40., 0.))]);
    g.frame(vec![
        primary_event(point + Vec2::new(40., 0.), false),
        egui::Event::PointerGone,
    ]);
    let Layout::Array(a) = &g.app.document.instances().sets[0].layout else {
        panic!()
    };
    assert!(a.step_rotation.2 > 0., "drag changes the per-copy angle");
    g.app.camera.zoom = 1.1;
    g.app.camera.yaw = 0.4;
    g.app.camera.pitch = -0.3;
    g.save("progressive-rotation");
    click_guide_label(&mut g, "Cancel preview");
    assert!(g.app.document.instances().sets.is_empty());
}
