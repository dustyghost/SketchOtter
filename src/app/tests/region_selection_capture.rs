use super::*;
use crate::viewport::{Component, ComponentMode, RegionShape};
use crate::{sculpt::Vec3, viewport::ModelTool};
#[test]
#[ignore = "requires Vulkan; actual Q topology and Z selection controls"]
fn region_selection_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for size in [[1100, 900], [760, 720]] {
        let mut g = Guide::new(&directory, size);
        g.interactive = true;
        g.toolbar = false;
        let mut cube = Primitive::for_kind(PrimitiveKind::Cube);
        cube.divisions = [3; 3];
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(cube.mesh().unwrap()))
            .unwrap();
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.zoom = 0.6;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app
            .dispatch(Command::ModelingComponent(ComponentMode::Vertex), &g.ctx);
        let rect = g.frame(vec![]).0;
        let q = g.app.document.mesh().quads[0];
        g.app.viewport.modeling.selected =
            [Component::Vertex(q[0]), Component::Vertex(q[2])].into();
        g.frame(vec![]);
        g.app.open_quick_menu(&g.ctx, Action::ViewportContextMenu);
        g.app.ui.viewport_menu.position = Some(rect.min + egui::vec2(15., 15.));
        g.save(&format!("modeling-topology-q-{}", size[0]));
        let before = g.app.document.geometry_snapshot();
        click_guide_label(&mut g, "Connect vertices");
        assert_eq!(
            g.app.document.mesh().quads.len(),
            before.mesh.quads.len() - 1
        );
        assert!(g.app.ui.viewport_menu.context.is_none());
        g.app.dispatch(Command::Undo, &g.ctx);
        g.app.ui.preferences.selection_mode = preferences::SelectionMode::Components;
        g.app.ui.preferences.component_region_shape = RegionShape::Lasso;
        g.frame(vec![]);
        g.app.open_quick_menu(&g.ctx, Action::MaskVisibilityMenu);
        g.app.ui.viewport_menu.position = Some(rect.min + egui::vec2(15., 15.));
        g.save(&format!("modeling-selection-z-{}", size[0]));
        click_guide_label(&mut g, "Through mesh");
        assert!(g.app.ui.preferences.component_select_through);
        click_guide_label(&mut g, "Rectangle");
        assert_eq!(
            g.app.ui.preferences.component_region_shape,
            RegionShape::Rectangle
        );
        assert!(g.app.ui.viewport_menu.context.is_none());
        g.frame(vec![]); // Let the closed menu release its pointer capture.
        let area = rect.shrink(70.);
        g.modifiers = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        g.frame(vec![
            egui::Event::PointerMoved(area.min),
            primary_event(area.min, true),
        ]);
        g.frame(vec![egui::Event::PointerMoved(area.max)]);
        g.save(&format!("modeling-region-drag-{}", size[0]));
        g.frame(vec![primary_event(area.max, false)]);
        g.modifiers = egui::Modifiers::NONE;
        g.frame(vec![]);
        assert_eq!(
            g.app.viewport.modeling.selected.len(),
            before.mesh.vertices.len(),
            "{}",
            g.app.ui.status
        );
        g.save(&format!("modeling-through-selection-{}", size[0]));
        g.app.open_quick_menu(&g.ctx, Action::MaskVisibilityMenu);
        click_guide_label(&mut g, "Mask");
        click_guide_label(&mut g, "Rectangle");
        assert_eq!(
            g.app.ui.preferences.mask_preset.shape,
            preferences::MaskShape::Rectangle
        );
        g.app.open_quick_menu(&g.ctx, Action::MaskVisibilityMenu);
        click_guide_label(&mut g, "Visible only");
        assert!(g.app.ui.preferences.mask_preset.visible_only);
        g.save(&format!("mask-depth-z-{}", size[0]));
        assert_eq!(g.app.document.geometry_snapshot(), before);
    }
}

#[test]
#[ignore = "requires Vulkan; clicks both Delete loop choices and checks Undo/Redo"]
fn delete_loop_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for size in [[1100, 900], [760, 720]] {
        let mut g = Guide::new(&directory, size);
        g.interactive = true;
        g.toolbar = false;
        let mut grid = Primitive::for_kind(PrimitiveKind::Grid);
        grid.divisions = [5, 5, 1];
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(grid.mesh().unwrap()))
            .unwrap();
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.zoom = 0.6;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app
            .dispatch(Command::ModelingComponent(ComponentMode::Edge), &g.ctx);
        let edges: Vec<_> = g
            .app
            .document
            .mesh()
            .polygon_edges()
            .into_iter()
            .filter(|e| {
                e.iter()
                    .all(|&v| (g.app.document.mesh().vertices[v].0 - 0.2).abs() < 1e-5)
            })
            .collect();
        let before = g.app.document.geometry_snapshot();
        for (label, remaining, name) in [
            ("Delete loop (keep surface)", 20, "keep-surface"),
            ("Delete loop (leave gap)", 15, "leave-gap"),
        ] {
            let rect = g.frame(vec![]).0;
            g.app.viewport.modeling.selected = edges.iter().copied().map(Component::Edge).collect();
            g.frame(vec![]);
            let history = g.app.document.history_counts().0;
            g.app.open_quick_menu(&g.ctx, Action::ViewportContextMenu);
            g.app.ui.viewport_menu.position = Some(rect.min + egui::vec2(10., 10.));
            if name == "keep-surface" {
                g.save(&format!("delete-loop-options-{}", size[0]));
            }
            click_guide_label(&mut g, label);
            assert!(g.app.ui.viewport_menu.context.is_none());
            assert_eq!(
                g.app.document.mesh().quads.len(),
                remaining,
                "{}",
                g.app.ui.status
            );
            assert_eq!(g.app.document.history_counts().0, history + 1);
            let after = g.app.document.geometry_snapshot();
            g.save(&format!("delete-loop-{name}-{}", size[0]));
            g.app.dispatch(Command::Undo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), before);
            g.app.dispatch(Command::Redo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), after);
            g.app.dispatch(Command::Undo, &g.ctx);
        }
    }
}

#[test]
#[ignore = "requires Vulkan; clicks vertex and face deletion and checks Undo/Redo"]
fn component_delete_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for size in [[1100, 900], [760, 720]] {
        for (mode, label, count, name) in [
            (ComponentMode::Vertex, "Delete vertices", 21, "vertices"),
            (ComponentMode::Face, "Delete faces", 24, "faces"),
        ] {
            let mut g = Guide::new(&directory, size);
            g.interactive = true;
            g.toolbar = false;
            let mut grid = Primitive::for_kind(PrimitiveKind::Grid);
            grid.divisions = [5, 5, 1];
            g.app
                .document
                .replace_geometry(crate::sculpt::Geometry::new(grid.mesh().unwrap()))
                .unwrap();
            g.app.camera.yaw = 0.;
            g.app.camera.pitch = 0.;
            g.app.camera.zoom = 0.6;
            g.app.dispatch(Command::ModelingMode(true), &g.ctx);
            g.app.dispatch(Command::ModelingComponent(mode), &g.ctx);
            let rect = g.frame(vec![]).0;
            let component = if mode == ComponentMode::Vertex {
                Component::Vertex(
                    g.app
                        .document
                        .mesh()
                        .vertices
                        .iter()
                        .position(|p| (p.0 - 0.2).abs() < 1e-5 && (p.1 - 0.2).abs() < 1e-5)
                        .unwrap(),
                )
            } else {
                Component::Face(12)
            };
            g.app.viewport.modeling.selected = [component].into();
            g.frame(vec![]);
            let before = g.app.document.geometry_snapshot();
            let history = g.app.document.history_counts().0;
            g.app.open_quick_menu(&g.ctx, Action::ViewportContextMenu);
            g.app.ui.viewport_menu.position = Some(rect.min + egui::vec2(10., 10.));
            g.save(&format!("delete-{name}-options-{}", size[0]));
            click_guide_label(&mut g, label);
            assert!(g.app.ui.viewport_menu.context.is_none());
            assert_eq!(
                g.app.document.mesh().quads.len(),
                count,
                "{}",
                g.app.ui.status
            );
            assert_eq!(g.app.document.history_counts().0, history + 1);
            let after = g.app.document.geometry_snapshot();
            g.save(&format!("delete-{name}-result-{}", size[0]));
            g.app.dispatch(Command::Undo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), before);
            g.app.dispatch(Command::Redo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), after);
        }
    }
}

#[test]
#[ignore = "requires Vulkan; point extrusion, endpoint picking and Undo/Redo"]
fn point_extrude_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for perspective in [false, true] {
        let mut g = Guide::new(&directory, [1100, 900]);
        g.interactive = true;
        g.toolbar = false;
        let mut primitive = Primitive::for_kind(PrimitiveKind::Grid);
        primitive.divisions = [3, 3, 1];
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(primitive.mesh().unwrap()))
            .unwrap();
        g.app.camera.yaw = 0.6;
        g.app.camera.pitch = -0.25;
        g.app.camera.roll = 0.2;
        g.app.camera.zoom = 0.65;
        g.app.camera.perspective = perspective;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app
            .execute_action(Action::ModelTool(ModelTool::PointExtrude), &g.ctx);
        g.app.ui.modeling.extrude_snap = Some(false);
        let rect = g.frame(vec![]).0;
        let project = |camera: &crate::camera::Camera, p: Vec3| {
            let (right, up, _) = camera.basis();
            rect.center()
                + camera.pan
                + Vec2::new(p.dot(right), -p.dot(up)) * camera.scale_at(rect.size(), p)
        };
        let root = g
            .app
            .document
            .mesh()
            .vertices
            .iter()
            .position(|p| p.0 > 0. && p.0 < 0.5 && p.1 > 0. && p.1 < 0.5)
            .unwrap();
        let before = g.app.document.geometry_snapshot();
        let start = project(&g.app.camera, before.mesh.vertices[root]);
        let destination = start + Vec2::new(-85., -45.);
        g.stroke(start, destination);
        let after = g.app.document.geometry_snapshot();
        assert_eq!(
            after.mesh.vertices.len(),
            before.mesh.vertices.len() + 1,
            "{}",
            g.app.ui.status
        );
        assert_eq!(
            after.mesh.loose_edges,
            vec![[root, before.mesh.vertices.len()]]
        );
        assert_eq!(after.mesh.triangles, before.mesh.triangles);
        let tip = before.mesh.vertices.len();
        assert!(project(&g.app.camera, after.mesh.vertices[tip]).distance(destination) < 1.);
        let (_, _, view) = g.app.camera.basis();
        assert!(
            (after.mesh.vertices[tip] - before.mesh.vertices[root])
                .dot(view)
                .abs()
                < 1e-5
        );
        assert_eq!(
            g.app.viewport.modeling.selected,
            [Component::Vertex(tip)].into()
        );
        let name = if perspective {
            "perspective"
        } else {
            "orthographic"
        };
        g.save(&format!("point-extrude-{name}"));
        let next = destination + Vec2::new(-45., 35.);
        g.stroke(destination, next);
        let second_tip = after.mesh.vertices.len();
        assert!(
            project(&g.app.camera, g.app.document.mesh().vertices[second_tip]).distance(next) < 1.
        );
        assert_eq!(g.app.document.mesh().loose_edges.len(), 2);
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), after);
        g.app.execute_action(Action::ModelSelect, &g.ctx);
        g.app.execute_action(Action::ModelClearSelection, &g.ctx);
        let point = project(&g.app.camera, after.mesh.vertices[tip]);
        g.frame(vec![
            egui::Event::PointerMoved(point),
            primary_event(point, true),
        ]);
        g.frame(vec![primary_event(point, false)]);
        assert_eq!(
            g.app.viewport.modeling.selected,
            [Component::Vertex(tip)].into()
        );
        g.app
            .execute_action(Action::ModelTool(ModelTool::Move), &g.ctx);
        let destination = point + Vec2::new(40., -20.);
        g.stroke(point, destination);
        assert_eq!(g.app.document.mesh().loose_edges, after.mesh.loose_edges);
        assert!(
            project(&g.app.camera, g.app.document.mesh().vertices[tip]).distance(destination) < 2.,
            "{}",
            g.app.ui.status
        );
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), after);
        g.app.execute_action(Action::ModelSelect, &g.ctx);
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), after);
        g.app.execute_action(Action::ModelClearSelection, &g.ctx);
        g.app.execute_action(Action::ModelEdges, &g.ctx);
        let midpoint = (after.mesh.vertices[root] + after.mesh.vertices[tip]) * 0.5;
        let point = project(&g.app.camera, midpoint);
        g.frame(vec![
            egui::Event::PointerMoved(point),
            primary_event(point, true),
        ]);
        g.frame(vec![primary_event(point, false)]);
        assert_eq!(
            g.app.viewport.modeling.selected,
            [Component::Edge([root, tip])].into()
        );
    }
}

#[test]
#[ignore = "requires Vulkan; pointer snapping, Shift override, real joins and Undo/Redo"]
fn extrusion_snap_join_offscreen_review() {
    use crate::sculpt::{Geometry, Mesh};
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for perspective in [false, true] {
        let mut g = Guide::new(&directory, [1100, 900]);
        g.interactive = true;
        g.toolbar = false;
        let mesh = Mesh {
            vertices: vec![
                Vec3(-1., -0.5, 0.),
                Vec3(-0.3, -0.5, 0.),
                Vec3(-0.3, 0.5, 0.),
                Vec3(-1., 0.5, 0.),
                Vec3(0.3, -0.5, 0.),
                Vec3(1., -0.5, 0.),
                Vec3(1., 0.5, 0.),
                Vec3(0.3, 0.5, 0.),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            quads: vec![[0, 1, 2, 3], [4, 5, 6, 7]],
            mask: vec![0.; 8],
            ..Default::default()
        };
        g.app
            .document
            .replace_geometry(Geometry::new(mesh.clone()))
            .unwrap();
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.roll = 0.15;
        g.app.camera.zoom = 0.8;
        g.app.camera.perspective = perspective;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app
            .execute_action(Action::ModelTool(ModelTool::PointExtrude), &g.ctx);
        let rect = g.frame(vec![]).0;
        let project = |p: Vec3| {
            let (right, up, _) = g.app.camera.basis();
            rect.center()
                + g.app.camera.pan
                + Vec2::new(p.dot(right), -p.dot(up)) * g.app.camera.scale_at(rect.size(), p)
        };
        let start = project(mesh.vertices[1]);
        let target = project(mesh.vertices[4]) + Vec2::new(-4., -3.);
        let edge_start = project((mesh.vertices[1] + mesh.vertices[2]) * 0.5);
        let edge_target = project((mesh.vertices[4] + mesh.vertices[7]) * 0.5) + Vec2::new(-4., 0.);
        let before = g.app.document.geometry_snapshot();
        let history = g.app.document.history_counts().0;
        g.frame(vec![
            egui::Event::PointerMoved(start),
            primary_event(start, true),
        ]);
        g.frame(vec![egui::Event::PointerMoved(target)]);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        assert_eq!(
            g.app.viewport.modeling.preview().unwrap().loose_edges,
            vec![[1, 4]]
        );
        assert_eq!(g.app.viewport.modeling.preview().unwrap().vertices.len(), 8);
        g.save(&format!("extrusion-snap-vertex-{perspective}"));
        g.modifiers = egui::Modifiers::SHIFT;
        g.frame(vec![]);
        assert_eq!(
            g.app.viewport.modeling.preview().unwrap().vertices.len(),
            9,
            "Shift must restore free extrusion"
        );
        g.modifiers = egui::Modifiers::NONE;
        g.frame(vec![]);
        assert_eq!(g.app.viewport.modeling.preview().unwrap().vertices.len(), 8);
        g.frame(vec![primary_event(target, false)]);
        let point_joined = g.app.document.geometry_snapshot();
        assert_eq!(point_joined.mesh.vertices, mesh.vertices);
        assert_eq!(point_joined.mesh.loose_edges, vec![[1, 4]]);
        assert_eq!(g.app.document.history_counts().0, history + 1);
        assert_eq!(
            g.app.viewport.modeling.selected,
            [Component::Vertex(4)].into()
        );
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), point_joined);
        g.app.dispatch(Command::Undo, &g.ctx);
        g.app
            .execute_action(Action::ModelTool(ModelTool::EdgeExtrude), &g.ctx);
        g.frame(vec![]);
        let history = g.app.document.history_counts().0;
        g.frame(vec![
            egui::Event::PointerMoved(edge_start),
            primary_event(edge_start, true),
        ]);
        g.frame(vec![egui::Event::PointerMoved(edge_target)]);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        assert_eq!(g.app.viewport.modeling.preview().unwrap().quads.len(), 3);
        assert_eq!(g.app.viewport.modeling.preview().unwrap().vertices.len(), 8);
        g.save(&format!("extrusion-snap-edge-{perspective}"));
        g.modifiers = egui::Modifiers::SHIFT;
        g.frame(vec![]);
        assert_ne!(g.app.viewport.modeling.preview().unwrap().vertices.len(), 8);
        g.modifiers = egui::Modifiers::NONE;
        g.frame(vec![]);
        assert_eq!(g.app.viewport.modeling.preview().unwrap().vertices.len(), 8);
        g.frame(vec![primary_event(edge_target, false)]);
        let joined = g.app.document.geometry_snapshot();
        assert_eq!(joined.mesh.quads.len(), 3);
        assert_eq!(joined.mesh.vertices, mesh.vertices);
        assert_eq!(g.app.document.history_counts().0, history + 1);
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), joined);
        let saved = crate::persistence::json_bytes(&g.app.document.project()).unwrap();
        let restored: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.mesh, joined.mesh);
    }
}

#[test]
#[ignore = "requires Vulkan; clicks Create face for selected vertices and edge endpoints"]
fn create_selected_face_offscreen_review() {
    use crate::sculpt::{Geometry, Mesh};
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for (name, selection) in [
        (
            "triangle",
            vec![
                Component::Vertex(0),
                Component::Vertex(1),
                Component::Vertex(2),
            ],
        ),
        (
            "quad",
            vec![
                Component::Vertex(0),
                Component::Vertex(1),
                Component::Vertex(2),
                Component::Vertex(3),
            ],
        ),
        (
            "edges",
            vec![Component::Edge([0, 1]), Component::Edge([2, 3])],
        ),
    ] {
        let mut g = Guide::new(&directory, [1100, 900]);
        g.interactive = true;
        g.toolbar = false;
        let mesh = Mesh {
            vertices: vec![
                Vec3(-0.5, -0.5, 0.),
                Vec3(0.5, -0.5, 0.),
                Vec3(0.5, 0.5, 0.),
                Vec3(-0.5, 0.5, 0.),
            ],
            mask: vec![0.; 4],
            loose_edges: vec![[0, 1], [1, 2], [2, 3]],
            ..Default::default()
        };
        g.app
            .document
            .replace_geometry(Geometry::new(mesh))
            .unwrap();
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.zoom = 0.8;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app.dispatch(
            Command::ModelingComponent(if name == "edges" {
                ComponentMode::Edge
            } else {
                ComponentMode::Vertex
            }),
            &g.ctx,
        );
        let rect = g.frame(vec![]).0;
        g.app.viewport.modeling.selected = selection.into_iter().collect();
        g.frame(vec![]);
        let before = g.app.document.geometry_snapshot();
        let history = g.app.document.history_counts();
        g.app.open_quick_menu(&g.ctx, Action::ViewportContextMenu);
        g.app.ui.viewport_menu.position = Some(rect.min + egui::vec2(15., 15.));
        // The edge menu has more topology actions than fit at once. Reach
        // Create face through the real scroll area before clicking it.
        for _ in 0..8 {
            g.frame(vec![]);
            let menu = g
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::new("viewport_tool_menu")))
                .expect("Modeling menu is open");
            if g.text_positions
                .iter()
                .any(|(text, point)| text == "Create face" && menu.shrink(20.).contains(*point))
            {
                break;
            }
            g.frame(vec![
                egui::Event::PointerMoved(menu.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., -100.),
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            g.frame(vec![egui::Event::PointerGone]);
        }
        g.save(&format!("create-selected-face-{name}-q"));
        click_guide_label(&mut g, "Create face");
        assert!(g.app.viewport.modeling.preview().is_none());
        assert_eq!(
            g.app.document.mesh().triangles.len(),
            if name == "triangle" { 1 } else { 2 },
            "{}",
            g.app.ui.status
        );
        assert_eq!(
            g.app.document.mesh().quads.len(),
            usize::from(name != "triangle")
        );
        assert_eq!(
            g.app.document.mesh().loose_edges.len(),
            usize::from(name == "triangle")
        );
        assert_eq!(g.app.document.history_counts().0, history.0 + 1);
        let after = g.app.document.geometry_snapshot();
        g.save(&format!("create-selected-face-{name}-applied"));
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), after);
        let saved = crate::persistence::json_bytes(&g.app.document.project()).unwrap();
        let restored: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.mesh, after.mesh);
    }
}
