use super::*;
use crate::sculpt::{Geometry, Vec3};

#[test]
#[ignore = "renders and clicks the camera workflow; requires Vulkan"]
fn saved_cameras_offscreen_review() {
    let directory = std::env::var_os("CLAY_GUIDE_PREVIEW")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("clay-cameras-review"));
    std::fs::create_dir_all(&directory).unwrap();
    for size in [[1100, 800], [640, 520]] {
        let mut g = Guide::new(&directory, size);
        g.interactive = true;
        let mut primitive = Primitive::for_kind(PrimitiveKind::Cube);
        primitive.size = Vec3(0.8, 1.2, 0.6);
        g.app
            .document
            .replace_geometry(Geometry::new(primitive.mesh().unwrap()))
            .unwrap();
        g.app.ui.wire = true;
        g.app.camera.yaw = 0.45;
        g.app.camera.pitch = -0.3;
        g.app.camera.roll = 0.4;
        g.app.camera.zoom = 0.7;
        g.frame(vec![]);
        g.frame(vec![]);
        let geometry = g.app.document.geometry_snapshot();
        let history = g.app.document.history_counts();
        click_guide_label(&mut g, "Cameras…");
        g.frame(vec![]);
        g.frame(vec![]);
        click_guide_label(&mut g, "Perspective");
        assert!(g.app.camera.perspective);
        click_guide_label(&mut g, "Save view");
        assert_eq!(g.app.document.saved_cameras().len(), 1);
        assert!(g.app.document.saved_cameras()[0].view.perspective);
        let before = g.app.camera;
        click_guide_label(&mut g, "Orthographic");
        assert!(!g.app.camera.perspective);
        assert!(g.app.document.saved_cameras()[0].view.perspective);
        let orthographic = g.app.camera;
        click_guide_label(&mut g, "Reset roll");
        assert_eq!(
            g.app.camera,
            Camera {
                roll: 0.,
                ..orthographic
            }
        );
        click_guide_label(&mut g, "Camera 1");
        assert_eq!(g.app.camera, before);
        if size[0] == 1100 {
            // Exercise the same window with the panels hidden in Zen mode.
            g.app.ui.set_zen_mode(true);
            g.toolbar = false;
            g.save("saved-cameras-zen");
            g.toolbar = true;
            g.app.ui.set_zen_mode(false);
        }
        g.save(&format!("saved-cameras-{}", size[0]));
        let saved = g.app.document.saved_cameras()[0].clone();
        click_guide_label(&mut g, "Orthographic");
        assert!(!g.app.camera.perspective);
        g.app.camera.roll_by(0.2);
        click_guide_label(&mut g, "Update from view");
        assert_eq!(
            g.app.document.saved_cameras()[0],
            saved,
            "opening confirmation cannot overwrite"
        );
        g.frame(vec![]);
        g.frame(vec![]);
        click_guide_label(&mut g, "Keep saved view");
        assert_eq!(g.app.document.saved_cameras()[0], saved);
        click_guide_label(&mut g, "Update from view");
        g.frame(vec![]);
        g.frame(vec![]);
        g.save(&format!("camera-overwrite-{}", size[0]));
        click_guide_label(&mut g, "Replace saved view");
        assert_eq!(
            g.app.document.saved_cameras()[0].view.roll,
            g.app.camera.roll
        );
        assert_eq!(g.app.document.saved_cameras().len(), 1);
        assert!(!g.app.document.saved_cameras()[0].view.perspective);
        if size[0] == 1100 {
            camera_name_input(&mut g, "camera_save_name_rect", "Detail");
            click_guide_label(&mut g, "Save view");
            assert_eq!(g.app.document.saved_cameras()[1].name, "Detail");
            click_guide_label(&mut g, "Save view");
            assert_eq!(
                g.app.document.saved_cameras()[2].name,
                "Camera 2",
                "successful typed saves clear the name field"
            );
            // Rename the first row: both duplicate and blank names remain editable.
            click_guide_label(&mut g, "Rename");
            camera_name_input(&mut g, "camera_rename_name_rect", "Detail");
            click_guide_label(&mut g, "Apply name");
            assert_eq!(g.app.document.saved_cameras()[0].name, "Camera 1");
            assert!(
                g.text_positions
                    .iter()
                    .any(|(t, _)| t.contains("already exists"))
            );
            g.save("camera-rename-error");
            camera_name_input(&mut g, "camera_rename_name_rect", " ");
            click_guide_label(&mut g, "Apply name");
            assert_eq!(g.app.document.saved_cameras()[0].name, "Camera 1");
            camera_name_input(&mut g, "camera_rename_name_rect", "Front");
            click_guide_label(&mut g, "Apply name");
            assert_eq!(g.app.document.saved_cameras()[0].name, "Front");
            // A long library scrolls independently from the save controls.
            for i in 0..20 {
                g.app.dispatch(
                    Command::Camera(crate::command::CameraCommand::Save(format!("Study {i}"))),
                    &g.ctx,
                );
            }
            g.frame(vec![]);
            click_guide_label(&mut g, "Save view");
            assert_eq!(g.app.document.saved_cameras().len(), 24);
            g.save("camera-library");
        }
        assert_eq!(g.app.document.geometry_snapshot(), geometry);
        assert_eq!(g.app.document.history_counts(), history);
    }
}

fn camera_name_input(g: &mut Guide, key: &str, value: &str) {
    g.frame(vec![]);
    let rect = g
        .ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(key)))
        .unwrap();
    let point = rect.center();
    g.frame(vec![
        egui::Event::PointerMoved(point),
        primary_event(point, true),
    ]);
    g.frame(vec![primary_event(point, false)]);
    g.modifiers = egui::Modifiers::COMMAND;
    g.frame(vec![egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }]);
    g.modifiers = egui::Modifiers::NONE;
    g.frame(vec![egui::Event::Text(value.into())]);
    g.frame(vec![]);
}
