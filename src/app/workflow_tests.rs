//! Small real-control/input contracts, shared by the default and focused harness.
use super::*;
use crate::sculpt::{Geometry, Primitive, PrimitiveKind, Vec3};
use crate::viewport::{TransformMode, TransformOperation, TransformSpace};
use egui::{Pos2, Vec2};

#[test]
fn workflow_ctrl_edge_removal_has_priority_over_model_taps() {
    let mut app = App::default();
    let ctx = egui::Context::default();
    app.document
        .replace_geometry(Geometry::new(
            Primitive::for_kind(PrimitiveKind::Plane).mesh().unwrap(),
        ))
        .unwrap();
    app.dispatch(Command::Action(Action::SelectEdges), &ctx);
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let rect = app.ui.viewport_menu.viewport_rect.unwrap();
    let scale = rect.width().min(rect.height()) * 0.35;
    let a = rect.center() + Vec2::new(0., -scale);
    let b = rect.center() + Vec2::new(-scale, 0.);
    let before = app.document.geometry_snapshot();
    for (point, modifiers, expected) in [
        (a, egui::Modifiers::NONE, 1),
        (b, egui::Modifiers::SHIFT, 2),
        (a, egui::Modifiers::CTRL, 1),
    ] {
        tap(&mut app, &ctx, point, modifiers, false);
        assert_eq!(app.ui.edges.selected.len(), expected);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.camera.zoom, 1.);
    }
}

#[test]
fn workflow_persistent_lasso_keeps_its_shape_and_commits_only_the_outline() {
    use crate::viewport::SelectionTool;
    for tool in [SelectionTool::MaskLasso, SelectionTool::Lasso] {
        for degenerate in [false, true] {
            let mut app = App::default();
            let ctx = egui::Context::default();
            app.dispatch(Command::SelectTool(Some(tool)), &ctx);
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let before = app.document.geometry_snapshot();
            let modifiers = if tool == SelectionTool::Lasso {
                egui::Modifiers::ALT
            } else {
                egui::Modifiers::NONE
            };
            let a = Pos2::new(500., 250.);
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(a), button(a, true, modifiers)],
                modifiers,
            );
            let points = if degenerate {
                vec![Pos2::new(700., 600.)]
            } else {
                vec![
                    Pos2::new(700., 250.),
                    Pos2::new(700., 600.),
                    Pos2::new(500., 600.),
                    a,
                ]
            };
            let end = *points.last().unwrap();
            frame(
                &mut app,
                &ctx,
                points.into_iter().map(egui::Event::PointerMoved).collect(),
                modifiers,
            );
            assert_eq!(app.document.geometry_snapshot(), before);
            frame(
                &mut app,
                &ctx,
                vec![button(end, false, modifiers)],
                modifiers,
            );
            if degenerate {
                assert_eq!(app.document.geometry_snapshot(), before);
                assert_eq!(app.document.history_counts(), (0, 0));
            } else {
                if tool == SelectionTool::Lasso {
                    let mesh = app.document.mesh();
                    assert!(mesh.hidden_face_count() > 0);
                    assert!(mesh.hidden_face_count() < mesh.triangles.len());
                } else {
                    let mask = &app.document.mesh().mask;
                    assert!(mask.contains(&1.) && mask.contains(&0.));
                }
                roundtrip(&mut app, &before, 0);
            }
        }
    }
}

#[test]
fn workflow_default_mask_hold_paints_before_opening_z_and_restores_selection() {
    use crate::viewport::SelectionTool;
    for tool in [
        None,
        Some(SelectionTool::Visibility(
            preferences::VisibilitySettings::default(),
        )),
        Some(SelectionTool::Lasso),
    ] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        assert_eq!(
            app.ui.preferences.selection_mode,
            preferences::SelectionMode::Mask
        );
        assert_eq!(
            app.ui.preferences.mask_preset.shape,
            preferences::MaskShape::Brush
        );
        app.ui.selection_tool = tool;
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.mesh().clone();
        let camera = (
            app.camera.yaw,
            app.camera.pitch,
            app.camera.zoom,
            app.camera.pan,
        );
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let a = Pos2::new(500., 400.);
        let b = a + Vec2::new(30., 0.);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(a), button(a, true, modifiers)],
            modifiers,
        );
        assert!(app.ui.mask_active);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(b)],
            modifiers,
        );
        frame(&mut app, &ctx, vec![button(b, false, modifiers)], modifiers);
        assert_ne!(app.document.mesh().mask, before.mask, "{tool:?}");
        assert_eq!(app.document.mesh().vertices, before.vertices);
        assert_eq!(app.document.mesh().hidden_faces, before.hidden_faces);
        assert_eq!(
            (
                app.camera.yaw,
                app.camera.pitch,
                app.camera.zoom,
                app.camera.pan
            ),
            camera
        );
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        assert!(!app.ui.mask_active);
        assert_eq!(app.ui.selection_tool, tool);
        assert!(app.ui.viewport_menu.context.is_none());
    }
    // Defaults apply to missing preferences, not deliberate saved choices.
    let saved: preferences::Preferences =
        serde_json::from_str(r#"{"selection_mode":"Visibility","mask_preset":{"shape":"Lasso"}}"#)
            .unwrap();
    assert_eq!(saved.selection_mode, preferences::SelectionMode::Visibility);
    assert_eq!(saved.mask_preset.shape, preferences::MaskShape::Lasso);
}

#[test]
fn workflow_pen_tip_release_before_pointer_gone_completes_one_tap() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts().0;
    let p = Pos2::new(500., 400.);
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(p), button(p, true, modifiers)],
        modifiers,
    );
    frame(
        &mut app,
        &ctx,
        vec![button(p, false, modifiers), egui::Event::PointerGone],
        modifiers,
    );
    assert!(app.document.mesh().mask.iter().all(|&m| m == 1.));
    roundtrip(&mut app, &before, history);
}

#[test]
fn workflow_tap_threshold_and_same_frame_out_and_back_drag_do_not_conflict() {
    for drag in [false, true] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
        app.ui.preferences.mask_preset.shape = preferences::MaskShape::Rectangle;
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let a = Pos2::new(500., 400.);
        let b = a + Vec2::new(if drag { 100. } else { 1. }, 0.);
        let mut events = vec![
            egui::Event::PointerMoved(a),
            button(a, true, modifiers),
            egui::Event::PointerMoved(b),
        ];
        if drag {
            events.push(egui::Event::PointerMoved(a));
        }
        events.push(button(if drag { a } else { b }, false, modifiers));
        frame(&mut app, &ctx, events, modifiers);
        if drag {
            assert!(app.document.mesh().mask.contains(&0.));
        } else {
            assert!(app.document.mesh().mask.iter().all(|&m| m == 1.));
        }
    }
}

#[test]
fn workflow_text_entry_and_quick_menu_clicks_never_invert_the_surface() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    app.ui.viewport_menu.context = Some(crate::ui::context_menu::ViewportMenuContext::Mask);
    app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
    for _ in 0..5 {
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    }
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("icon_button", "Pin menu open"))))
        .unwrap();
    tap(&mut app, &ctx, rect.center(), modifiers, false);
    assert!(app.ui.viewport_menu.pinned);
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    app.ui.viewport_menu.context = None;
    let mut text = String::new();
    for pressed in [true, false] {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1000., 800.),
                )),
                modifiers,
                events: vec![
                    egui::Event::PointerMoved(Pos2::new(500., 400.)),
                    button(Pos2::new(500., 400.), pressed, modifiers),
                ],
                ..Default::default()
            },
            |ctx| {
                egui::Window::new("Workflow text input").show(ctx, |ui| {
                    ui.text_edit_singleline(&mut text).request_focus();
                });
                egui::CentralPanel::default().show(ctx, |ui| app.show_viewport(ui));
            },
        );
    }
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
}

fn fixture() -> App {
    let mut app = App::default();
    let mut p = Primitive::for_kind(PrimitiveKind::Plane);
    p.divisions = [4, 2, 1];
    app.document
        .replace_geometry(Geometry::new(p.mesh().unwrap()))
        .unwrap();
    app
}
fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}
fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> egui::FullOutput {
    frame_size(app, ctx, events, modifiers, Vec2::new(1000., 800.))
}
fn frame_size(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    size: Vec2,
) -> egui::FullOutput {
    ctx.style_mut(|s| s.animation_time = 0.);
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            focused: !events
                .iter()
                .any(|e| matches!(e, egui::Event::WindowFocused(false))),
            events,
            modifiers,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.show_viewport(ui));
        },
    )
}

#[test]
fn workflow_quick_menu_pin_clicks_are_shared_visible_and_do_not_edit() {
    use crate::ui::context_menu::ViewportMenuContext as Menu;
    for size in [Vec2::new(1000., 800.), Vec2::new(640., 480.)] {
        for menu in [
            Menu::Modeling,
            Menu::Mask,
            Menu::Visibility,
            Menu::Slice,
            Menu::Components,
            Menu::Transform,
        ] {
            let mut app = fixture();
            let ctx = egui::Context::default();
            app.ui.viewport_menu.context = Some(menu);
            app.ui.viewport_menu.just_opened = true;
            app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
            let before = app.document.geometry_snapshot();
            let history = app.document.history_counts();
            frame_size(&mut app, &ctx, vec![], egui::Modifiers::NONE, size);
            frame_size(&mut app, &ctx, vec![], egui::Modifiers::NONE, size);
            for _ in 0..5 {
                frame_size(&mut app, &ctx, vec![], egui::Modifiers::NONE, size);
            }
            #[cfg(target_os = "linux")]
            if menu == Menu::Modeling
                && size.x == 640.
                && let Some(dir) = std::env::var_os("CLAY_WORKFLOW_CAPTURES")
            {
                let mut capture = crate::viewport::UiCapture::new([640, 480]);
                let visual_ctx = egui::Context::default();
                let mut visual_app = fixture();
                visual_app.ui.viewport_menu.context = Some(menu);
                visual_app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
                for _ in 0..6 {
                    let output = frame_size(
                        &mut visual_app,
                        &visual_ctx,
                        vec![],
                        egui::Modifiers::NONE,
                        size,
                    );
                    let rgba = capture.render(&visual_ctx, output);
                    capture.save(
                        std::path::PathBuf::from(&dir).join("modeling-compact.png"),
                        &rgba,
                    );
                }
            }
            // Record actual button hit rectangles, not label/geometry guesses.
            for (label, expected) in [("Pin menu open", true), ("Unpin menu", false)] {
                let rect = ctx
                    .data(|d| d.get_temp::<egui::Rect>(egui::Id::new(("icon_button", label))))
                    .unwrap();
                assert!(rect.width() >= 32. && rect.height() >= 32.);
                assert!(ctx.content_rect().contains_rect(rect), "{menu:?}: {rect:?}");
                for pressed in [true, false] {
                    frame_size(
                        &mut app,
                        &ctx,
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            button(rect.center(), pressed, egui::Modifiers::NONE),
                        ],
                        egui::Modifiers::NONE,
                        size,
                    );
                }
                assert_eq!(
                    app.ui.viewport_menu.pinned, expected,
                    "{menu:?} size={size:?}, button={rect:?}, menu={:?}, context={:?}",
                    app.ui.viewport_menu.rect, app.ui.viewport_menu.context
                );
                let output = frame_size(&mut app, &ctx, vec![], egui::Modifiers::NONE, size);
                assert!(
                    app.ui.viewport_menu.context.is_some(),
                    "{menu:?}, size={size:?}, pinned={expected}"
                );
                text_point(&output, if expected { "Pinned" } else { "Quick menu" });
                assert_eq!(app.document.geometry_snapshot(), before);
                assert_eq!(app.document.history_counts(), history);
                if expected {
                    let p = Pos2::new(size.x - 200., size.y - 140.);
                    assert!(!app.ui.viewport_menu.rect.unwrap().contains(p));
                    let pan = app.camera.pan;
                    let middle = |pos, pressed| egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Middle,
                        modifiers: egui::Modifiers::NONE,
                    };
                    frame_size(
                        &mut app,
                        &ctx,
                        vec![egui::Event::PointerMoved(p), middle(p, true)],
                        egui::Modifiers::NONE,
                        size,
                    );
                    frame_size(
                        &mut app,
                        &ctx,
                        vec![egui::Event::PointerMoved(p + Vec2::new(15., 10.))],
                        egui::Modifiers::NONE,
                        size,
                    );
                    frame_size(
                        &mut app,
                        &ctx,
                        vec![middle(p + Vec2::new(15., 10.), false)],
                        egui::Modifiers::NONE,
                        size,
                    );
                    assert_ne!(app.camera.pan, pan);
                    assert_eq!(app.ui.viewport_menu.context, Some(menu));
                    assert_eq!(app.document.history_counts(), history);
                }
                #[cfg(target_os = "linux")]
                if expected
                    && menu == Menu::Transform
                    && let Some(dir) = std::env::var_os("CLAY_WORKFLOW_CAPTURES")
                {
                    let mut capture =
                        crate::viewport::UiCapture::new([size.x as i32, size.y as i32]);
                    let visual_ctx = egui::Context::default();
                    let mut visual_app = fixture();
                    visual_app.ui.viewport_menu.context = Some(menu);
                    visual_app.ui.viewport_menu.pinned = true;
                    visual_app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
                    let first = frame_size(
                        &mut visual_app,
                        &visual_ctx,
                        vec![],
                        egui::Modifiers::NONE,
                        size,
                    );
                    capture.render_without_readback(&visual_ctx, first);
                    let output = frame_size(
                        &mut visual_app,
                        &visual_ctx,
                        vec![],
                        egui::Modifiers::NONE,
                        size,
                    );
                    let rgba = capture.render(&visual_ctx, output);
                    capture.save(
                        std::path::PathBuf::from(dir).join(format!("pin-{}.png", size.x)),
                        &rgba,
                    );
                }
            }
            for (key, label, pinned) in [
                (egui::Key::Space, "Pin menu open", true),
                (egui::Key::Enter, "Unpin menu", false),
            ] {
                let id = ctx
                    .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("icon_button_id", label))))
                    .unwrap();
                ctx.memory_mut(|m| m.request_focus(id));
                frame_size(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    egui::Modifiers::NONE,
                    size,
                );
                assert_eq!(app.ui.viewport_menu.pinned, pinned, "{key:?}, {menu:?}");
                assert_eq!(app.document.history_counts(), history);
            }
            let event = egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            frame_size(&mut app, &ctx, vec![event], egui::Modifiers::NONE, size);
            assert!(app.ui.viewport_menu.context.is_none());
        }
    }
}
fn tap(app: &mut App, ctx: &egui::Context, pos: Pos2, modifiers: egui::Modifiers, fast: bool) {
    if fast {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                button(pos, true, modifiers),
                button(pos, false, modifiers),
            ],
            modifiers,
        );
    } else {
        frame(
            app,
            ctx,
            vec![egui::Event::PointerMoved(pos), button(pos, true, modifiers)],
            modifiers,
        );
        frame(app, ctx, vec![button(pos, false, modifiers)], modifiers);
    }
}
fn text_point(output: &egui::FullOutput, label: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .rev()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Control not visible: {label}"))
}
fn roundtrip(app: &mut App, before: &Geometry, count: usize) {
    let after = app.document.geometry_snapshot();
    assert_eq!(app.document.history_counts().0, count + 1);
    app.document.undo();
    assert_eq!(&app.document.geometry_snapshot(), before);
    app.document.redo();
    assert_eq!(app.document.geometry_snapshot(), after);
    let project =
        serde_json::from_str(&serde_json::to_string(&app.document.project()).unwrap()).unwrap();
    let mut reopened = Document::default();
    reopened.replace_project(project, "split.clay").unwrap();
    assert_eq!(reopened.geometry_snapshot(), after);
}

#[test]
fn workflow_modifier_taps_invert_once_preserve_soft_masks_and_history() {
    for visibility in [false, true] {
        for fast in [false, true] {
            let mut app = fixture();
            let ctx = egui::Context::default();
            let mut geometry = app.document.geometry_snapshot();
            geometry.mesh.mask = geometry
                .mesh
                .vertices
                .iter()
                .map(|p| (p.0 + 0.5).clamp(0., 1.))
                .collect();
            app.document.replace_geometry(geometry).unwrap();
            app.ui.preferences.selection_mode = if visibility {
                preferences::SelectionMode::Visibility
            } else {
                preferences::SelectionMode::Mask
            };
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let before = app.document.geometry_snapshot();
            let count = app.document.history_counts().0;
            tap(
                &mut app,
                &ctx,
                Pos2::new(500., 400.),
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                fast,
            );
            if visibility {
                assert_eq!(
                    app.document.mesh().hidden_face_count(),
                    before.mesh.triangles.len()
                );
            } else {
                for (&old, &new) in before.mesh.mask.iter().zip(&app.document.mesh().mask) {
                    assert!((new - (1. - old)).abs() < 1e-6);
                }
            }
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            roundtrip(&mut app, &before, count);
        }
    }
}

#[test]
fn workflow_ctrl_click_selects_frontmost_in_sculpt_modeling_mask_slice_and_transform() {
    for context in 0..5 {
        for fast in [false, true] {
            let mut app = fixture();
            let ctx = egui::Context::default();
            app.document.duplicate_model(false).unwrap();
            app.document.select_submodel(0).unwrap();
            match context {
                1 => app.dispatch(Command::ModelingMode(true), &ctx),
                2 => app.ui.preferences.selection_mode = preferences::SelectionMode::Mask,
                3 => app.dispatch(Command::SliceTool(true), &ctx),
                4 => app.dispatch(Command::TransformMode(TransformMode::World), &ctx),
                _ => {}
            }
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let history = app.document.history_counts();
            let vertices = app.document.mesh().vertices.clone();
            // Overlapping models: frontmost tie follows the existing picker.
            // Move the inactive model towards the camera to remove the tie.
            app.document.select_submodel(1).unwrap();
            app.document
                .apply_transform(
                    crate::sculpt::Transform {
                        translation: Vec3(0., 0., 0.2),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            app.document.end_stroke();
            app.document.select_submodel(0).unwrap();
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let count = app.document.history_counts();
            // Avoid the active transform's centre and axis handles.
            tap(
                &mut app,
                &ctx,
                Pos2::new(540., 435.),
                egui::Modifiers::CTRL,
                fast,
            );
            assert_eq!(
                app.document.active_model_index(),
                1,
                "context {context}, fast {fast}"
            );
            // Reuse the existing undoable document/panel selection transaction.
            let selected_count = app.document.history_counts();
            assert_eq!(selected_count.0, count.0 + 1);
            tap(
                &mut app,
                &ctx,
                Pos2::new(700., 650.),
                egui::Modifiers::CTRL,
                fast,
            );
            assert_eq!(app.document.active_model_index(), 1);
            assert_eq!(app.document.history_counts(), selected_count);
            assert_eq!(app.document.project().submodels[0].mesh.vertices, vertices);
            assert!(history.0 < count.0);
        }
    }
}

#[test]
fn workflow_ctrl_shift_drag_keeps_region_selection_and_never_also_inverts() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
    app.ui.preferences.mask_preset.shape = preferences::MaskShape::Rectangle;
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    let a = Pos2::new(500., 280.);
    let b = Pos2::new(680., 520.);
    let count = app.document.history_counts().0;
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(a), button(a, true, modifiers)],
        modifiers,
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(b)],
        modifiers,
    );
    frame(&mut app, &ctx, vec![button(b, false, modifiers)], modifiers);
    assert_eq!(app.document.history_counts().0, count + 1);
    assert!(app.document.mesh().mask.contains(&0.));
    assert!(app.document.mesh().mask.contains(&1.));
}

#[test]
fn workflow_ctrl_drag_navigation_preserves_geometry_and_remapped_pan() {
    for remapped in [false, true] {
        for fast in [false, true] {
            let mut app = fixture();
            let ctx = egui::Context::default();
            if remapped {
                app.ui.preferences.zoom_hold = preferences::Hold::Alt;
                app.ui.preferences.pan_hold = preferences::Hold::Ctrl;
            }
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let before = app.document.geometry_snapshot();
            let history = app.document.history_counts();
            let zoom = app.camera.zoom;
            let pan = app.camera.pan;
            let a = Pos2::new(760., 600.);
            let b = a + Vec2::new(30., 35.);
            if fast {
                frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(a),
                        button(a, true, egui::Modifiers::CTRL),
                        egui::Event::PointerMoved(b),
                        button(b, false, egui::Modifiers::CTRL),
                    ],
                    egui::Modifiers::CTRL,
                );
            } else {
                frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(a),
                        button(a, true, egui::Modifiers::CTRL),
                    ],
                    egui::Modifiers::CTRL,
                );
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(b)],
                    egui::Modifiers::CTRL,
                );
                frame(
                    &mut app,
                    &ctx,
                    vec![button(b, false, egui::Modifiers::CTRL)],
                    egui::Modifiers::CTRL,
                );
            }
            if remapped {
                assert_ne!(app.camera.pan, pan, "fast={fast}");
            } else {
                assert_ne!(app.camera.zoom, zoom, "fast={fast}");
            }
            assert_eq!(app.document.geometry_snapshot(), before);
            assert_eq!(app.document.history_counts(), history);
        }
    }
}

#[test]
fn workflow_pending_taps_cancel_and_already_active_or_hidden_targets_do_not_select() {
    for cancel in [
        egui::Event::PointerGone,
        egui::Event::WindowFocused(false),
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        },
    ] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        let p = Pos2::new(500., 400.);
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p), button(p, true, modifiers)],
            modifiers,
        );
        frame(&mut app, &ctx, vec![cancel], modifiers);
        frame(&mut app, &ctx, vec![button(p, false, modifiers)], modifiers);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
    }
    let mut app = fixture();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let history = app.document.history_counts();
    tap(
        &mut app,
        &ctx,
        Pos2::new(540., 435.),
        egui::Modifiers::CTRL,
        true,
    );
    assert_eq!(app.document.history_counts(), history);
    app.document.duplicate_model(false).unwrap();
    let mut project = app.document.project();
    project.submodels[0].metadata.visible = false;
    app.document
        .replace_project(project, "hidden.clay")
        .unwrap();
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    tap(
        &mut app,
        &ctx,
        Pos2::new(540., 435.),
        egui::Modifiers::CTRL,
        true,
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
}

#[test]
fn workflow_remapped_selection_hold_requires_an_active_context_to_invert() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.ui.preferences.visibility_hold = preferences::HoldBinding {
        alt: true,
        ..Default::default()
    };
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    tap(
        &mut app,
        &ctx,
        Pos2::new(540., 435.),
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        true,
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
}

#[test]
fn workflow_screen_controls_move_depth_rotate_after_orbit_roll_and_preserve_pivot() {
    for perspective in [false, true] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        app.camera.perspective = perspective;
        app.camera.yaw = 0.55;
        app.camera.pitch = -0.3;
        app.camera.roll = 0.65;
        app.dispatch(Command::TransformMode(TransformMode::World), &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let screen = text_point(&output, "Screen space");
        tap(&mut app, &ctx, screen, egui::Modifiers::NONE, false);
        assert_eq!(app.ui.transform_space, TransformSpace::Screen);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts().0, count);
        let start = Pos2::new(585., 400.);
        let end = start + Vec2::new(30., 0.);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(end)],
            egui::Modifiers::NONE,
        );
        frame(
            &mut app,
            &ctx,
            vec![button(end, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        let delta = app.document.mesh().vertices[0] - before.mesh.vertices[0];
        let (right, up, view) = app.camera.basis();
        assert!(delta.dot(right) > 0.01, "{delta:?}");
        assert!(delta.dot(up).abs() < 1e-5 && delta.dot(view).abs() < 1e-5);
        roundtrip(&mut app, &before, count);
        app.document.undo();
        app.dispatch(
            Command::TransformOperation(TransformOperation::Rotate),
            &ctx,
        );
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let center = Pos2::new(500., 400.);
        let a = center + Vec2::new(60.104, -60.104);
        let b = center + Vec2::new(-60.104, -60.104);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(a),
                button(a, true, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(b)],
            egui::Modifiers::NONE,
        );
        frame(
            &mut app,
            &ctx,
            vec![button(b, false, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        for (&p, &q) in before
            .mesh
            .vertices
            .iter()
            .zip(&app.document.mesh().vertices)
        {
            assert!((p.dot(view) - q.dot(view)).abs() < 1e-4);
            assert!((p.len() - q.len()).abs() < 1e-4);
        }
        assert_ne!(app.document.geometry_snapshot(), before);
    }
}

#[test]
fn workflow_z_split_matches_selection_context() {
    use crate::preferences::SelectionMode;
    use crate::ui::context_menu::ViewportMenuContext as Menu;
    for (mode, context, action, label) in [
        (
            SelectionMode::Mask,
            Menu::Mask,
            Action::SplitMasked,
            "Split masked",
        ),
        (
            SelectionMode::Visibility,
            Menu::Visibility,
            Action::SplitVisible,
            "Split visible",
        ),
        (
            SelectionMode::Components,
            Menu::Components,
            Action::SplitSelectedFaces,
            "Split selected",
        ),
    ] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        let mut geometry = app.document.geometry_snapshot();
        geometry.mesh.mask = geometry
            .mesh
            .vertices
            .iter()
            .map(|p| if p.0 > 0. { 1. } else { 0. })
            .collect();
        geometry.mesh.hidden_faces = (0..geometry.mesh.triangles.len())
            .map(|f| f % 4 < 2)
            .collect();
        app.document.replace_geometry(geometry).unwrap();
        app.dispatch(Command::ModelingMode(true), &ctx);
        app.ui.preferences.selection_mode = mode;
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
        assert_eq!(app.ui.viewport_menu.context, Some(context));
        let entries = app.ui.viewport_menu_entries(context, &app.document);
        let splits = entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.action,
                    Action::SplitMasked
                        | Action::SplitVisible
                        | Action::SplitSelectedFaces
                        | Action::SplitDisconnectedIslands
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0].action, action);
        assert_eq!(splits[0].label, label);
        assert!(splits[0].enabled, "{}", splits[0].hint);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);

        app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
        let split_index = entries
            .iter()
            .position(|entry| entry.action == action)
            .unwrap();
        app.ui.viewport_menu.selected = split_index;
        for _ in 0..6 {
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        }
        let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let split_point = text_point(&output, label);
        assert!(app.ui.viewport_menu.rect.unwrap().contains(split_point));

        #[cfg(target_os = "linux")]
        if let Some(dir) = std::env::var_os("CLAY_WORKFLOW_CAPTURES") {
            // Start with a fresh egui context so the renderer receives the
            // initial font atlas (including the white pixel used by buttons).
            let visual_ctx = egui::Context::default();
            let mut visual_app = fixture();
            visual_app
                .document
                .replace_geometry(before.clone())
                .unwrap();
            visual_app.dispatch(Command::ModelingMode(true), &visual_ctx);
            visual_app.ui.preferences.selection_mode = mode;
            visual_app.ui.viewport_menu.context = Some(context);
            visual_app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
            visual_app.ui.viewport_menu.selected = split_index;
            let mut capture = crate::viewport::UiCapture::new([1000, 800]);
            for _ in 0..6 {
                let output = frame(&mut visual_app, &visual_ctx, vec![], egui::Modifiers::NONE);
                let rgba = capture.render(&visual_ctx, output);
                capture.save(
                    std::path::PathBuf::from(&dir).join(format!("split-{context:?}.png")),
                    &rgba,
                );
            }
        }

        // Activate real menu rows through both pointer and keyboard routes.
        // Modeling's real face-selection dispatch is covered by the adjacent test.
        if mode == SelectionMode::Mask {
            tap(&mut app, &ctx, split_point, egui::Modifiers::NONE, false);
        } else if mode == SelectionMode::Visibility {
            app.ui.viewport_menu.selected = split_index;
            app.ui.viewport_menu.just_opened = false;
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                egui::Modifiers::NONE,
            );
        }
        if mode != SelectionMode::Components {
            assert_eq!(app.ui.surface_edit_warning.as_ref().unwrap().action, action);
            assert_eq!(app.document.geometry_snapshot(), before);
            app.dispatch(Command::ConfirmSurfaceEdit(false), &ctx);
            assert_eq!(app.document.history_counts(), history);
        }
    }
}

#[test]
fn workflow_submodels_has_distinct_split_to_parts_control() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    let mut geometry = app.document.geometry_snapshot();
    let mut other = geometry.mesh.clone();
    for p in &mut other.vertices {
        p.0 += 3.;
    }
    geometry.mesh = geometry.mesh.merged_with(&other).unwrap();
    app.document.replace_geometry(geometry).unwrap();
    app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
    for panel in &mut app.ui.preferences.panels {
        panel.open = panel.tool == preferences::Tool::Submodels;
        panel.dock = if panel.open {
            preferences::Dock::Left
        } else {
            preferences::Dock::Hidden
        };
    }
    let before = app.document.geometry_snapshot();
    let count = app.document.history_counts().0;
    let panel_frame = |app: &mut App, events| {
        let mut commands = Vec::new();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(1200., 1000.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                commands = app.ui.brush_panel(ctx, &app.document, "", 1., &app.camera);
            },
        );
        for command in commands {
            app.dispatch(command, &ctx);
        }
        output
    };
    for _ in 0..6 {
        panel_frame(&mut app, vec![]);
    }
    let output = panel_frame(&mut app, vec![]);
    let region = text_point(&output, "Split masked");
    let parts = text_point(&output, "Split to parts");
    assert_ne!(region, parts);
    assert!(ctx.content_rect().contains(parts));
    for pressed in [true, false] {
        panel_frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(parts),
                button(parts, pressed, egui::Modifiers::NONE),
            ],
        );
    }
    assert_eq!(app.document.model_count(), 2, "{}", app.ui.status);
    roundtrip(&mut app, &before, count);
}

#[test]
fn workflow_split_visible_preserves_hidden_remainder_and_is_atomic() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    let history = app.document.history_counts();
    let before = app.document.geometry_snapshot();
    assert!(!app.ui.action_enabled(&Action::SplitVisible, &app.document));
    assert!(app.document.split_visible().is_err());
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);

    let mut geometry = before;
    geometry.mesh.hidden_faces = (0..geometry.mesh.triangles.len())
        .map(|f| f % 4 < 2)
        .collect();
    geometry.mesh.mask = geometry
        .mesh
        .vertices
        .iter()
        .map(|p| (p.0 + 1.) / 2.)
        .collect();
    app.document.replace_geometry(geometry).unwrap();
    let before = app.document.geometry_snapshot();
    let count = app.document.history_counts().0;
    app.dispatch(Command::SplitVisible, &ctx);
    assert_eq!(
        app.ui.surface_edit_warning.as_ref().unwrap().action,
        Action::SplitVisible
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    app.dispatch(Command::ConfirmSurfaceEdit(true), &ctx);
    assert_eq!(app.document.model_count(), 2, "{}", app.ui.status);
    let project = app.document.project();
    assert_eq!(
        project.mesh.hidden_face_count(),
        project.mesh.triangles.len()
    );
    let separated = &project.submodels[0].mesh;
    assert_eq!(separated.hidden_face_count(), 0);
    assert_eq!(
        separated.triangles.len() + project.mesh.triangles.len(),
        before.mesh.triangles.len()
    );
    assert!(project.submodels[0].name.starts_with("Visible "));
    for (part, visible) in [(&project.mesh, false), (separated, true)] {
        part.validate().unwrap();
        for triangle in &part.triangles {
            let source = before
                .mesh
                .triangles
                .iter()
                .position(|t| {
                    (0..3).all(|c| part.vertices[triangle[c]] == before.mesh.vertices[t[c]])
                })
                .unwrap();
            assert_eq!(before.mesh.face_visible(source), visible);
            for c in 0..3 {
                assert_eq!(
                    part.mask[triangle[c]],
                    before.mesh.mask[before.mesh.triangles[source][c]]
                );
            }
        }
    }
    roundtrip(&mut app, &before, count);
}

#[test]
fn workflow_split_selected_and_islands_are_atomic_and_persist_attributes() {
    for islands in [false, true] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        let mut geometry = app.document.geometry_snapshot();
        if islands {
            let mut other = geometry.mesh.clone();
            for p in &mut other.vertices {
                p.0 += 2.;
            }
            geometry.mesh = geometry.mesh.merged_with(&other).unwrap();
        }
        geometry.mesh.mask = geometry
            .mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(i, _)| (i % 7) as f32 / 6.)
            .collect();
        geometry.mesh.uv.corners = geometry
            .mesh
            .triangles
            .iter()
            .map(|_| [[0., 0.], [1., 0.], [0., 1.]])
            .collect();
        geometry.mesh.uv.topology = geometry.mesh.triangles.clone();
        geometry
            .mesh
            .create_vertex_group("Soft selection".into(), 0.3, false)
            .unwrap();
        geometry.mesh.materials = geometry
            .mesh
            .vertices
            .iter()
            .map(|p| crate::sculpt::Material {
                base_color: [p.0.abs().min(1.), 0.3, 0.7],
                ..Default::default()
            })
            .collect();
        geometry.mesh.creases = geometry.mesh.polygon_edges();
        geometry.mesh.uv.seams = geometry.mesh.polygon_edges();
        geometry.mesh.hidden_faces = (0..geometry.mesh.triangles.len())
            .map(|i| i % 3 == 0)
            .collect();
        app.document.replace_geometry(geometry).unwrap();
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        if islands {
            app.dispatch(Command::SplitDisconnectedIslands, &ctx);
        } else {
            app.document.split_selected_faces(&[0, 1]).unwrap();
        }
        assert_eq!(app.document.model_count(), 2, "{}", app.ui.status);
        let project = app.document.project();
        let parts = [&project.mesh, &project.submodels[0].mesh];
        assert_eq!(
            parts.iter().map(|m| m.triangles.len()).sum::<usize>(),
            before.mesh.triangles.len()
        );
        for part in parts {
            assert!(!part.uv.corners.is_empty());
            assert_eq!(part.creases, part.polygon_edges());
            assert_eq!(part.uv.seams, part.polygon_edges());
            assert_eq!(part.quads.len() * 2, part.triangles.len());
            part.validate().unwrap();
            for (i, p) in part.vertices.iter().enumerate() {
                assert!(
                    before
                        .mesh
                        .vertices
                        .iter()
                        .zip(&before.mesh.mask)
                        .any(|(q, &mask)| q == p && mask == part.mask[i])
                );
                let source = before
                    .mesh
                    .vertices
                    .iter()
                    .zip(&before.mesh.mask)
                    .position(|(q, &mask)| q == p && mask == part.mask[i])
                    .unwrap();
                assert_eq!(part.materials[i], before.mesh.materials[source]);
                for (a, b) in part.vertex_groups.iter().zip(&before.mesh.vertex_groups) {
                    assert_eq!(a.weights[i], b.weights[source]);
                }
            }
            for (f, triangle) in part.triangles.iter().enumerate() {
                let source = before
                    .mesh
                    .triangles
                    .iter()
                    .position(|t| {
                        (0..3).all(|c| {
                            part.vertices[triangle[c]] == before.mesh.vertices[t[c]]
                                && part.mask[triangle[c]] == before.mesh.mask[t[c]]
                        })
                    })
                    .unwrap();
                assert_eq!(part.face_visible(f), before.mesh.face_visible(source));
                assert_eq!(part.uv.corners[f], before.mesh.uv.corners[source]);
                assert_eq!(
                    part.polygroups.get(f).copied().unwrap_or(0),
                    before.mesh.polygroups.get(source).copied().unwrap_or(0)
                );
            }
        }
        roundtrip(&mut app, &before, count);
        app.document.undo();
        let counts = app.document.history_counts();
        assert!(app.document.split_selected_faces(&[]).is_err());
        assert_eq!(app.document.history_counts(), counts);
        assert_eq!(app.document.geometry_snapshot(), before);
        if !islands {
            assert!(
                app.document
                    .split_disconnected_islands()
                    .unwrap_err()
                    .contains("only one")
            );
        }
    }
}

#[test]
fn workflow_face_selection_dispatch_and_model_limit_rejections_are_atomic() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.dispatch(Command::ModelingMode(true), &ctx);
    app.ui.modeling.mode = crate::viewport::ComponentMode::Face;
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    tap(
        &mut app,
        &ctx,
        Pos2::new(560., 435.),
        egui::Modifiers::NONE,
        false,
    );
    assert!(!app.viewport.modeling.selected_faces().is_empty());
    let before = app.document.geometry_snapshot();
    let count = app.document.history_counts().0;
    app.dispatch(Command::SplitSelectedFaces, &ctx);
    assert_eq!(app.document.model_count(), 2, "{}", app.ui.status);
    roundtrip(&mut app, &before, count);

    let mut project = app.document.project();
    let template = project.submodels[0].clone();
    while project.submodels.len() < 63 {
        let mut part = template.clone();
        part.metadata.model_id = 0;
        part.name = format!("Part {}", project.submodels.len());
        project.submodels.push(part);
    }
    // Two disconnected parts in the active model, and no free model slots.
    let mut other = project.mesh.clone();
    for p in &mut other.vertices {
        p.0 += 3.;
    }
    project.mesh = project.mesh.merged_with(&other).unwrap();
    app.document.replace_project(project, "limit.clay").unwrap();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    assert!(
        app.document
            .split_selected_faces(&[0])
            .unwrap_err()
            .contains("64")
    );
    assert!(
        app.document
            .split_disconnected_islands()
            .unwrap_err()
            .contains("64")
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
}

#[test]
fn workflow_screen_depth_and_scale_use_camera_axes_in_both_projections() {
    for perspective in [false, true] {
        for scaling in [false, true] {
            let mut app = fixture();
            let ctx = egui::Context::default();
            app.camera.perspective = perspective;
            app.camera.yaw = 0.55;
            app.camera.pitch = -0.3;
            app.camera.roll = 0.65;
            app.dispatch(Command::TransformMode(TransformMode::World), &ctx);
            app.dispatch(Command::TransformSpace(TransformSpace::Screen), &ctx);
            if scaling {
                app.dispatch(Command::TransformOperation(TransformOperation::Scale), &ctx);
            }
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let before = app.document.geometry_snapshot();
            let count = app.document.history_counts().0;
            let a = if scaling {
                Pos2::new(585., 400.)
            } else {
                Pos2::new(449., 349.)
            };
            let b = a + if scaling {
                Vec2::new(30., 0.)
            } else {
                Vec2::new(-25., -25.)
            };
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(a),
                    button(a, true, egui::Modifiers::NONE),
                ],
                egui::Modifiers::NONE,
            );
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(b)],
                egui::Modifiers::NONE,
            );
            frame(
                &mut app,
                &ctx,
                vec![button(b, false, egui::Modifiers::NONE)],
                egui::Modifiers::NONE,
            );
            let (right, up, view) = app.camera.basis();
            for (&p, &q) in before
                .mesh
                .vertices
                .iter()
                .zip(&app.document.mesh().vertices)
            {
                if scaling {
                    assert!((p.dot(up) - q.dot(up)).abs() < 1e-4);
                    assert!((p.dot(view) - q.dot(view)).abs() < 1e-4);
                    assert!((q.dot(right) - p.dot(right) * 0.3_f32.exp()).abs() < 1e-4);
                } else {
                    assert!((q - p).dot(right).abs() < 1e-4);
                    assert!((q - p).dot(up).abs() < 1e-4);
                    assert!((q - p).dot(view) > 0.);
                }
            }
            roundtrip(&mut app, &before, count);
        }
    }
}

#[test]
fn workflow_space_controls_preserve_a_custom_pivot_without_geometry_history() {
    let mut app = fixture();
    let ctx = egui::Context::default();
    app.camera.yaw = 0.6;
    app.camera.roll = 0.4;
    app.dispatch(Command::TransformMode(TransformMode::World), &ctx);
    app.dispatch(
        Command::TransformOperation(TransformOperation::PlacePivot),
        &ctx,
    );
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    }
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    let start = Pos2::new(500., 400.);
    let end = Pos2::new(535., 425.);
    frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::PointerMoved(start),
            button(start, true, egui::Modifiers::NONE),
        ],
        egui::Modifiers::NONE,
    );
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(end)],
        egui::Modifiers::NONE,
    );
    let output = frame(
        &mut app,
        &ctx,
        vec![button(end, false, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    let pivot = text_point(&output, "PIVOT");
    for label in ["Screen space", "World space"] {
        let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let point = text_point(&output, label);
        tap(&mut app, &ctx, point, egui::Modifiers::NONE, false);
        let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        assert_eq!(text_point(&output, "PIVOT"), pivot);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn workflow_optional_transform_visual_review() {
    let Some(dir) = std::env::var_os("CLAY_WORKFLOW_CAPTURES") else {
        return;
    };
    for perspective in [false, true] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        app.camera.perspective = perspective;
        app.camera.yaw = 0.55;
        app.camera.pitch = -0.3;
        app.camera.roll = 0.65;
        app.dispatch(Command::TransformMode(TransformMode::World), &ctx);
        app.dispatch(Command::TransformSpace(TransformSpace::Screen), &ctx);
        let mut capture = crate::viewport::UiCapture::new([1000, 800]);
        for _ in 0..6 {
            let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
            let rgba = capture.render(&ctx, output);
            capture.save(
                std::path::PathBuf::from(&dir).join(format!("screen-space-{perspective}.png")),
                &rgba,
            );
        }
    }
}
