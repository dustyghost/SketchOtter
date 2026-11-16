use super::*;
use crate::viewport::{Component, ComponentMode, RegionShape};
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
