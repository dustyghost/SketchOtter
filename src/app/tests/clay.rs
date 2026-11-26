use super::*;
use crate::sculpt::{AdaptiveSettings, Geometry, Kind, Primitive, PrimitiveKind};

fn stroke(app: &mut App, ctx: &egui::Context, offset: f32) {
    let rect = navigation_frame(app, ctx, vec![]);
    let start = rect.center() + Vec2::new(offset, -130.);
    navigation_frame(
        app,
        ctx,
        vec![egui::Event::PointerMoved(start), primary_event(start, true)],
    );
    let mut end = start;
    for step in 1..=40 {
        let t = step as f32 / 40.;
        end = start + Vec2::new(20. * (t * std::f32::consts::PI).sin(), 260. * t);
        navigation_frame(app, ctx, vec![egui::Event::PointerMoved(end)]);
    }
    navigation_frame(
        app,
        ctx,
        vec![primary_event(end, false), egui::Event::PointerGone],
    );
}

fn scene(kind: Kind, flat: bool, adaptive: bool) -> App {
    let mut app = App::default();
    if flat {
        let mut cube = Primitive::for_kind(PrimitiveKind::Cube);
        cube.divisions = [40, 40, 2];
        app.document
            .replace_geometry(Geometry::new(cube.mesh().unwrap()))
            .unwrap();
    }
    app.ui.selected = app
        .document
        .brushes()
        .iter()
        .position(|b| b.kind == kind)
        .unwrap();
    app.document
        .set_adaptive(AdaptiveSettings {
            enabled: adaptive,
            ..Default::default()
        })
        .unwrap();
    app
}

#[test]
fn clay_viewport_stroke_restores_geometry_and_topology_with_one_undo() {
    for adaptive in [false, true] {
        let mut app = scene(Kind::Clay, false, adaptive);
        let ctx = egui::Context::default();
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        stroke(&mut app, &ctx, 0.);
        let after = app.document.geometry_snapshot();
        assert_ne!(before, after);
        after.mesh.validate_closed().unwrap();
        assert_eq!(app.document.history_counts().0, history.0 + 1);
        app.dispatch(Command::Undo, &ctx);
        assert_eq!(app.document.geometry_snapshot(), before);
        app.dispatch(Command::Redo, &ctx);
        assert_eq!(app.document.geometry_snapshot(), after);
    }
}

#[test]
fn clay_mirrored_viewport_stroke_has_one_exact_undo_and_redo() {
    let mut app = scene(Kind::Clay, false, false);
    let ctx = egui::Context::default();
    app.camera.yaw = 0.;
    app.camera.pitch = 0.;
    app.document
        .set_symmetry(crate::sculpt::Symmetry {
            enabled: true,
            ..Default::default()
        })
        .unwrap();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    stroke(&mut app, &ctx, 12.);
    let after = app.document.geometry_snapshot();
    assert_ne!(after, before);
    assert_eq!(after.mesh.triangles, before.mesh.triangles);
    assert_eq!(app.document.history_counts().0, history.0 + 1);
    app.dispatch(Command::Undo, &ctx);
    assert_eq!(app.document.geometry_snapshot(), before);
    app.dispatch(Command::Redo, &ctx);
    assert_eq!(app.document.geometry_snapshot(), after);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "manual symmetry seam review; requires Vulkan and CLAY_BUILDUP_PREVIEW"]
fn clay_symmetry_seam_offscreen_review() {
    use crate::sculpt::{Dab, Vec3};
    let prefix = std::env::var("CLAY_BUILDUP_PREVIEW").expect("Set CLAY_BUILDUP_PREVIEW");
    let mut capture = crate::viewport::UiCapture::new([900, 700]);
    for x in [0., 0.04, 0.12] {
        let mut app = scene(Kind::Clay, false, false);
        if let Ok(path) = std::env::var("CLAY_SYMMETRY_SOURCE") {
            app.document
                .replace_project(crate::persistence::load_project(&path).unwrap(), &path)
                .unwrap();
        }
        app.document
            .set_symmetry(crate::sculpt::Symmetry {
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let brush = app
            .document
            .brushes()
            .iter()
            .find(|b| b.kind == Kind::Clay)
            .unwrap()
            .clone();
        let before = app.document.geometry_snapshot();
        for step in 0..24 {
            let y = -0.3 + step as f32 * 0.6 / 23.;
            let local_x = if x == 0.12 {
                x * (2. * step as f32 / 23. - 1.)
            } else {
                x
            };
            let center = Vec3(local_x, y, (1. - local_x * local_x - y * y).sqrt());
            app.document.apply_dab(
                &brush,
                Dab {
                    center,
                    normal: center.norm(),
                    right: Vec3(1., 0., 0.),
                    up: Vec3(0., 1., 0.),
                    radius: 0.3,
                    pressure: 0.6,
                    delta: Vec3::default(),
                    invert: false,
                },
            );
        }
        app.document.end_stroke();
        let after = app.document.geometry_snapshot();
        assert_ne!(after, before);
        app.document.undo();
        assert_eq!(app.document.geometry_snapshot(), before);
        app.document.redo();
        assert_eq!(app.document.geometry_snapshot(), after);
        app.document.mesh().validate_closed().unwrap();
        app.camera.yaw = 0.25;
        app.camera.pitch = -0.15;
        app.viewport.initialize_renderer(&capture.state).unwrap();
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 700.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.show_viewport(ui));
            },
        );
        let rgba = capture.render(&ctx, output);
        capture.save(format!("{prefix}-symmetry-{x:.2}.png"), &rgba);
        app.viewport.destroy_renderer();
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "manual visual review; requires a Vulkan adapter (software lavapipe supported)"]
fn clay_buildup_offscreen_preview() {
    let mut capture = crate::viewport::UiCapture::new([1000, 800]);
    for (kind, flat, adaptive) in [
        (Kind::Standard, true, false),
        (Kind::Clay, true, false),
        (Kind::Standard, false, false),
        (Kind::Clay, false, false),
        (Kind::Clay, false, true),
    ] {
        let mut app = scene(kind, flat, adaptive);
        let ctx = egui::Context::default();
        for offset in [-110., 40., 40., 40.] {
            stroke(&mut app, &ctx, offset);
        }
        app.document.mesh().validate_closed().unwrap();
        app.viewport.initialize_renderer(&capture.state).unwrap();
        app.camera.yaw = 0.3;
        app.camera.pitch = -0.15;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1000., 800.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.show_viewport(ui));
            },
        );
        let rgba = capture.render(&ctx, output);
        if let Ok(prefix) = std::env::var("CLAY_BUILDUP_PREVIEW") {
            capture.save(
                format!(
                    "{prefix}-{kind:?}-{}-{}.png",
                    if flat { "flat" } else { "sphere" },
                    if adaptive { "adaptive" } else { "fixed" }
                ),
                &rgba,
            );
        }
        app.viewport.destroy_renderer();
    }
}
