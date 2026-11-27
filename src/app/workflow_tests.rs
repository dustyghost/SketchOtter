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
    let mut geometry = app.document.geometry_snapshot();
    geometry.mesh.mask.fill(0.2); // Detect accidental clearing as well as inversion.
    app.document.replace_geometry(geometry).unwrap();
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

fn vertex_group_fixture() -> App {
    let mut app = fixture();
    let mut geometry = app.document.geometry_snapshot();
    geometry
        .mesh
        .create_vertex_group("Left".into(), 0.5, true)
        .unwrap();
    geometry.mesh.mask = geometry
        .mesh
        .vertices
        .iter()
        .map(|p| if p.0 >= 0. { 1. } else { 0. })
        .collect();
    geometry
        .mesh
        .create_vertex_group("Right".into(), 0.5, false)
        .unwrap();
    geometry.mesh.mask.fill(0.2);
    app.document.replace_geometry(geometry).unwrap();
    app
}

#[test]
fn workflow_vertex_group_mask_click_overrides_invert_and_is_atomic() {
    for fast in [false, true] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.execute_action(Action::MaskVertexGroup, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        tap(
            &mut app,
            &ctx,
            Pos2::new(650., 400.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            fast,
        );
        let mut expected = before.clone();
        for (f, triangle) in before.mesh.triangles.iter().enumerate() {
            if before.mesh.polygroups[f] == 2 {
                for &v in triangle {
                    expected.mesh.mask[v] = 1.;
                }
            }
        }
        assert_eq!(
            app.document.geometry_snapshot(),
            expected,
            "{}",
            app.ui.status
        );
        roundtrip(&mut app, &before, count);
        let after = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        tap(
            &mut app,
            &ctx,
            Pos2::new(120., 120.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            fast,
        );
        assert!(
            app.document.mesh().mask.iter().all(|&m| m == 0.),
            "empty-space shortcut clears rather than inverts"
        );
        roundtrip(&mut app, &after, history.0);
    }
}

#[test]
fn workflow_vertex_group_visibility_hides_last_group_without_inverting() {
    for fast in [false, true] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.execute_action(Action::VisibilityVertexGroup, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        tap(
            &mut app,
            &ctx,
            Pos2::new(650., 400.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            fast,
        );
        assert!(
            (0..app.document.mesh().triangles.len())
                .all(|f| app.document.mesh().face_visible(f) == (before.mesh.polygroups[f] != 2)),
            "{}",
            app.ui.status
        );
        assert_eq!(app.document.mesh().mask, before.mesh.mask);
        roundtrip(&mut app, &before, count);
        let before_last = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        tap(
            &mut app,
            &ctx,
            Pos2::new(350., 400.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            fast,
        );
        assert_eq!(
            app.document.mesh().hidden_face_count(),
            before.mesh.triangles.len(),
            "the last group must hide, not flip"
        );
        roundtrip(&mut app, &before_last, count);
    }
}

#[test]
fn workflow_vertex_group_component_click_selects_whole_group_in_each_mode() {
    use crate::viewport::{Component, ComponentMode};
    for mode in [
        ComponentMode::Vertex,
        ComponentMode::Edge,
        ComponentMode::Face,
    ] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.dispatch(Command::ModelingMode(true), &ctx);
        app.dispatch(Command::ModelingComponent(mode), &ctx);
        app.ui.modeling.protect_mask = false;
        app.execute_action(Action::ModelVertexGroup, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        let mut expected = std::collections::BTreeSet::new();
        let faces = crate::sculpt::uv::polygon_triangles(&before.mesh);
        for (i, p) in before.mesh.polygons().iter().enumerate() {
            if !faces[i].iter().any(|&f| before.mesh.polygroups[f] == 2) {
                continue;
            }
            match mode {
                ComponentMode::Vertex => {
                    expected.extend(p.vertices.iter().copied().map(Component::Vertex))
                }
                ComponentMode::Edge => expected.extend(
                    p.edges()
                        .map(|(a, b)| Component::Edge([a.min(b), a.max(b)])),
                ),
                ComponentMode::Face => {
                    expected.insert(Component::Face(i));
                }
            }
        }
        tap(
            &mut app,
            &ctx,
            Pos2::new(650., 400.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            false,
        );
        assert_eq!(
            app.viewport.modeling.selected, expected,
            "{mode:?}: {}",
            app.ui.status
        );
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
        app.execute_action(Action::ModelUndoSelection, &ctx);
        assert!(app.viewport.modeling.selected.is_empty());
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        app.execute_action(Action::ModelRedoSelection, &ctx);
        assert_eq!(app.viewport.modeling.selected, expected);
    }
}

#[test]
fn workflow_vertex_group_options_are_clickable_saved_and_restore_shapes() {
    use crate::ui::context_menu::ViewportMenuContext as Menu;
    for (mode, menu, action, shape) in [
        (
            preferences::SelectionMode::Mask,
            Menu::Mask,
            Action::MaskVertexGroup,
            Action::TemporaryMaskRectangle,
        ),
        (
            preferences::SelectionMode::Visibility,
            Menu::Visibility,
            Action::VisibilityVertexGroup,
            Action::TemporaryVisibilityLasso,
        ),
        (
            preferences::SelectionMode::Components,
            Menu::Components,
            Action::ModelVertexGroup,
            Action::ModelRegionShape(crate::viewport::RegionShape::Line),
        ),
    ] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.dispatch(Command::ModelingMode(true), &ctx);
        app.ui.preferences.selection_mode = mode;
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
        app.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
        for _ in 0..6 {
            frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        }
        let output = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let point = text_point(&output, "Vertex Group");
        assert!(output.shapes.iter().any(|shape| shape.clip_rect.contains(point)
            && matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "Vertex Group")));
        tap(&mut app, &ctx, point, egui::Modifiers::NONE, false);
        assert!(app.ui.viewport_menu.context.is_none());
        let saved: preferences::Preferences =
            serde_json::from_str(&serde_json::to_string(&app.ui.preferences).unwrap()).unwrap();
        assert_eq!(
            saved.mask_vertex_group,
            mode == preferences::SelectionMode::Mask
        );
        assert_eq!(
            saved.visibility_vertex_group,
            mode == preferences::SelectionMode::Visibility
        );
        assert_eq!(
            saved.component_vertex_group,
            mode == preferences::SelectionMode::Components
        );
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
        let entries = app.ui.viewport_menu_entries(menu, &app.document);
        assert!(entries.iter().any(|e| e.action == action && e.selected));

        #[cfg(target_os = "linux")]
        if let Some(dir) = std::env::var_os("CLAY_WORKFLOW_CAPTURES") {
            let mut visual = vertex_group_fixture();
            let visual_ctx = egui::Context::default();
            visual.dispatch(Command::ModelingMode(true), &visual_ctx);
            visual.ui.preferences = saved;
            visual.ui.viewport_menu.context = Some(menu);
            visual.ui.viewport_menu.position = Some(Pos2::new(20., 20.));
            visual.ui.viewport_menu.selected =
                entries.iter().position(|e| e.action == action).unwrap();
            let mut capture = crate::viewport::UiCapture::new([1000, 800]);
            for _ in 0..6 {
                let output = frame(&mut visual, &visual_ctx, vec![], egui::Modifiers::NONE);
                let rgba = capture.render(&visual_ctx, output);
                capture.save(
                    std::path::PathBuf::from(&dir).join(format!("vertex-group-{menu:?}.png")),
                    &rgba,
                );
            }
        }
        app.execute_action(shape, &ctx);
        assert!(
            !app.ui.preferences.mask_vertex_group
                && !app.ui.preferences.visibility_vertex_group
                && !app.ui.preferences.component_vertex_group
        );
    }
    let old: preferences::Preferences = serde_json::from_str(
        r#"{"mask_preset":{"shape":"Lasso"},"component_region_shape":"Line"}"#,
    )
    .unwrap();
    assert!(!old.mask_vertex_group && !old.visibility_vertex_group && !old.component_vertex_group);
    assert_eq!(old.mask_preset.shape, preferences::MaskShape::Lasso);
    assert_eq!(
        old.component_region_shape,
        crate::viewport::RegionShape::Line
    );
}

#[test]
fn workflow_vertex_group_reverse_and_component_change_respect_scope() {
    let hold = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    for alt in [false, true] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.execute_action(Action::MaskVertexGroup, &ctx);
        if !alt {
            app.execute_action(Action::TemporaryMaskErase, &ctx);
        }
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        tap(
            &mut app,
            &ctx,
            Pos2::new(650., 400.),
            if alt {
                hold | egui::Modifiers::ALT
            } else {
                hold
            },
            false,
        );
        let mut expected = before.clone();
        for (f, t) in before.mesh.triangles.iter().enumerate() {
            if before.mesh.polygroups[f] == 2 {
                for &v in t {
                    expected.mesh.mask[v] = 0.;
                }
            }
        }
        assert_eq!(
            app.document.geometry_snapshot(),
            expected,
            "{}",
            app.ui.status
        );
        roundtrip(&mut app, &before, count);
    }
    let mut app = vertex_group_fixture();
    let ctx = egui::Context::default();
    app.dispatch(Command::ModelingMode(true), &ctx);
    app.dispatch(
        Command::ModelingComponent(crate::viewport::ComponentMode::Face),
        &ctx,
    );
    app.ui.modeling.protect_mask = false;
    app.execute_action(Action::ModelVertexGroup, &ctx);
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    let before = app.document.geometry_snapshot();
    tap(&mut app, &ctx, Pos2::new(650., 400.), hold, false);
    app.execute_action(
        Action::ModelRegionChange(crate::viewport::SelectionChange::Add),
        &ctx,
    );
    tap(&mut app, &ctx, Pos2::new(350., 400.), hold, false);
    assert_eq!(
        app.viewport.modeling.selected_faces().len(),
        before.mesh.polygons().len()
    );
    app.execute_action(
        Action::ModelRegionChange(crate::viewport::SelectionChange::Remove),
        &ctx,
    );
    tap(&mut app, &ctx, Pos2::new(650., 400.), hold, false);
    assert_eq!(
        app.viewport.modeling.selected_faces().len(),
        before.mesh.polygons().len() / 2
    );
    app.ui.modeling.protect_mask = true; // Every vertex has a deliberate soft mask.
    app.execute_action(
        Action::ModelRegionChange(crate::viewport::SelectionChange::Replace),
        &ctx,
    );
    tap(&mut app, &ctx, Pos2::new(650., 400.), hold, false);
    assert!(app.viewport.modeling.selected.is_empty());
    assert_eq!(app.document.geometry_snapshot(), before);
}

#[test]
fn workflow_vertex_group_remapped_hold_pen_and_cancellation_do_not_invert() {
    for cancel in [
        None,
        Some(egui::Event::PointerGone),
        Some(egui::Event::WindowFocused(false)),
        Some(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }),
    ] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.ui.preferences.visibility_hold = preferences::HoldBinding {
            key: Some("F2".into()),
            ..Default::default()
        };
        app.execute_action(Action::MaskVertexGroup, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        let point = Pos2::new(650., 400.);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key {
                    key: egui::Key::F2,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(point),
                button(point, true, egui::Modifiers::NONE),
            ],
            egui::Modifiers::NONE,
        );
        assert_eq!(
            app.document.geometry_snapshot(),
            before,
            "commit only on release"
        );
        if let Some(cancel) = cancel {
            frame(&mut app, &ctx, vec![cancel], egui::Modifiers::NONE);
            frame(
                &mut app,
                &ctx,
                vec![button(point, false, egui::Modifiers::NONE)],
                egui::Modifiers::NONE,
            );
            assert_eq!(app.document.geometry_snapshot(), before);
            assert_eq!(app.document.history_counts(), history);
        } else {
            frame(
                &mut app,
                &ctx,
                vec![
                    button(point, false, egui::Modifiers::NONE),
                    egui::Event::PointerGone,
                ],
                egui::Modifiers::NONE,
            );
            assert!(app.document.mesh().mask.contains(&1.));
            assert!(app.document.mesh().mask.contains(&0.2));
            roundtrip(&mut app, &before, history.0);
        }
    }
}

#[test]
fn workflow_vertex_group_navigation_and_unassigned_clicks_preserve_document() {
    for remapped in [false, true] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.ui.preferences.visibility_hold = preferences::HoldBinding {
            key: Some("F2".into()),
            ..Default::default()
        };
        if remapped {
            app.ui.preferences.pan_hold = preferences::Hold::Ctrl;
            app.ui.preferences.zoom_hold = preferences::Hold::Alt;
        }
        app.execute_action(Action::MaskVertexGroup, &ctx);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::F2,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        let camera = app.camera;
        let a = Pos2::new(650., 450.);
        let b = a + Vec2::new(30., 35.);
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
        if remapped {
            assert_ne!(app.camera.pan, camera.pan);
        } else {
            assert_ne!(app.camera.zoom, camera.zoom);
        }
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
    }
    for action in [
        Action::MaskVertexGroup,
        Action::VisibilityVertexGroup,
        Action::ModelVertexGroup,
    ] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        let mut geometry = app.document.geometry_snapshot();
        for label in &mut geometry.mesh.polygroups {
            if *label == 2 {
                *label = 0;
            }
        }
        app.document.replace_geometry(geometry).unwrap();
        app.dispatch(Command::ModelingMode(true), &ctx);
        app.execute_action(action, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        tap(
            &mut app,
            &ctx,
            Pos2::new(650., 400.),
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            true,
        );
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
        assert!(app.viewport.modeling.selected.is_empty());
    }
}

#[test]
fn workflow_vertex_group_attribute_edits_cover_levels_and_protect_hidden_vertices() {
    let mut app = vertex_group_fixture();
    let mut geometry = app.document.geometry_snapshot();
    geometry.subdivide().unwrap();
    for mesh in std::iter::once(&mut geometry.mesh).chain(geometry.settings.lower.iter_mut()) {
        mesh.hidden_faces = mesh
            .triangles
            .iter()
            .enumerate()
            .map(|(f, t)| {
                mesh.polygroups[f] == 2 && t.iter().map(|&v| mesh.vertices[v].1).sum::<f32>() > 0.
            })
            .collect();
    }
    app.document.replace_geometry(geometry).unwrap();
    app.document.duplicate_model(false).unwrap();
    let before = app.document.geometry_snapshot();
    let count = app.document.history_counts().0;
    let mut expected = before.clone();
    for mesh in std::iter::once(&mut expected.mesh).chain(expected.settings.lower.iter_mut()) {
        let editable = mesh.sculptable_vertices();
        for (f, t) in mesh.triangles.iter().enumerate() {
            if mesh.face_visible(f) && mesh.polygroups[f] == 2 {
                for &v in t {
                    if editable[v] {
                        mesh.mask[v] = 1.;
                    }
                }
            }
        }
    }
    app.document.mask_polygroup(2, false).unwrap();
    assert_eq!(app.document.geometry_snapshot(), expected);
    roundtrip(&mut app, &before, count);
    app.document.undo();
    assert_eq!(app.document.geometry_snapshot(), before);
    let count = app.document.history_counts().0;
    let mut expected = before.clone();
    for mesh in std::iter::once(&mut expected.mesh).chain(expected.settings.lower.iter_mut()) {
        let visible = (0..mesh.triangles.len())
            .map(|f| mesh.face_visible(f) && mesh.polygroups[f] != 2)
            .collect();
        mesh.set_face_visibility(visible);
    }
    app.document.hide_polygroup(2, true).unwrap();
    assert_eq!(app.document.geometry_snapshot(), expected);
    roundtrip(&mut app, &before, count);
}

#[test]
fn workflow_context_clear_empty_space_uses_active_tab_with_or_without_group_mode() {
    let hold = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    for group in [false, true] {
        for fast in [false, true] {
            for (context, (action, shape)) in [
                (Action::MaskVertexGroup, Action::TemporaryMaskBrush),
                (
                    Action::VisibilityVertexGroup,
                    Action::TemporaryVisibilityRectangle,
                ),
                (
                    Action::ModelVertexGroup,
                    Action::ModelRegionShape(crate::viewport::RegionShape::Rectangle),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let mut app = vertex_group_fixture();
                let ctx = egui::Context::default();
                app.dispatch(Command::ModelingMode(true), &ctx);
                app.dispatch(
                    Command::ModelingComponent(crate::viewport::ComponentMode::Face),
                    &ctx,
                );
                app.ui.modeling.protect_mask = false;
                app.execute_action(action, &ctx);
                if context == 1 {
                    app.document.hide_polygroup(2, true).unwrap();
                }
                frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
                if context == 2 {
                    tap(&mut app, &ctx, Pos2::new(650., 400.), hold, fast);
                } else {
                    app.viewport
                        .modeling
                        .selected
                        .insert(crate::viewport::Component::Face(0));
                }
                if !group {
                    app.execute_action(shape, &ctx);
                }
                frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
                let before = app.document.geometry_snapshot();
                let history = app.document.history_counts();
                let selected = app.viewport.modeling.selected.clone();
                assert!(!selected.is_empty());
                tap(&mut app, &ctx, Pos2::new(120., 120.), hold, fast);
                let mut expected = before.clone();
                match context {
                    0 => expected.mesh.mask.fill(0.),
                    1 => expected.mesh.show_all_faces(),
                    _ => (),
                }
                assert_eq!(
                    app.document.geometry_snapshot(),
                    expected,
                    "context {context}, group {group}: {}",
                    app.ui.status
                );
                if context == 2 {
                    assert!(app.viewport.modeling.selected.is_empty());
                    assert_eq!(app.document.history_counts(), history);
                    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
                    app.execute_action(Action::ModelUndoSelection, &ctx);
                    assert_eq!(app.viewport.modeling.selected, selected);
                    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
                    app.execute_action(Action::ModelRedoSelection, &ctx);
                    assert!(app.viewport.modeling.selected.is_empty());
                } else {
                    assert_eq!(
                        app.viewport.modeling.selected, selected,
                        "clear only the active context"
                    );
                    roundtrip(&mut app, &before, history.0);
                }
                let after = app.document.geometry_snapshot();
                let history = app.document.history_counts();
                tap(&mut app, &ctx, Pos2::new(120., 120.), hold, fast);
                assert_eq!(app.document.geometry_snapshot(), after);
                assert_eq!(
                    app.document.history_counts(),
                    history,
                    "clearing an empty context is a no-op"
                );
            }
        }
    }
}

#[test]
fn workflow_context_clear_is_explicit_remappable_safe_and_cancelable() {
    let hold = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    let mut app = vertex_group_fixture();
    let ctx = egui::Context::default();
    app.ui.preferences.visibility_hold = preferences::HoldBinding {
        key: Some("F2".into()),
        ..Default::default()
    };
    app.ui.preferences.selection_mode = preferences::SelectionMode::Mask;
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    tap(&mut app, &ctx, Pos2::new(120., 120.), hold, true);
    assert!(
        app.document.mesh().mask.iter().all(|&m| m == 0.),
        "the explicit shortcut survives hold remapping"
    );
    // Showing everything must recover a completely hidden active mesh too.
    app.document.invert_visibility().unwrap();
    app.ui.preferences.selection_mode = preferences::SelectionMode::Visibility;
    frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.document.mesh().hidden_face_count(),
        app.document.mesh().triangles.len()
    );
    tap(&mut app, &ctx, Pos2::new(500., 400.), hold, false);
    assert_eq!(app.document.mesh().hidden_face_count(), 0);
    for cancel in [
        egui::Event::PointerGone,
        egui::Event::WindowFocused(false),
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: hold,
        },
    ] {
        let mut app = vertex_group_fixture();
        let ctx = egui::Context::default();
        app.execute_action(Action::MaskVertexGroup, &ctx);
        frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        let before = app.document.geometry_snapshot();
        let history = app.document.history_counts();
        let point = Pos2::new(120., 120.);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(point), button(point, true, hold)],
            hold,
        );
        frame(&mut app, &ctx, vec![cancel], hold);
        frame(&mut app, &ctx, vec![button(point, false, hold)], hold);
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), history);
    }
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
fn workflow_selection_menu_reopens_with_reachable_tabs() {
    use crate::ui::context_menu::ViewportMenuContext as Menu;
    fn menu_frame(
        app: &mut App,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        ctx.style_mut(|s| s.animation_time = 0.);
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| {
                let mut commands = app.ui.toolbar(ctx, &app.document, &app.camera);
                commands.extend(
                    app.ui
                        .brush_panel(ctx, &app.document, "Guide", 1., &app.camera),
                );
                for command in commands {
                    app.dispatch(command, ctx);
                }
                egui::CentralPanel::default().show(ctx, |ui| app.show_viewport(ui));
            },
        )
    }
    for size in [Vec2::new(1100., 800.), Vec2::new(640., 480.)] {
        let mut app = fixture();
        for panel in &mut app.ui.preferences.panels {
            panel.dock = crate::preferences::Dock::Hidden;
        }
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            let _ = app.ui.toolbar(ctx, &app.document, &app.camera);
        });
        app.dispatch(Command::ModelingMode(true), &ctx);
        menu_frame(&mut app, &ctx, vec![], size);
        app.open_quick_menu(&ctx, Action::ViewportContextMenu);
        app.ui.viewport_menu.position = Some(Pos2::new(size.x - 10., size.y - 10.));
        app.ui.viewport_menu.selected = 0;
        for _ in 0..6 {
            menu_frame(&mut app, &ctx, vec![], size);
        }
        // Scroll the old chooser to the last operation before opening Z.
        menu_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            size,
        );
        for _ in 0..6 {
            menu_frame(&mut app, &ctx, vec![], size);
        }
        app.ui.viewport_menu.context = None;
        app.ui.preferences.selection_mode = crate::preferences::SelectionMode::Mask;
        menu_frame(&mut app, &ctx, vec![], size);
        app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
        for _ in 0..6 {
            menu_frame(&mut app, &ctx, vec![], size);
        }
        let output = menu_frame(&mut app, &ctx, vec![], size);
        let point = text_point(&output, "Visibility");
        assert!(
            app.ui.viewport_menu.rect.unwrap().contains(point),
            "{size:?}: tab outside menu at {point:?}"
        );
        assert!(output.shapes.iter().any(|shape| shape.clip_rect.contains(point)
            && matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Visibility")), "{size:?}: Visibility tab is clipped");
        for pressed in [true, false] {
            menu_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(point),
                    button(point, pressed, egui::Modifiers::NONE),
                ],
                size,
            );
        }
        assert_eq!(
            app.ui.viewport_menu.context,
            Some(Menu::Visibility),
            "{size:?}"
        );
        for label in ["Mask", "Rectangle", "Visible only"] {
            if app.ui.viewport_menu.context.is_none() {
                app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
            }
            for _ in 0..6 {
                menu_frame(&mut app, &ctx, vec![], size);
            }
            let mut output = menu_frame(&mut app, &ctx, vec![], size);
            if label == "Visible only" {
                // Depth controls can be below the fold on a compact display.
                // Click them only after real wheel input reveals their pixels.
                for _ in 0..16 {
                    if output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Text(text)
                            if text.galley.text() == label
                            && shape.clip_rect.contains(text.pos + text.galley.size() * 0.5))
                    }) {
                        break;
                    }
                    let rect = app.ui.viewport_menu.rect.unwrap();
                    output = menu_frame(
                        &mut app,
                        &ctx,
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: Vec2::new(0., -60.),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        size,
                    );
                }
            }
            let point = text_point(&output, label);
            assert!(output.shapes.iter().any(|shape| shape.clip_rect.contains(point)
                && matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)), "{size:?}: {label} is clipped at {point:?}");
            for pressed in [true, false] {
                menu_frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(point),
                        button(point, pressed, egui::Modifiers::NONE),
                    ],
                    size,
                );
            }
            if label == "Mask" {
                assert_eq!(
                    app.ui.viewport_menu.context,
                    Some(Menu::Mask),
                    "{size:?}: Mask tab"
                );
            } else if label == "Rectangle" {
                assert_eq!(
                    app.ui.preferences.mask_preset.shape,
                    crate::preferences::MaskShape::Rectangle,
                    "{size:?}: Mask shape"
                );
            }
        }
        assert!(app.ui.preferences.mask_preset.visible_only, "{size:?}");
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
            Action::SplitHidden,
            "Split hidden",
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
                        | Action::SplitHidden
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
fn workflow_region_splits_clear_new_part_and_preserve_source_atomically() {
    // Bindings saved while the unreleased action was named SplitVisible still
    // resolve to the single Visibility split rather than invalidating preferences.
    assert_eq!(
        serde_json::from_str::<Action>("\"SplitVisible\"").unwrap(),
        Action::SplitHidden
    );
    for masked in [false, true] {
        let mut app = fixture();
        let ctx = egui::Context::default();
        let history = app.document.history_counts();
        let before = app.document.geometry_snapshot();
        let action = if masked {
            Action::SplitMasked
        } else {
            Action::SplitHidden
        };
        assert!(!app.ui.action_enabled(&action, &app.document));
        assert!(
            if masked {
                app.document.split_masked()
            } else {
                app.document.split_hidden()
            }
            .is_err()
        );
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
            .map(|p| if p.0 > 0. { 0.9 } else { 0.2 })
            .collect();
        app.document.replace_geometry(geometry).unwrap();
        let before = app.document.geometry_snapshot();
        let count = app.document.history_counts().0;
        let threshold = before.scene.active_metadata.mask_threshold;
        let selected = |f| {
            if masked {
                before.mesh.face_mask_selected(f, threshold)
            } else {
                !before.mesh.face_visible(f)
            }
        };
        let remaining = before.mesh.face_subset(|f| !selected(f)).unwrap();
        let mut expected = before.mesh.face_subset(selected).unwrap();
        expected.show_all_faces();
        expected.mask.fill(0.);
        app.dispatch(
            if masked {
                Command::SplitMasked
            } else {
                Command::SplitHidden
            },
            &ctx,
        );
        assert_eq!(app.ui.surface_edit_warning.as_ref().unwrap().action, action);
        assert_eq!(app.document.geometry_snapshot(), before);
        app.dispatch(Command::ConfirmSurfaceEdit(true), &ctx);
        assert_eq!(app.document.model_count(), 2, "{}", app.ui.status);
        let project = app.document.project();
        assert_eq!(
            project.mesh, remaining,
            "source attributes must remain unchanged"
        );
        let separated = &project.submodels[0].mesh;
        assert_eq!(separated, &expected);
        assert!(project.submodels[0].metadata.visible);
        assert_eq!(separated.hidden_face_count(), 0);
        assert!(separated.mask.iter().all(|&m| m == 0.));
        assert!(app.viewport.modeling.selected_faces().is_empty());
        assert_eq!(
            separated.triangles.len() + project.mesh.triangles.len(),
            before.mesh.triangles.len()
        );
        assert!(
            project.submodels[0]
                .name
                .starts_with(if masked { "Masked " } else { "Hidden " })
        );
        for (part, is_new) in [(&project.mesh, false), (separated, true)] {
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
                assert_eq!(selected(source), is_new);
                for (c, &vertex) in triangle.iter().enumerate() {
                    assert_eq!(
                        part.mask[vertex],
                        if is_new {
                            0.
                        } else {
                            before.mesh.mask[before.mesh.triangles[source][c]]
                        }
                    );
                }
            }
        }
        roundtrip(&mut app, &before, count);
    }
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
        for (index, part) in parts.into_iter().enumerate() {
            let cleared = !islands && index == 1;
            if cleared {
                assert!(!part.has_hidden_faces());
                assert!(project.submodels[0].metadata.visible);
                assert!(part.mask.iter().all(|&m| m == 0.));
            }
            assert!(!part.uv.corners.is_empty());
            assert_eq!(part.creases, part.polygon_edges());
            assert_eq!(part.uv.seams, part.polygon_edges());
            assert_eq!(part.quads.len() * 2, part.triangles.len());
            part.validate().unwrap();
            for (i, p) in part.vertices.iter().enumerate() {
                assert!(before.mesh.vertices.contains(p));
                // Islands may have coincident, deliberately unwelded vertices
                // with different attributes at their touching boundaries.
                let source = before
                    .mesh
                    .vertices
                    .iter()
                    .zip(&before.mesh.mask)
                    .position(|(q, &mask)| q == p && (cleared || mask == part.mask[i]))
                    .unwrap();
                assert_eq!(
                    part.mask[i],
                    if cleared {
                        0.
                    } else {
                        before.mesh.mask[source]
                    }
                );
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
                        (0..3).all(|c| part.vertices[triangle[c]] == before.mesh.vertices[t[c]])
                    })
                    .unwrap();
                assert_eq!(
                    part.face_visible(f),
                    cleared || before.mesh.face_visible(source)
                );
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
    assert!(app.viewport.modeling.selected_faces().is_empty());
    assert!(app.document.model(1).unwrap().metadata.visible);
    assert!(!app.document.model(1).unwrap().mesh.has_hidden_faces());
    assert!(
        app.document
            .model(1)
            .unwrap()
            .mesh
            .mask
            .iter()
            .all(|&m| m == 0.)
    );
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
