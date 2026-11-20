use super::*;
use crate::viewport::{Component, ComponentMode, ModelTool};

#[test]
#[ignore = "requires Vulkan; Q bevel controls, live sizing, cancellation, groups and Undo/Redo"]
fn component_bevel_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for (tool, name, loop_selection) in [
        (ModelTool::EdgeChamfer, "chamfer-edge", false),
        (ModelTool::EdgeBevel, "bevel-edge", false),
        (ModelTool::VertexChamfer, "chamfer-vertex", false),
        (ModelTool::VertexBevel, "bevel-vertex", false),
        (ModelTool::EdgeChamfer, "chamfer-edge-loop", true),
        (ModelTool::EdgeBevel, "bevel-edge-loop", true),
    ] {
        let mut g = Guide::new(&directory, [1100, 950]);
        g.interactive = true;
        g.toolbar = false;
        g.panel(Tool::Mesh, Dock::Right);
        let mut primitive = Primitive::for_kind(PrimitiveKind::Cube);
        primitive.divisions = [if loop_selection { 3 } else { 1 }; 3];
        let mesh = primitive.mesh().unwrap();
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(mesh.clone()))
            .unwrap();
        g.app.camera.yaw = 0.55;
        g.app.camera.pitch = -0.35;
        g.app.camera.zoom = 0.7;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app.dispatch(
            Command::ModelingComponent(tool.required_mode().unwrap()),
            &g.ctx,
        );
        g.frame(vec![]);
        g.app.execute_action(Action::ViewportContextMenu, &g.ctx);
        g.app.ui.viewport_menu.position = Some(egui::pos2(180., 130.));
        g.frame(vec![]);
        click_guide_label(&mut g, tool.label());
        assert_eq!(g.app.ui.modeling.tool, tool);
        assert!(g.app.ui.viewport_menu.context.is_none());
        g.app.ui.modeling.new_group = true;
        let rect = g.frame(vec![]).0;
        let (r, u, d) = g.app.camera.basis();
        let component = if tool.required_mode() == Some(ComponentMode::Vertex) {
            Component::Vertex(
                (0..mesh.vertices.len())
                    .max_by(|&a, &b| mesh.vertices[a].dot(d).total_cmp(&mesh.vertices[b].dot(d)))
                    .unwrap(),
            )
        } else {
            let mut edges = mesh.polygon_edges();
            if loop_selection {
                let top = mesh
                    .vertices
                    .iter()
                    .map(|p| p.1)
                    .fold(f32::NEG_INFINITY, f32::max);
                edges.retain(|e| {
                    e.iter().all(|&v| {
                        let p = mesh.vertices[v];
                        (p.1 - top).abs() < 1e-6
                            && ((p.0.abs() - top).abs() < 1e-6 || (p.2.abs() - top).abs() < 1e-6)
                    })
                });
                assert_eq!(edges.len(), 12);
                g.app.viewport.modeling.selected =
                    edges.iter().copied().map(Component::Edge).collect();
            }
            Component::Edge(
                edges
                    .into_iter()
                    .max_by(|a, b| {
                        let depth =
                            |e: &[usize; 2]| (mesh.vertices[e[0]] + mesh.vertices[e[1]]).dot(d);
                        depth(a).total_cmp(&depth(b))
                    })
                    .unwrap(),
            )
        };
        let p = match component {
            Component::Vertex(v) => mesh.vertices[v],
            Component::Edge([a, b]) => (mesh.vertices[a] + mesh.vertices[b]) * 0.5,
            _ => unreachable!(),
        };
        let start = rect.center()
            + g.app.camera.pan
            + Vec2::new(p.dot(r), -p.dot(u)) * g.app.camera.scale_at(rect.size(), p);
        let end = start - Vec2::Y * 40.;
        let before = g.app.document.geometry_snapshot();
        let history = g.app.document.history_counts();
        for cancel in [true, false] {
            g.frame(vec![
                egui::Event::PointerMoved(start),
                primary_event(start, true),
            ]);
            g.frame(vec![egui::Event::PointerMoved(end)]);
            let preview = g
                .app
                .viewport
                .modeling
                .preview()
                .unwrap_or_else(|| panic!("{}: {}", name, g.app.ui.status))
                .clone();
            preview.validate_closed().unwrap();
            assert_ne!(preview, mesh);
            assert_eq!(g.app.document.geometry_snapshot(), before);
            if cancel {
                g.frame(vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]);
                g.frame(vec![
                    primary_event(end, false),
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Default::default(),
                    },
                ]);
                assert_eq!(g.app.document.geometry_snapshot(), before);
                assert_eq!(g.app.document.history_counts(), history);
            } else {
                g.save(name);
                g.frame(vec![primary_event(end, false)]);
                let after = g.app.document.geometry_snapshot();
                if loop_selection {
                    g.save(&format!("{name}-applied"));
                }
                assert_eq!(after.mesh.vertices, preview.vertices);
                assert_eq!(after.mesh.vertex_groups.len(), 1);
                assert_eq!(g.app.document.history_counts().0, history.0 + 1);
                g.app.dispatch(Command::Undo, &g.ctx);
                assert_eq!(g.app.document.geometry_snapshot(), before);
                g.app.dispatch(Command::Redo, &g.ctx);
                assert_eq!(g.app.document.geometry_snapshot(), after);
                let saved = crate::persistence::json_bytes(&g.app.document.project()).unwrap();
                let restored: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
                restored.validate().unwrap();
            }
        }
    }
}
