use super::*;

fn click(g: &mut Guide, point: Pos2) {
    g.frame(vec![
        egui::Event::PointerMoved(point),
        primary_event(point, true),
    ]);
    g.frame(vec![primary_event(point, false)]);
}
fn control(g: &mut Guide, label: &str) {
    reach_sidebar_control(g, label);
    click_guide_label(g, label);
}
#[test]
#[ignore = "actual selection/snapping controls; requires Vulkan and CLAY_GUIDE_PREVIEW"]
fn precision_modeling_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for (size, suffix) in [([1200, 1000], ""), ([760, 720], "-narrow")] {
        let mut g = Guide::new(&directory, size);
        g.interactive = true;
        g.toolbar = false;
        g.panel(Tool::Mesh, Dock::Right);
        let mut primitive = Primitive::for_kind(PrimitiveKind::Cube);
        primitive.divisions = [3; 3];
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(primitive.mesh().unwrap()))
            .unwrap();
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.zoom = 0.7;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        let rect = g.frame(vec![]).0;
        click(&mut g, rect.center());
        let original = g.app.viewport.modeling.selected.clone();
        control(&mut g, "Grow selection");
        assert_eq!(g.app.viewport.modeling.selected.len(), 5);
        g.save(&format!("precision-selection-growth{suffix}"));
        control(&mut g, "Undo selection change");
        assert_eq!(g.app.viewport.modeling.selected, original);
        control(&mut g, "Redo selection change");
        assert_eq!(g.app.viewport.modeling.selected.len(), 5);
        control(&mut g, "Shrink selection");
        assert_eq!(g.app.viewport.modeling.selected, original);
        let mesh = crate::app::tests::modeling::precision_snap_mesh();
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(mesh.clone()))
            .unwrap();
        g.app.camera.zoom = 0.8;
        let rect = g.frame(vec![]).0;
        let scale = rect.width().min(rect.height()) * 0.35 * 0.8;
        let point = |x: f32, y: f32| rect.center() + Vec2::new(x, -y) * scale;
        click(&mut g, point(-0.7, 0.));
        assert_eq!(g.app.ui.modeling.count, 1);
        control(&mut g, "Move");
        control(&mut g, "Pick snap source");
        click(&mut g, point(-0.4, -0.4));
        click(&mut g, point(0.4, -0.4));
        assert!(
            g.app.viewport.modeling.snap_ready(),
            "{:?}",
            g.app.viewport.modeling.snap_hint()
        );
        let expected = g.app.viewport.modeling.preview().unwrap().clone();
        assert_eq!(g.app.document.mesh(), &mesh);
        control(&mut g, "Pick another target");
        assert!(!g.app.viewport.modeling.snap_ready());
        click(&mut g, point(0.4, 0.4));
        assert!(g.app.viewport.modeling.snap_ready());
        control(&mut g, "Cancel snap");
        assert_eq!(g.app.document.mesh(), &mesh);
        control(&mut g, "Pick snap source");
        click(&mut g, point(-0.4, -0.4));
        click(&mut g, point(0.4, -0.4));
        control(&mut g, "Apply snap");
        assert_eq!(g.app.document.mesh(), &expected);
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.mesh(), &mesh);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.mesh(), &expected);
        g.app.dispatch(Command::Undo, &g.ctx);
        g.frame(vec![]);
        // Restore selection using the actual selection tool, then capture a pinned preview.
        control(&mut g, "Select");
        click(&mut g, point(-0.7, 0.));
        control(&mut g, "Move");
        control(&mut g, "Pick snap source");
        click(&mut g, point(-0.4, -0.4));
        click(&mut g, point(0.4, -0.4));
        reach_sidebar_control(&mut g, "Apply snap");
        g.save(&format!("precision-snap-preview{suffix}"));
        assert!(g.app.viewport.modeling.snap_ready());
    }
}
