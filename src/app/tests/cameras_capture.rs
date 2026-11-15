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
        click_guide_label(&mut g, "Save view");
        assert_eq!(g.app.document.saved_cameras().len(), 1);
        let before = g.app.camera;
        click_guide_label(&mut g, "Reset roll");
        assert_eq!(g.app.camera, Camera { roll: 0., ..before });
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
        click_guide_label(&mut g, "Update from view");
        assert_eq!(g.app.document.saved_cameras().len(), 1);
        assert_eq!(g.app.document.geometry_snapshot(), geometry);
        assert_eq!(g.app.document.history_counts(), history);
    }
}
