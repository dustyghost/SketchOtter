use super::*;

#[test]
#[ignore = "manual review of Q options and presets; requires Vulkan and CLAY_GUIDE_PREVIEW"]
fn modeling_notes_quick_menu_capture() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let mut g = Guide::new(&directory, [1100, 900]);
    g.interactive = true;
    let mut primitive = Primitive::for_kind(PrimitiveKind::Cube);
    primitive.divisions = [3; 3];
    g.app
        .document
        .replace_geometry(crate::sculpt::Geometry::new(primitive.mesh().unwrap()))
        .unwrap();
    g.app.dispatch(Command::ModelingMode(true), &g.ctx);
    g.frame(vec![]);
    g.app.open_viewport_menu(&g.ctx);
    g.app.ui.viewport_menu.position = Some(egui::pos2(20., 150.));
    g.app.ui.viewport_menu.pinned = true;
    g.save("quick-menu-top");
    let p = g.app.ui.viewport_menu.rect.unwrap().center();
    for _ in 0..8 {
        g.frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., -180.),
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    g.save("quick-menu-options");
    assert!(
        g.text_positions
            .iter()
            .any(|(text, _)| text.contains("Tool presets"))
    );
}

#[test]
#[ignore = "manual review of the saved thickness case; requires Vulkan and CLAY_GUIDE_PREVIEW"]
fn modeling_notes_thickness_capture() {
    let directory = std::env::var("CLAY_GUIDE_PREVIEW").unwrap();
    std::fs::create_dir_all(&directory).unwrap();
    let source = std::env::var("CLAY_REVIEW_SOURCE").unwrap();
    let project = crate::persistence::load_project(&source).unwrap();
    let mut g = Guide::new(std::path::Path::new(&directory), [1000, 850]);
    g.interactive = true;
    g.app.document.replace_project(project, &source).unwrap();
    let mut geometry = crate::sculpt::Geometry::new(g.app.document.mesh().clone());
    geometry.scene.active_metadata = g
        .app
        .document
        .geometry_snapshot()
        .scene
        .active_metadata
        .clone();
    g.app.document.replace_geometry(geometry).unwrap();
    g.app.camera.yaw = 0.5;
    g.app.camera.pitch = -0.6;
    g.app.camera.roll = 0.;
    g.app.camera.zoom = 0.5;
    g.app.camera.pan = Vec2::new(-100., 0.);
    g.app.ui.wire = true;
    g.panel(Tool::Submodels, Dock::Right);
    for thickness in [0.3, 0.9, 1.2] {
        g.app.document.set_temporary_thickness(thickness).unwrap();
        g.save(&format!("thickness-{thickness}"));
    }
}
