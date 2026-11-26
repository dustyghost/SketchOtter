use super::*;
use crate::sculpt::{Geometry, GroupPurpose, PanelProfile, Vec3, VertexGroup};
fn fixture(kind: &str) -> crate::sculpt::Mesh {
    let mut primitive = Primitive::for_kind(PrimitiveKind::Grid);
    primitive.divisions = [12, 12, 1];
    let mut mesh = primitive.mesh().unwrap();
    for name in ["Left plate", "Right plate"] {
        mesh.vertex_groups.push(VertexGroup {
            purpose: GroupPurpose::Selection,
            visible: true,
            name: name.into(),
            weights: vec![0.; mesh.vertices.len()],
        });
    }
    mesh.polygroups = mesh
        .quads
        .iter()
        .flat_map(|q| {
            let p = q.iter().fold(Vec3::default(), |s, &v| s + mesh.vertices[v]) * 0.25;
            let g = match kind {
                "hole" => usize::from(p.0.abs() > 0.25 || p.1.abs() > 0.25),
                "islands" => usize::from(p.0.abs() > 0.4),
                "concave" => usize::from(p.0 < 0.2 || p.1 < 0.2),
                "narrow" => usize::from(p.0.abs() < 0.15),
                "jagged" => 1,
                _ => {
                    if p.0 < 0. {
                        1
                    } else {
                        2
                    }
                }
            };
            [g; 2]
        })
        .collect();
    for p in &mut mesh.vertices {
        if kind == "jagged" && p.1 > 0.99 && p.0.abs() < 0.99 {
            p.1 += if ((p.0 + 1.) * 6.).round() as i32 % 2 == 0 {
                0.03
            } else {
                -0.03
            };
        }
        p.2 = 0.1 * (p.0 * p.0 + p.1 * p.1);
    }
    if kind == "jagged" {
        // The source deliberately mixes diagonal triangles and quads.
        mesh.quads = mesh
            .quads
            .into_iter()
            .enumerate()
            .filter_map(|(i, q)| (i % 3 == 0).then_some(q))
            .collect();
    }
    mesh
}
#[test]
#[ignore = "manual shaded/wireframe, subdivision and actual panel-control review; requires Vulkan and CLAY_GUIDE_PREVIEW"]
fn panels_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for kind in ["adjacent", "hole", "islands", "concave", "narrow", "jagged"] {
        let mut g = Guide::new(&directory, [1220, 920]);
        g.interactive = true;
        g.toolbar = false;
        g.app
            .document
            .replace_geometry(Geometry::new(fixture(kind)))
            .unwrap();
        g.app.document.mark_saved("panel-source.clay");
        g.app.camera.yaw = 0.18;
        g.app.camera.pitch = -0.35;
        g.app.camera.zoom = 0.72;
        g.app.camera.pan = Vec2::new(210., 0.);
        let before = g.app.document.geometry_snapshot();
        g.app.dispatch(Command::OpenPanels, &g.ctx);
        crate::app::test_support::finish_projection(&mut g.app);
        assert!(g.app.ui.projection.open, "{}", g.app.ui.status);
        g.save(&format!("panels-{kind}-initial"));
        click_guide_label(&mut g, "All groups");
        // Setting changes must clear Apply, without changing the document.
        crate::app::test_support::finish_projection(&mut g.app);
        assert!(!g.app.ui.projection.report.as_ref().unwrap().can_apply());
        g.app.ui.projection.panels.options.border.width = 0.025;
        if kind == "jagged" {
            g.app.ui.projection.panels.options.border.smoothing = 1.;
            g.app.ui.projection.panels.options.border.corner_angle = 75.;
        }
        g.save(&format!("panels-{kind}-source"));
        click_guide_label(&mut g, "Preview panels");
        crate::app::test_support::finish_projection(&mut g.app);
        assert!(
            g.app.projection.draft.is_some(),
            "{kind}: {}",
            g.app.ui.status
        );
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.save(&format!("panels-{kind}-shaded"));
        click_guide_label(&mut g, "Wireframe");
        assert!(g.app.ui.wire);
        g.save(&format!("panels-{kind}-wire"));
        if kind == "adjacent" {
            let expected = g
                .app
                .projection
                .draft
                .as_ref()
                .unwrap()
                .panels
                .as_ref()
                .unwrap()
                .panels[0]
                .mesh
                .clone();
            click_guide_label(&mut g, "Apply panels");
            assert_eq!(g.app.document.mesh(), &expected);
            assert_eq!(g.app.document.undo_label(), Some("Generate panels"));
            let after = g.app.document.geometry_snapshot();
            g.app.dispatch(Command::Undo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), before);
            g.app.dispatch(Command::Redo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), after);
            let smoothed = g
                .app
                .document
                .mesh()
                .smooth_subdivide(1., &|| false)
                .unwrap();
            smoothed.validate_closed().unwrap();
            g.app
                .document
                .replace_geometry(Geometry::new(smoothed))
                .unwrap();
            g.app.ui.wire = false;
            g.save("panels-subdivided-shaded");
            g.app.ui.wire = true;
            g.save("panels-subdivided-wire");
        } else {
            click_guide_label(&mut g, "Cancel");
            assert_eq!(g.app.document.geometry_snapshot(), before);
        }
    }
    // Compare loop resolution and bevel profiles on the same saved geometry.
    let mut g = Guide::new(&directory, [1220, 920]);
    g.interactive = true;
    g.toolbar = false;
    g.app
        .document
        .replace_geometry(Geometry::new(fixture("hole")))
        .unwrap();
    g.app.camera.yaw = 0.2;
    g.app.camera.pitch = -0.5;
    g.app.camera.zoom = 0.8;
    g.app.camera.pan = Vec2::new(210., 0.);
    g.app.dispatch(Command::OpenPanels, &g.ctx);
    for (loops, profile, name) in [
        (1, PanelProfile::Flat, "flat-1"),
        (4, PanelProfile::Rounded, "round-4"),
    ] {
        g.app.ui.projection.panels.options.border.loops = loops;
        g.app.ui.projection.panels.options.profile = profile;
        g.app.dispatch(Command::PreviewProjection, &g.ctx);
        crate::app::test_support::finish_projection(&mut g.app);
        assert!(g.app.projection.draft.is_some(), "{}", g.app.ui.status);
        g.app.ui.wire = true;
        g.save(&format!("panels-profile-{name}"));
    }
    click_guide_label(&mut g, "Cancel");
    review_small_window(&directory);
}

fn review_small_window(directory: &std::path::Path) {
    // A separate renderer/context matches launching the app on a small display.
    let mut g = Guide::new(directory, [800, 560]);
    g.interactive = true;
    g.toolbar = false;
    g.app
        .document
        .replace_geometry(Geometry::new(fixture("adjacent")))
        .unwrap();
    g.app.camera.zoom = 0.5;
    g.app.camera.pan = Vec2::new(260., 0.);
    g.app.camera.yaw = 0.18;
    g.app.camera.pitch = -0.35;
    let before = g.app.document.geometry_snapshot();
    g.app.dispatch(Command::OpenPanels, &g.ctx);
    crate::app::test_support::finish_projection(&mut g.app);
    g.save("panels-small-window-selection");
    click_guide_label(&mut g, "Preview panels");
    crate::app::test_support::finish_projection(&mut g.app);
    g.save("panels-small-window");
    click_guide_label(&mut g, "Cancel");
    assert_eq!(g.app.document.geometry_snapshot(), before);
    g.app.dispatch(Command::OpenPanels, &g.ctx);
    crate::app::test_support::finish_projection(&mut g.app);
    g.frame(vec![]);
    click_guide_label(&mut g, "Preview panels");
    crate::app::test_support::finish_projection(&mut g.app);
    click_guide_label(&mut g, "Apply panels");
    assert_eq!(g.app.document.undo_label(), Some("Generate panels"));
}

#[test]
#[ignore = "GPU interaction regression: panel Apply/Cancel must remain accessible on a short viewport"]
fn panels_controls_offscreen_review() {
    let directory = std::env::var("CLAY_GUIDE_PREVIEW")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("sketchotter-panel-controls"));
    std::fs::create_dir_all(&directory).unwrap();
    review_small_window(&directory);
}
