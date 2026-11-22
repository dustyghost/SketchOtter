use super::*;
use crate::preferences::{BuiltinMenu, PinnedMenu};

fn fixture() -> App {
    let mut app = App::default();
    let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Plane);
    p.divisions = [4, 2, 1];
    app.document
        .replace_geometry(crate::sculpt::Geometry::new(p.mesh().unwrap()))
        .unwrap();
    app.document.select_face_region(|p| p.0 > 0., true).unwrap();
    app.ui.preferences.pinned_menus.push(PinnedMenu {
        menu: BuiltinMenu::Visibility,
        position: [20., 90.],
    });
    app.camera.yaw = 0.;
    app.camera.pitch = 0.;
    app
}

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    size: Vec2,
    events: Vec<egui::Event>,
    capture: &mut Option<crate::viewport::UiCapture>,
) -> egui::FullOutput {
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| app.workspace_ui(ctx),
    );
    if let Some(capture) = capture {
        capture.render(ctx, output.clone());
    }
    output
}

fn point(output: &egui::FullOutput, label: &str, size: Vec2) -> Pos2 {
    fn find(shape: &egui::Shape, clip: egui::Rect, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let p = t.pos + t.galley.size() * 0.5;
                clip.contains(p).then_some(p)
            }
            egui::Shape::Vec(shapes) => shapes.iter().rev().find_map(|s| find(s, clip, label)),
            _ => None,
        }
    }
    let position = output
        .shapes
        .iter()
        .rev()
        .find_map(|s| find(&s.shape, s.clip_rect, label))
        .unwrap_or_else(|| {
            for s in &output.shapes {
                if let egui::Shape::Text(t) = &s.shape {
                    eprintln!(
                        "Text {:?} at {:?}, clip {:?}",
                        t.galley.text(),
                        t.pos,
                        s.clip_rect
                    );
                }
            }
            panic!("Missing reachable control: {label} at {size:?}");
        });
    assert!(egui::Rect::from_min_size(Pos2::ZERO, size).contains(position));
    position
}

fn click(
    app: &mut App,
    ctx: &egui::Context,
    size: Vec2,
    position: Pos2,
    capture: &mut Option<crate::viewport::UiCapture>,
) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            size,
            vec![
                egui::Event::PointerMoved(position),
                primary_event(position, pressed),
            ],
            capture,
        );
    }
}

fn exercise_controls(gpu: bool) {
    for dimensions in [[1000, 800], [640, 480]] {
        for split in [true, false] {
            let size = Vec2::new(dimensions[0] as f32, dimensions[1] as f32);
            let ctx = egui::Context::default();
            ctx.style_mut(|s| s.animation_time = 0.);
            let mut app = fixture();
            app.ui.preferences.compact_toolbar = dimensions[0] < 800;
            let mut capture = gpu.then(|| crate::viewport::UiCapture::new(dimensions));
            if let Some(capture) = &capture {
                app.viewport.initialize_renderer(&capture.state).unwrap();
            }
            for _ in 0..4 {
                frame(&mut app, &ctx, size, vec![], &mut capture);
            }
            let output = frame(&mut app, &ctx, size, vec![], &mut capture);
            if let Some(capture) = &mut capture {
                let path = std::env::var("CLAY_GUIDE_PREVIEW")
                    .unwrap_or_else(|_| "/tmp/clay-visibility-review".into());
                std::fs::create_dir_all(&path).unwrap();
                let rgba = capture.render(&ctx, output.clone());
                capture.save(
                    std::path::Path::new(&path).join(format!("menu-{}.png", dimensions[0])),
                    &rgba,
                );
            }
            let label = if split {
                "Split hidden"
            } else {
                "Delete hidden"
            };
            let before = app.document.geometry_snapshot();
            let history = app.document.history_counts();
            click(
                &mut app,
                &ctx,
                size,
                point(&output, label, size),
                &mut capture,
            );
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![], &mut capture);
            }
            assert!(app.ui.hidden_faces_warning.is_some());
            assert_eq!(app.document.geometry_snapshot(), before);
            assert_eq!(app.document.history_counts(), history);
            let output = frame(&mut app, &ctx, size, vec![], &mut capture);
            if let Some(capture) = &mut capture {
                let path = std::env::var("CLAY_GUIDE_PREVIEW")
                    .unwrap_or_else(|_| "/tmp/clay-visibility-review".into());
                std::fs::create_dir_all(&path).unwrap();
                let rgba = capture.render(&ctx, output.clone());
                capture.save(
                    std::path::Path::new(&path).join(format!(
                        "{}-{}.png",
                        if split { "split" } else { "delete" },
                        dimensions[0]
                    )),
                    &rgba,
                );
            }
            click(
                &mut app,
                &ctx,
                size,
                point(&output, "Cancel", size),
                &mut capture,
            );
            assert!(app.ui.hidden_faces_warning.is_none());
            assert_eq!(app.document.geometry_snapshot(), before);
            assert_eq!(app.document.history_counts(), history);
            // Escape follows the same cancellation path without leaking a key
            // or pointer event into sculpting or history.
            app.dispatch(
                if split {
                    Command::SplitHidden
                } else {
                    Command::DeleteHidden
                },
                &ctx,
            );
            frame(&mut app, &ctx, size, vec![], &mut capture);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    &mut capture,
                );
            }
            assert!(app.ui.hidden_faces_warning.is_none());
            assert_eq!(app.document.geometry_snapshot(), before);
            assert_eq!(app.document.history_counts(), history);
            let output = frame(&mut app, &ctx, size, vec![], &mut capture);
            click(
                &mut app,
                &ctx,
                size,
                point(&output, label, size),
                &mut capture,
            );
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![], &mut capture);
            }
            let output = frame(&mut app, &ctx, size, vec![], &mut capture);
            click(
                &mut app,
                &ctx,
                size,
                point(
                    &output,
                    if split {
                        "Apply split"
                    } else {
                        "Delete hidden faces"
                    },
                    size,
                ),
                &mut capture,
            );
            assert!(app.ui.hidden_faces_warning.is_none());
            assert_eq!(app.document.history_counts().0, history.0 + 1);
            assert_eq!(
                app.document.mesh().triangles.len(),
                before.mesh.triangles.len() / 2
            );
            assert_eq!(app.document.model_count(), if split { 2 } else { 1 });
            let after = app.document.geometry_snapshot();
            app.dispatch(Command::Undo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), before);
            app.dispatch(Command::Redo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), after);
            if gpu {
                app.viewport.destroy_renderer();
            }
        }
    }
}

#[test]
fn visibility_edit_controls_apply_cancel_and_undo_in_compact_windows() {
    exercise_controls(false);
}

#[test]
#[ignore = "real visibility controls and visual review; requires Vulkan"]
fn visibility_edits_offscreen_review() {
    exercise_controls(true);
}

#[test]
fn visibility_edit_search_shortcuts_and_stale_confirmation() {
    for (action, command) in [
        (Action::SplitHidden, Command::SplitHidden),
        (Action::DeleteHidden, Command::DeleteHidden),
    ] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        let size = Vec2::new(1000., 800.);
        let mut capture = None;
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], &mut capture);
        }
        app.execute_action(Action::Search, &ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx, size, vec![], &mut capture);
        }
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Text(action.label().into())],
            &mut capture,
        );
        frame(
            &mut app,
            &ctx,
            size,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut capture,
        );
        assert!(
            app.ui.hidden_faces_warning.is_some(),
            "search must run {action:?}"
        );
        app.dispatch(Command::ConfirmHiddenFaces(false), &ctx);
        app.ui
            .preferences
            .bind(
                action.clone(),
                Shortcut::new(egui::Key::F8, egui::Modifiers::ALT),
            )
            .unwrap();
        let restored: Preferences =
            serde_json::from_str(&serde_json::to_string(&app.ui.preferences).unwrap()).unwrap();
        assert_eq!(restored.shortcut(&action).unwrap().key, "F8");
        app.dispatch(command, &ctx);
        assert!(app.ui.hidden_faces_warning.is_some());
        let before = app.document.geometry_snapshot();
        app.dispatch(Command::InvertMask, &ctx);
        assert_eq!(
            app.document.geometry_snapshot(),
            before,
            "modal must block unrelated edits"
        );
        app.document.invert_mask().unwrap();
        let changed = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        app.dispatch(Command::ConfirmHiddenFaces(true), &ctx);
        assert_eq!(app.document.geometry_snapshot(), changed);
        assert_eq!(app.document.history_counts(), history);
        assert!(app.ui.status.contains("model changed"));
    }
}

#[test]
fn visibility_edit_policy_reports_actual_prerequisites() {
    let mut app = fixture();
    for action in [Action::SplitHidden, Action::DeleteHidden] {
        assert!(app.ui.action_enabled(&action, &app.document));
    }
    app.document.set_temporary_thickness(0.1).unwrap();
    assert!(
        app.ui
            .action_unavailable(&Action::SplitHidden, &app.document)
            .unwrap()
            .contains("Thickness")
    );
    app.document.set_temporary_thickness(0.).unwrap();
    app.document.show_all_faces().unwrap();
    assert!(
        app.ui
            .action_unavailable(&Action::DeleteHidden, &app.document)
            .unwrap()
            .contains("Hide some")
    );
    app.document.select_face_region(|_| true, true).unwrap();
    assert!(
        app.ui
            .action_unavailable(&Action::SplitHidden, &app.document)
            .unwrap()
            .contains("Keep some")
    );
    assert!(
        app.ui
            .action_unavailable(&Action::GroupAll, &app.document)
            .is_some()
    );
}
