use super::*;
use crate::command::CameraCommand as C;

fn fixture() -> (App, egui::Context) {
    let mut app = App::default();
    let ctx = egui::Context::default();
    navigation_frame(&mut app, &ctx, vec![]);
    app.camera = Camera {
        yaw: 0.8,
        pitch: -0.45,
        roll: 0.7,
        pan: Vec2::new(110., -70.),
        zoom: 1.7,
        perspective: true,
        field_of_view: 62.,
        perspective_radius: 3.,
    };
    (app, ctx)
}

#[test]
fn cameras_active_identity_survives_rename_and_does_not_follow_deleted_indices() {
    let (mut app, ctx) = fixture();
    app.dispatch(Command::Camera(C::Save("First".into())), &ctx);
    app.dispatch(Command::Camera(C::Save("Second".into())), &ctx);
    assert_eq!(
        app.document.saved_cameras()[0].view,
        app.document.saved_cameras()[1].view
    );
    assert_eq!(app.ui.cameras.active.as_deref(), Some("Second"));
    app.dispatch(Command::Camera(C::Recall(0)), &ctx);
    app.dispatch(Command::Camera(C::Rename(0, "Renamed".into())), &ctx);
    assert_eq!(app.ui.cameras.active.as_deref(), Some("Renamed"));
    app.dispatch(Command::Camera(C::Rename(0, "Second".into())), &ctx);
    assert_eq!(app.ui.cameras.active.as_deref(), Some("Renamed"));
    app.dispatch(Command::Camera(C::Delete(1)), &ctx);
    assert_eq!(app.ui.cameras.active.as_deref(), Some("Renamed"));
    app.dispatch(Command::Camera(C::Delete(0)), &ctx);
    assert_eq!(app.ui.cameras.active, None);
}

#[test]
fn cameras_view_matching_handles_resize_roundoff_and_detects_navigation() {
    let (mut app, _) = fixture();
    app.camera.pan = Vec2::new(0.5, -0.8);
    let saved = app.camera.saved_view(Vec2::splat(672.)).unwrap();
    let restored = Camera::from_saved_view(saved, Vec2::splat(360.)).unwrap();
    let roundtrip = restored.saved_view(Vec2::splat(360.)).unwrap();
    assert_ne!(
        saved.pan, roundtrip.pan,
        "fixture exercises real f32 conversion loss"
    );
    assert!(saved.matches(&roundtrip));
    let mut rolled = roundtrip;
    rolled.roll += std::f32::consts::TAU;
    assert!(saved.matches(&rolled));
    for change in 0..8 {
        let mut changed = roundtrip;
        match change {
            0 => changed.pan[0] += 0.01,
            1 => changed.roll += 0.01,
            2 => changed.yaw += 0.01,
            3 => changed.pitch += 0.01,
            4 => changed.zoom *= 1.01,
            5 => changed.field_of_view += 1.,
            6 => changed.perspective_radius += 0.1,
            _ => changed.perspective = !changed.perspective,
        }
        assert!(!saved.matches(&changed));
    }
    let mut tiny = saved;
    tiny.zoom = 1e-6;
    let mut zoomed = tiny;
    zoomed.zoom *= 2.;
    assert!(!tiny.matches(&zoomed));
}

#[test]
fn cameras_reset_roll_keeps_direction_framing_geometry_and_redo() {
    let (mut app, ctx) = fixture();
    app.document.invert_mask().unwrap();
    app.document.undo();
    app.document.mark_saved("roll.clay");
    let project = app.document.project();
    let history = app.document.history_counts();
    let before = app.camera;
    app.dispatch(Command::Action(Action::ResetRoll), &ctx);
    assert_eq!(app.camera, Camera { roll: 0., ..before });
    assert_eq!(app.camera.basis().2, before.basis().2);
    assert!(app.document.project() == project);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
}

#[test]
fn cameras_roll_widget_drag_both_ways_and_double_click_reset() {
    use super::viewport_controls::{click, control_rect, frame, settle};
    let (mut app, ctx) = fixture();
    let index = app
        .ui
        .preferences
        .viewport_controls
        .iter()
        .position(|c| c.kind == crate::preferences::ViewportControlKind::Roll)
        .unwrap();
    let before = app.camera;
    let project = app.document.project();
    for distance in [60., -90.] {
        settle(&mut app, &ctx);
        let start = control_rect(&ctx, index).center();
        let roll = app.camera.roll;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start), primary_event(start, true)],
        );
        let end = start + Vec2::new(distance, 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        frame(&mut app, &ctx, vec![primary_event(end, false)]);
        assert!((app.camera.roll - roll - distance * 0.008).abs() < 1e-5);
        assert_eq!(
            Camera {
                roll: before.roll,
                ..app.camera
            },
            before
        );
    }
    let position = control_rect(&ctx, index).center();
    click(&mut app, &ctx, position);
    click(&mut app, &ctx, position);
    assert_eq!(app.camera.roll, 0.);
    assert!(app.document.project() == project);
    assert_eq!(app.document.history_counts(), (0, 0));
    assert!(!app.document.is_dirty());
}

#[test]
fn cameras_recall_restores_each_projection_and_scales_framing_after_resize() {
    for perspective in [false, true] {
        let (mut app, ctx) = fixture();
        app.camera.perspective = perspective;
        let before = app.camera;
        app.dispatch(Command::Camera(C::Save("Detail".into())), &ctx);
        assert_eq!(app.document.saved_cameras().len(), 1);
        let saved = app.document.saved_cameras()[0].clone();
        app.document.mark_saved("views.clay");
        app.camera.orbit(Vec2::new(60., 40.));
        app.camera.pan += Vec2::new(20., 10.);
        app.camera.zoom_step(true);
        app.camera.perspective = !perspective;
        app.camera.field_of_view = 20.;
        app.camera.perspective_radius = 8.;
        assert_eq!(app.document.saved_cameras()[0], saved);
        app.dispatch(Command::Camera(C::Recall(0)), &ctx);
        assert_eq!(
            Camera {
                pan: before.pan,
                ..app.camera
            },
            before
        );
        assert!((app.camera.pan - before.pan).length() < 1e-4);
        let rect = app.ui.viewport_menu.viewport_rect.unwrap();
        app.ui.viewport_menu.viewport_rect =
            Some(egui::Rect::from_min_size(rect.min, rect.size() * 0.5));
        app.dispatch(Command::Camera(C::Recall(0)), &ctx);
        assert!((app.camera.pan - before.pan * 0.5).length() < 1e-4);
        assert!(!app.document.is_dirty());
        assert_eq!(app.document.history_counts(), (0, 0));
    }
}

#[test]
fn cameras_metadata_edits_preserve_sculpt_history_and_invalid_edits_are_atomic() {
    let (mut app, ctx) = fixture();
    app.document.invert_mask().unwrap();
    app.document.undo();
    let history = app.document.history_counts();
    let geometry = app.document.geometry_snapshot();
    app.dispatch(Command::Camera(C::Save("  Detail  ".into())), &ctx);
    app.dispatch(Command::Camera(C::Save("Front".into())), &ctx);
    assert_eq!(app.document.saved_cameras()[0].name, "Detail");
    assert!(app.document.is_dirty());
    app.document.mark_saved("views.clay");
    for command in [
        C::Save("Detail".into()),
        C::Rename(1, "Detail".into()),
        C::Rename(0, " ".into()),
        C::Delete(99),
        C::Update(99),
        C::Recall(99),
        C::Roll(f32::NAN),
    ] {
        let cameras = app.document.saved_cameras().to_vec();
        let camera = app.camera;
        app.dispatch(Command::Camera(command), &ctx);
        assert!(!app.ui.cameras.notice.is_empty());
        assert_eq!(app.document.saved_cameras(), cameras);
        assert_eq!(app.camera, camera);
        assert!(!app.document.is_dirty());
    }
    app.dispatch(Command::Camera(C::Update(0)), &ctx);
    assert!(!app.document.is_dirty(), "identical update is a no-op");
    app.camera.roll_by(0.5);
    app.dispatch(Command::Camera(C::Update(0)), &ctx);
    assert!(app.document.is_dirty());
    assert_eq!(app.document.saved_cameras()[0].view.roll, app.camera.roll);
    app.dispatch(Command::Camera(C::Rename(0, "Back".into())), &ctx);
    app.dispatch(Command::Camera(C::Delete(1)), &ctx);
    assert_eq!(app.document.saved_cameras().len(), 1);
    assert_eq!(app.document.saved_cameras()[0].name, "Back");
    assert_eq!(app.document.history_counts(), history);
    assert_eq!(app.document.geometry_snapshot(), geometry);
    app.document.redo();
    assert_eq!(app.document.saved_cameras()[0].name, "Back");
}

#[test]
fn cameras_save_reopen_recover_and_legacy_project_compatibility() {
    let (mut app, ctx) = fixture();
    app.dispatch(Command::Camera(C::Save("Portrait".into())), &ctx);
    let project = app.document.project();
    let path = std::env::temp_dir().join(format!("clay-camera-{}.clay", std::process::id()));
    persistence::save_project(path.to_str().unwrap(), &project).unwrap();
    let loaded = persistence::load_project(path.to_str().unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut reopened = Document::default();
    reopened
        .replace_project(loaded.clone(), "views.clay")
        .unwrap();
    assert_eq!(reopened.saved_cameras(), app.document.saved_cameras());
    assert!(!reopened.is_dirty());
    reopened.new_sphere();
    assert!(reopened.saved_cameras().is_empty());
    reopened.recover_project(loaded).unwrap();
    assert_eq!(reopened.saved_cameras(), app.document.saved_cameras());
    assert!(reopened.is_dirty());
    let mut legacy = serde_json::to_value(project).unwrap();
    legacy.as_object_mut().unwrap().remove("saved_cameras");
    let legacy: crate::sculpt::Project = serde_json::from_value(legacy).unwrap();
    legacy.validate().unwrap();
    assert!(legacy.saved_cameras.is_empty());
}

#[test]
fn cameras_reject_invalid_persisted_views_and_excessive_library() {
    let (mut app, ctx) = fixture();
    app.dispatch(Command::Camera(C::Save("Detail".into())), &ctx);
    let project = app.document.project();
    for invalid in 0..8 {
        let mut bad = project.clone();
        let camera = &mut bad.saved_cameras[0];
        match invalid {
            0 => camera.view.roll = f32::NAN,
            1 => camera.view.zoom = 0.,
            2 => camera.view.pan[1] = f32::INFINITY,
            3 => camera.view.field_of_view = 120.,
            4 => camera.view.perspective_radius = -1.,
            5 => camera.name = "\n".into(),
            6 => bad.saved_cameras.push(bad.saved_cameras[0].clone()),
            _ => bad.saved_cameras = vec![bad.saved_cameras[0].clone(); 129],
        }
        assert!(app.document.replace_project(bad, "invalid.clay").is_err());
        assert!(app.document.project() == project);
    }
}

#[test]
fn cameras_recall_cancels_unfinished_selection_and_respects_modal_policy() {
    let (mut app, ctx) = fixture();
    app.dispatch(Command::Camera(C::Save("Detail".into())), &ctx);
    let saved = app.camera;
    app.camera.roll = 0.;
    let project = app.document.project();
    app.ui.pending = Some(crate::command::PendingAction::New);
    app.dispatch(Command::Camera(C::Recall(0)), &ctx);
    assert_eq!(app.camera.roll, 0.);
    app.ui.pending = None;
    app.viewport.session.gesture = Some(Gesture::Select);
    app.dispatch(Command::Camera(C::Recall(0)), &ctx);
    assert_eq!(app.camera.roll, saved.roll);
    assert!(app.viewport.session.gesture.is_none());
    assert!(app.document.project() == project);
    let (mut app, ctx, _) = super::path_brush::setup();
    navigation_frame(&mut app, &ctx, vec![]);
    app.camera.roll = 0.7;
    app.dispatch(Command::Path(crate::command::PathCommand::Preview), &ctx);
    super::path_brush::finish_preview(&mut app);
    let preview = app.paths.preview_mesh.clone();
    let draft = app.viewport.paths.draft.clone();
    let project = app.document.project();
    app.dispatch(Command::Action(Action::SavedCameras), &ctx);
    assert!(app.ui.cameras.open);
    app.dispatch(Command::Action(Action::ResetRoll), &ctx);
    assert_eq!(app.camera.roll, 0.);
    assert!(app.viewport.paths.active && app.paths.has_preview());
    assert_eq!(app.viewport.paths.draft, draft);
    assert_eq!(app.paths.preview_mesh, preview);
    assert!(app.document.project() == project);
}

#[test]
fn cameras_real_controls_save_recall_update_rename_delete_and_reset() {
    use super::viewport_controls::{center, click, frame, settle};
    let (mut app, ctx) = fixture();
    let geometry = app.document.geometry_snapshot();
    let open = center(&settle(&mut app, &ctx), "Cameras…");
    click(&mut app, &ctx, open);
    assert!(app.ui.cameras.open);
    let save = center(&settle(&mut app, &ctx), "Save view");
    click(&mut app, &ctx, save);
    assert_eq!(app.document.saved_cameras().len(), 1);
    let original = app.document.saved_cameras()[0].view;
    app.camera.roll = -0.5;
    let recall = center(&settle(&mut app, &ctx), "Camera 1");
    click(&mut app, &ctx, recall);
    assert_eq!(app.camera.roll, original.roll);
    let reset = center(&settle(&mut app, &ctx), "Reset roll");
    click(&mut app, &ctx, reset);
    assert_eq!(app.camera.roll, 0.);
    let update = center(&settle(&mut app, &ctx), "Update from view");
    click(&mut app, &ctx, update);
    assert_eq!(app.document.saved_cameras()[0].view, original);
    let keep = center(&settle(&mut app, &ctx), "Keep saved view");
    click(&mut app, &ctx, keep);
    assert_eq!(app.document.saved_cameras()[0].view, original);
    let update = center(&settle(&mut app, &ctx), "Update from view");
    click(&mut app, &ctx, update);
    let replace = center(&settle(&mut app, &ctx), "Replace saved view");
    click(&mut app, &ctx, replace);
    assert_eq!(app.document.saved_cameras()[0].view.roll, 0.);
    let rename = center(&settle(&mut app, &ctx), "Rename");
    click(&mut app, &ctx, rename);
    let output = settle(&mut app, &ctx);
    let field = output
        .shapes
        .iter()
        .rev()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Camera 1" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    click(&mut app, &ctx, field);
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("Portrait".into()),
        ],
    );
    let apply = center(&settle(&mut app, &ctx), "Apply name");
    click(&mut app, &ctx, apply);
    assert_eq!(app.document.saved_cameras()[0].name, "Portrait");
    let delete = center(&settle(&mut app, &ctx), "Delete");
    click(&mut app, &ctx, delete);
    let keep = center(&settle(&mut app, &ctx), "Keep camera");
    click(&mut app, &ctx, keep);
    assert_eq!(app.document.saved_cameras().len(), 1);
    let delete = center(&settle(&mut app, &ctx), "Delete");
    click(&mut app, &ctx, delete);
    let confirm = center(&settle(&mut app, &ctx), "Delete camera");
    click(&mut app, &ctx, confirm);
    assert!(app.document.saved_cameras().is_empty());
    assert_eq!(app.document.geometry_snapshot(), geometry);
    assert_eq!(app.document.history_counts(), (0, 0));
}
