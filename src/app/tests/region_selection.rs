use super::*;
use crate::{
    sculpt::{Geometry, Primitive, PrimitiveKind},
    viewport::{ComponentMode, RegionShape, SelectionChange},
};

fn setup(mode: ComponentMode, perspective: bool) -> (App, egui::Context, egui::Rect) {
    let mut app = App::default();
    let ctx = egui::Context::default();
    let mut cube = Primitive::for_kind(PrimitiveKind::Cube);
    cube.divisions = [3; 3];
    app.document
        .replace_geometry(Geometry::new(cube.mesh().unwrap()))
        .unwrap();
    app.camera.yaw = 0.;
    app.camera.pitch = 0.;
    app.camera.zoom = 0.6;
    app.camera.perspective = perspective;
    app.dispatch(Command::ModelingMode(true), &ctx);
    app.dispatch(Command::ModelingComponent(mode), &ctx);
    let rect = navigation_frame(&mut app, &ctx, vec![]);
    (app, ctx, rect)
}
fn drag(app: &mut App, ctx: &egui::Context, rect: egui::Rect, shape: RegionShape) {
    let area = rect.shrink(45.);
    let points = match shape {
        RegionShape::Rectangle => vec![area.left_top(), area.right_bottom()],
        RegionShape::Lasso => vec![
            area.left_top(),
            area.right_top(),
            area.right_bottom(),
            area.left_bottom(),
            area.left_top(),
        ],
        RegionShape::Line => vec![
            area.right_top() - egui::vec2(150., 0.),
            area.right_bottom() - egui::vec2(150., 0.),
        ],
    };
    let mut events = vec![
        egui::Event::PointerMoved(points[0]),
        primary_event(points[0], true),
    ];
    events.extend(points.iter().skip(1).map(|&p| egui::Event::PointerMoved(p)));
    events.push(primary_event(*points.last().unwrap(), false));
    let modifiers = if app.ui.modeling.active
        && app.ui.preferences.selection_mode == preferences::SelectionMode::Components
    {
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    navigation_frame_with_modifiers(app, ctx, events, modifiers);
    navigation_frame(app, ctx, vec![]);
}

#[test]
fn region_selection_shapes_modes_depth_and_history() {
    for perspective in [false, true] {
        for mode in [
            ComponentMode::Face,
            ComponentMode::Edge,
            ComponentMode::Vertex,
        ] {
            for shape in RegionShape::ALL {
                let (mut app, ctx, rect) = setup(mode, perspective);
                app.execute_action(Action::ModelTool(crate::viewport::ModelTool::Move), &ctx);
                let options = app.ui.modeling.options();
                let before = app.document.geometry_snapshot();
                let history = app.document.history_counts();
                app.execute_action(Action::ModelRegionShape(shape), &ctx);
                drag(&mut app, &ctx, rect, shape);
                assert_eq!(app.ui.modeling.options(), options);
                let visible = app.viewport.modeling.selected.clone();
                assert!(!visible.is_empty(), "{mode:?} {shape:?}: {}", app.ui.status);
                app.execute_action(Action::ModelSelectThrough(true), &ctx);
                drag(&mut app, &ctx, rect, shape);
                let all = app.viewport.modeling.selected.clone();
                let expected = match mode {
                    ComponentMode::Face => before.mesh.polygons().len(),
                    ComponentMode::Edge => before.mesh.polygon_edges().len(),
                    ComponentMode::Vertex => before.mesh.vertices.len(),
                };
                assert_eq!(all.len(), expected, "{mode:?} {shape:?}: {}", app.ui.status);
                assert!(all.len() > visible.len());
                app.dispatch(Command::ModelSelectionHistory(false), &ctx);
                assert_eq!(app.viewport.modeling.selected, visible);
                app.dispatch(Command::ModelSelectionHistory(true), &ctx);
                assert_eq!(app.viewport.modeling.selected, all);
                app.execute_action(Action::ModelRegionChange(SelectionChange::Remove), &ctx);
                assert_eq!(app.ui.preferences.component_region_shape, shape);
                drag(&mut app, &ctx, rect, shape);
                assert!(app.viewport.modeling.selected.is_empty());
                app.execute_action(Action::ModelRegionChange(SelectionChange::Add), &ctx);
                drag(&mut app, &ctx, rect, shape);
                assert_eq!(app.viewport.modeling.selected, all);
                assert_eq!(app.document.geometry_snapshot(), before);
                assert_eq!(app.document.history_counts(), history);
            }
        }
    }
}

#[test]
fn region_selection_cancel_and_protection_do_not_edit_geometry() {
    let (mut app, ctx, rect) = setup(ComponentMode::Vertex, false);
    let mut geometry = app.document.geometry_snapshot();
    geometry.mesh.mask[0] = 1.;
    geometry.mesh.hidden_faces = vec![false; geometry.mesh.triangles.len()];
    geometry.mesh.hidden_faces[2] = true;
    let hidden = geometry.mesh.triangles[2];
    app.document.replace_geometry(geometry.clone()).unwrap();
    app.execute_action(Action::ModelSelectThrough(true), &ctx);
    app.execute_action(Action::ModelRegionShape(RegionShape::Rectangle), &ctx);
    drag(&mut app, &ctx, rect, RegionShape::Rectangle);
    let selected = app.viewport.modeling.selected.clone();
    assert!(!selected.contains(&crate::viewport::Component::Vertex(0)));
    for v in hidden {
        assert!(!selected.contains(&crate::viewport::Component::Vertex(v)));
    }
    let start = rect.min + egui::vec2(50., 50.);
    let hold = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    navigation_frame_with_modifiers(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), primary_event(start, true)],
        hold,
    );
    navigation_frame_with_modifiers(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(rect.center())],
        hold,
    );
    assert_eq!(app.viewport.modeling.selected, selected);
    navigation_frame_with_modifiers(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: hold,
        }],
        hold,
    );
    navigation_frame(&mut app, &ctx, vec![primary_event(rect.center(), false)]);
    assert!(app.ui.modeling.active);
    assert_eq!(app.viewport.modeling.selected, selected);
    assert_eq!(app.document.geometry_snapshot(), geometry);

    // Releasing the shared hold before the mouse must cancel the outline and
    // must not let the still-down mouse begin the previous Modeling function.
    app.execute_action(Action::ModelTool(crate::viewport::ModelTool::Move), &ctx);
    let options = app.ui.modeling.options();
    let history = app.document.history_counts();
    navigation_frame_with_modifiers(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), primary_event(start, true)],
        hold,
    );
    navigation_frame_with_modifiers(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(rect.center())],
        hold,
    );
    navigation_frame(&mut app, &ctx, vec![]);
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(
            rect.center() + egui::vec2(30., 0.),
        )],
    );
    navigation_frame(&mut app, &ctx, vec![primary_event(rect.center(), false)]);
    assert_eq!(app.ui.modeling.options(), options);
    assert_eq!(app.viewport.modeling.selected, selected);
    assert_eq!(app.document.geometry_snapshot(), geometry);
    assert_eq!(app.document.history_counts(), history);
}

#[test]
fn region_selection_mask_depth_and_undo() {
    use crate::viewport::{RegionOperation, RegionTool, SelectionTool};
    for perspective in [false, true] {
        for shape in RegionShape::ALL {
            let (mut app, ctx, rect) = setup(ComponentMode::Vertex, perspective);
            app.dispatch(Command::ModelingMode(false), &ctx);
            let before = app.document.geometry_snapshot();
            let history = app.document.history_counts().0;
            let tool = |through| {
                SelectionTool::Region(RegionTool {
                    shape,
                    through,
                    flip: false,
                    operation: RegionOperation::Mask { erase: false },
                })
            };
            app.dispatch(Command::SelectTool(Some(tool(false))), &ctx);
            drag(&mut app, &ctx, rect, shape);
            let visible = app.document.mesh().mask.iter().filter(|&&x| x > 0.).count();
            assert!(
                visible > 0,
                "{shape:?}, perspective={perspective}: {}",
                app.ui.status
            );
            assert_eq!(app.document.history_counts().0, history + 1);
            app.dispatch(Command::Undo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), before);
            app.dispatch(Command::SelectTool(Some(tool(true))), &ctx);
            drag(&mut app, &ctx, rect, shape);
            assert_eq!(
                app.document.mesh().mask.iter().filter(|&&x| x > 0.).count(),
                before.mesh.vertices.len()
            );
            assert!(visible < before.mesh.vertices.len());
            app.dispatch(Command::Undo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), before);
            app.dispatch(Command::Redo, &ctx);
            assert!(app.document.mesh().mask.iter().all(|&x| x == 1.));
        }
    }
}

#[test]
fn region_selection_quick_menu_actions_and_preferences() {
    use crate::ui::context_menu::ViewportMenuContext;
    let (mut app, ctx, _) = setup(ComponentMode::Vertex, false);
    let entries = app
        .ui
        .viewport_menu_entries(ViewportMenuContext::Modeling, &app.document);
    for edit in [
        crate::sculpt::TopologyEdit::CreateFace,
        crate::sculpt::TopologyEdit::Connect,
        crate::sculpt::TopologyEdit::Weld,
    ] {
        assert!(
            entries
                .iter()
                .any(|e| e.action == Action::ModelTopology(edit))
        );
    }
    app.ui.preferences.selection_mode = preferences::SelectionMode::Components;
    app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
    assert_eq!(
        app.ui.viewport_menu.context,
        Some(ViewportMenuContext::Components)
    );
    let entries = app
        .ui
        .viewport_menu_entries(ViewportMenuContext::Components, &app.document);
    for shape in RegionShape::ALL {
        assert!(
            entries
                .iter()
                .any(|e| e.action == Action::ModelRegionShape(shape) && e.enabled)
        );
    }
    app.execute_action(Action::ModelRegionShape(RegionShape::Lasso), &ctx);
    app.execute_action(Action::ModelSelectThrough(true), &ctx);
    app.execute_action(Action::ModelRegionChange(SelectionChange::Add), &ctx);
    assert_eq!(
        app.ui.preferences.component_region_change,
        SelectionChange::Add
    );
    assert_eq!(
        app.ui.preferences.component_region_shape,
        RegionShape::Lasso
    );
    let saved = serde_json::to_string(&app.ui.preferences).unwrap();
    let restored: Preferences = serde_json::from_str(&saved).unwrap();
    assert!(restored.component_select_through);
    assert_eq!(restored.component_region_shape, RegionShape::Lasso);
    // The remembered line-side toggle must not invert rectangle/lasso regions.
    for shape in [
        preferences::MaskShape::Rectangle,
        preferences::MaskShape::Lasso,
    ] {
        let preset = preferences::MaskPreset {
            shape,
            visible_only: true,
            flip: true,
            ..Default::default()
        };
        assert!(matches!(
            preset.selection_tool(),
            Some(crate::viewport::SelectionTool::Region(
                crate::viewport::RegionTool { flip: false, .. }
            ))
        ));
    }
    let legacy: Preferences = serde_json::from_str("{}").unwrap();
    assert!(!legacy.component_select_through);
    app.ui.modeling.active = false;
    app.open_quick_menu(&ctx, Action::MaskVisibilityMenu);
    assert_eq!(
        app.ui.viewport_menu.context,
        Some(ViewportMenuContext::Mask)
    );
}

#[test]
fn region_selection_visible_bent_quad_excludes_the_occluded_face() {
    use crate::sculpt::{Mesh, Vec3};
    for perspective in [false, true] {
        let (mut app, ctx, _) = setup(ComponentMode::Face, perspective);
        let mesh = Mesh {
            vertices: vec![
                Vec3(-1., -1., -0.5),
                Vec3(1., -1., -0.5),
                Vec3(1., 1., -0.5),
                Vec3(-1., 1., -0.5),
                Vec3(-1., -1., 0.3),
                Vec3(1., -1., 0.3),
                Vec3(1., 1., 0.8),
                Vec3(-1., 1., 0.3),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            quads: vec![[0, 1, 2, 3], [4, 5, 6, 7]],
            mask: vec![0.; 8],
            ..Default::default()
        };
        app.document.replace_geometry(Geometry::new(mesh)).unwrap();
        let rect = navigation_frame(&mut app, &ctx, vec![]);
        app.execute_action(Action::ModelRegionShape(RegionShape::Rectangle), &ctx);
        drag(&mut app, &ctx, rect, RegionShape::Rectangle);
        assert_eq!(
            app.viewport.modeling.selected,
            [crate::viewport::Component::Face(1)].into()
        );
        app.execute_action(Action::ModelSelectThrough(true), &ctx);
        drag(&mut app, &ctx, rect, RegionShape::Rectangle);
        assert_eq!(app.viewport.modeling.selected.len(), 2);
    }
}
