use super::*;
use crate::sculpt::{Geometry, GroupPurpose, Primitive, PrimitiveKind, VertexGroup};
fn setup() -> (App, egui::Context) {
    let (mut app, ctx) = (App::default(), egui::Context::default());
    let mut primitive = Primitive::for_kind(PrimitiveKind::Grid);
    primitive.divisions = [4, 4, 1];
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
            [if q.iter().map(|&v| mesh.vertices[v].0).sum::<f32>() < 0. {
                1
            } else {
                2
            }; 2]
        })
        .collect();
    app.document.replace_geometry(Geometry::new(mesh)).unwrap();
    app.document.mark_saved("panel-source.clay");
    (app, ctx)
}
fn finish(app: &mut App) {
    crate::app::test_support::finish_projection(app);
}
#[test]
fn panels_apply_matches_preview_source_survives_undo_redo_and_save_reopen() {
    let (mut app, ctx) = setup();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    app.dispatch(Command::OpenPanels, &ctx);
    assert!(app.ui.projection.open, "{}", app.ui.status);
    app.ui.projection.panels.groups = vec![1, 2];
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::SelectSubmodel(1), &ctx);
    finish(&mut app);
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
    let draft = app
        .projection
        .draft
        .as_ref()
        .expect(&app.ui.status)
        .panels
        .as_ref()
        .unwrap();
    let expected: Vec<_> = draft.panels.iter().map(|p| p.mesh.clone()).collect();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.model_count(), 3, "{}", app.ui.status);
    assert_eq!(app.document.undo_label(), Some("Generate panels"));
    assert_eq!(app.document.history_counts().0, history.0 + 1);
    for (i, mesh) in expected.iter().enumerate() {
        assert_eq!(app.document.model(i).unwrap().mesh, mesh);
    }
    let source = app.document.model(2).unwrap();
    assert_eq!(source.mesh, &before.mesh);
    assert_eq!(source.settings, &before.settings);
    assert!(!source.metadata.visible);
    let after = app.document.geometry_snapshot();
    let encoded = serde_json::to_string(&app.document.project()).unwrap();
    let mut loaded = Document::default();
    loaded
        .replace_project(serde_json::from_str(&encoded).unwrap(), "panels.clay")
        .unwrap();
    assert_eq!(loaded.geometry_snapshot(), after);
    app.document.undo();
    assert_eq!(app.document.geometry_snapshot(), before);
    app.document.redo();
    assert_eq!(app.document.geometry_snapshot(), after);
}
#[test]
fn panels_cancel_invalid_stale_settings_and_scene_leave_document_untouched() {
    let (mut app, ctx) = setup();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    app.dispatch(Command::OpenPanels, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::CancelProjection, &ctx);
    assert!(!app.ui.projection.open);
    app.poll_projection();
    assert_eq!(app.document.geometry_snapshot(), before);
    app.dispatch(Command::OpenPanels, &ctx);
    app.ui.projection.panels.options.border.width = 10.;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish(&mut app);
    assert!(app.projection.draft.is_none());
    assert!(app.ui.projection.error.is_some());
    app.ui.projection.panels.options = Default::default();
    app.dispatch(Command::PreviewProjection, &ctx);
    finish(&mut app);
    assert!(app.projection.draft.is_some(), "{}", app.ui.status);
    app.ui.projection.panels.options.offset += 0.1;
    app.dispatch(Command::ApplyProjection, &ctx);
    assert!(
        app.ui.status.contains("settings changed"),
        "{}",
        app.ui.status
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
    app.dispatch(Command::PreviewProjection, &ctx);
    finish(&mut app);
    app.document.duplicate_model(false).unwrap();
    let changed = app.document.geometry_snapshot();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.geometry_snapshot(), changed);
    assert!(app.ui.status.contains("scene changed"), "{}", app.ui.status);
}
#[test]
fn panels_worker_discards_changed_settings_and_preview_navigation_cannot_sculpt() {
    let (mut app, ctx) = setup();
    let before = app.document.geometry_snapshot();
    app.dispatch(Command::OpenPanels, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    app.ui.projection.panels.groups = vec![2];
    finish(&mut app);
    assert!(app.projection.draft.is_none());
    app.dispatch(Command::PreviewProjection, &ctx);
    finish(&mut app);
    assert!(app.projection.draft.is_some(), "{}", app.ui.status);
    let rect = navigation_frame(&mut app, &ctx, vec![]);
    let p = rect.center();
    let yaw = app.camera.yaw;
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(p), primary_event(p, true)],
    );
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(p + Vec2::new(40., 10.))],
    );
    navigation_frame(
        &mut app,
        &ctx,
        vec![primary_event(p + Vec2::new(40., 10.), false)],
    );
    assert_ne!(app.camera.yaw, yaw);
    assert_eq!(app.document.geometry_snapshot(), before);
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert!(!app.ui.projection.open);
    assert!(app.projection.draft.is_none());
    assert_eq!(app.document.geometry_snapshot(), before);
}

#[test]
fn panels_mouse_and_native_pen_select_groups_preview_and_cancel() {
    let (mut app, ctx) = setup();
    let before = app.document.geometry_snapshot();
    let mut time = 0.;
    let frame = |app: &mut App, time: &mut f64, events: Vec<egui::Event>| {
        *time += 0.1;
        let mut commands = Vec::new();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1100., 900.),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| {
                commands.extend(app.ui.projection_window(ctx, &app.document, &app.camera));
            },
        );
        for command in commands {
            app.dispatch(command, &ctx);
        }
        output
    };
    fn text_point(shapes: &[egui::epaint::ClippedShape], label: &str) -> Pos2 {
        fn visit(shape: &egui::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| visit(s, label)),
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|s| visit(&s.shape, label))
            .unwrap_or_else(|| panic!("{label} is visible"))
    }
    app.dispatch(Command::OpenPanels, &ctx);
    finish(&mut app);
    assert!(!app.ui.projection.report.as_ref().unwrap().can_apply());
    assert!(
        !app.projection
            .draft
            .as_ref()
            .unwrap()
            .panels
            .as_ref()
            .unwrap()
            .outlines
            .is_empty()
    );
    frame(&mut app, &mut time, vec![]);
    let output = frame(&mut app, &mut time, vec![]);
    let p = text_point(&output.shapes, "Right plate");
    frame(
        &mut app,
        &mut time,
        vec![egui::Event::PointerMoved(p), primary_event(p, true)],
    );
    frame(&mut app, &mut time, vec![primary_event(p, false)]);
    assert_eq!(app.ui.projection.panels.groups, vec![1, 2]);
    finish(&mut app);
    let mut pen = crate::tablet::pen::PenInput::default();
    for label in ["Left plate", "Preview panels", "Cancel"] {
        let output = frame(&mut app, &mut time, vec![]);
        let p = text_point(&output.shapes, label);
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::End] {
            let mut input = egui::RawInput {
                events: vec![egui::Event::Touch {
                    device_id: egui::TouchDeviceId(1),
                    id: egui::TouchId(2),
                    phase,
                    pos: p,
                    force: Some(0.6),
                }],
                ..Default::default()
            };
            pen.route(&mut input, |id| id == 2);
            frame(&mut app, &mut time, input.events);
        }
        finish(&mut app);
        if label == "Left plate" {
            assert_eq!(app.ui.projection.panels.groups, vec![2]);
        }
        if label == "Preview panels" {
            assert!(
                app.ui.projection.report.as_ref().unwrap().can_apply(),
                "{}",
                app.ui.status
            );
        }
    }
    assert!(!app.ui.projection.open);
    assert_eq!(app.document.geometry_snapshot(), before);
}

#[test]
fn panels_seams_outside_selection_do_not_leak_from_retained_source_on_apply() {
    let (mut app, ctx) = setup();
    let mut geometry = app.document.geometry_snapshot();
    geometry.mesh.uv.seams = vec![*geometry.mesh.polygon_edges().last().unwrap()];
    app.document.replace_geometry(geometry).unwrap();
    app.dispatch(Command::OpenPanels, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish(&mut app);
    let expected = app
        .projection
        .draft
        .as_ref()
        .expect(&app.ui.status)
        .panels
        .as_ref()
        .unwrap()
        .panels[0]
        .mesh
        .clone();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.mesh(), &expected);
    app.document.mesh().validate_closed().unwrap();
}
