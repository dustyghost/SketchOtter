use super::*;
use crate::{
    sculpt::{Geometry, Mesh, Vec3},
    viewport::{Component, ComponentMode, ModelTool, MoveSnap},
};

fn snap_option(g: &mut Guide, label: &str, expected: MoveSnap) {
    reach_sidebar_control(g, "Snap and weld on move");
    g.frame(vec![]);
    let heading = g
        .text_positions
        .iter()
        .find(|(text, _)| text == "Snap and weld on move")
        .unwrap()
        .1;
    let point = g
        .text_positions
        .iter()
        .find(|(text, p)| text == label && p.y > heading.y && p.y < heading.y + 65.)
        .unwrap()
        .1;
    g.frame(vec![
        egui::Event::PointerMoved(point),
        primary_event(point, true),
    ]);
    g.frame(vec![primary_event(point, false), egui::Event::PointerGone]);
    assert_eq!(g.app.ui.modeling.move_snap, expected);
}

#[test]
#[ignore = "requires Vulkan; Move snap options, live welding, Shift override and history"]
fn move_snap_weld_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for perspective in [false, true] {
        let mut g = Guide::new(&directory, [1100, 900]);
        g.interactive = true;
        g.toolbar = false;
        g.panel(Tool::Mesh, Dock::Right);
        g.app.camera.yaw = 0.;
        g.app.camera.pitch = 0.;
        g.app.camera.roll = 0.1;
        g.app.camera.zoom = 0.8;
        g.app.camera.perspective = perspective;
        for edge_mode in [false, true] {
            let mesh = if edge_mode {
                Mesh {
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
                }
            } else {
                Mesh {
                    vertices: vec![
                        Vec3(-1., 0.5, 0.),
                        Vec3(-0.3, -0.5, 0.),
                        Vec3(0.3, -0.5, 0.),
                        Vec3(1., 0.5, 0.),
                    ],
                    loose_edges: vec![[0, 1], [0, 2], [2, 3]],
                    mask: vec![0.; 4],
                    ..Default::default()
                }
            };
            g.app
                .document
                .replace_geometry(Geometry::new(mesh.clone()))
                .unwrap();
            g.app.dispatch(Command::ModelingMode(true), &g.ctx);
            g.app.dispatch(
                Command::ModelingComponent(if edge_mode {
                    ComponentMode::Edge
                } else {
                    ComponentMode::Vertex
                }),
                &g.ctx,
            );
            g.app
                .execute_action(Action::ModelTool(ModelTool::Move), &g.ctx);
            g.frame(vec![]);
            snap_option(&mut g, "Off", MoveSnap::Off);
            snap_option(
                &mut g,
                if edge_mode { "Edge" } else { "Vertex" },
                if edge_mode {
                    MoveSnap::Edge
                } else {
                    MoveSnap::Vertex
                },
            );
            let rect = g.frame(vec![]).0;
            let project = |p: Vec3| {
                let (right, up, _) = g.app.camera.basis();
                rect.center()
                    + g.app.camera.pan
                    + Vec2::new(p.dot(right), -p.dot(up)) * g.app.camera.scale_at(rect.size(), p)
            };
            let start = project(if edge_mode {
                (mesh.vertices[1] + mesh.vertices[2]) * 0.5
            } else {
                mesh.vertices[1]
            });
            let target = project(if edge_mode {
                (mesh.vertices[4] + mesh.vertices[7]) * 0.5
            } else {
                mesh.vertices[2]
            }) + Vec2::new(-3., 2.);
            let before = g.app.document.geometry_snapshot();
            let history = g.app.document.history_counts().0;
            g.frame(vec![
                egui::Event::PointerMoved(start),
                primary_event(start, true),
            ]);
            g.frame(vec![egui::Event::PointerMoved(target)]);
            let count = if edge_mode { 6 } else { 3 };
            if g.app.viewport.modeling.preview().is_none() {
                g.save("move-snap-missing-preview");
                panic!(
                    "No preview: {perspective} {edge_mode}; start {start:?}, target {target:?}, mode {:?}, tool {:?}, active {}, status {}",
                    g.app.ui.modeling.mode,
                    g.app.ui.modeling.tool,
                    g.app.ui.modeling.active,
                    g.app.ui.status
                );
            }
            assert_eq!(g.app.document.geometry_snapshot(), before);
            assert_eq!(
                g.app.viewport.modeling.preview().unwrap().vertices.len(),
                count,
                "{}",
                g.app.ui.status
            );
            g.save(&format!(
                "move-snap-{}-{perspective}",
                if edge_mode { "edge" } else { "vertex" }
            ));
            g.modifiers = egui::Modifiers::SHIFT;
            g.frame(vec![]);
            assert_eq!(
                g.app.viewport.modeling.preview().unwrap().vertices.len(),
                mesh.vertices.len()
            );
            g.modifiers = egui::Modifiers::NONE;
            g.frame(vec![]);
            assert_eq!(
                g.app.viewport.modeling.preview().unwrap().vertices.len(),
                count
            );
            g.frame(vec![egui::Event::PointerMoved(
                target + Vec2::new(-55., 0.),
            )]);
            assert_eq!(
                g.app.viewport.modeling.preview().unwrap().vertices.len(),
                mesh.vertices.len()
            );
            g.frame(vec![egui::Event::PointerMoved(target)]);
            g.frame(vec![primary_event(target, false)]);
            let after = g.app.document.geometry_snapshot();
            assert_eq!(after.mesh.vertices.len(), count);
            assert_eq!(g.app.document.history_counts().0, history + 1);
            assert!(g.app.viewport.modeling.last_edit.is_none());
            if edge_mode {
                assert_eq!(after.mesh.quads.len(), 2);
                assert_eq!(after.mesh.model_edges().len(), 7);
                assert!(matches!(
                    g.app.viewport.modeling.selected.first(),
                    Some(Component::Edge(_))
                ));
            } else {
                assert_eq!(after.mesh.loose_edges.len(), 2);
            }
            g.app.dispatch(Command::Undo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), before);
            g.app.dispatch(Command::Redo, &g.ctx);
            assert_eq!(g.app.document.geometry_snapshot(), after);
            let saved = crate::persistence::json_bytes(&g.app.document.project()).unwrap();
            let restored: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
            restored.validate().unwrap();
            assert_eq!(restored.mesh, after.mesh);
            g.app.dispatch(Command::Undo, &g.ctx);
            g.app.execute_action(Action::ModelClearSelection, &g.ctx);
            g.frame(vec![]);
            let history = g.app.document.history_counts();
            g.frame(vec![
                egui::Event::PointerMoved(start),
                primary_event(start, true),
            ]);
            g.frame(vec![egui::Event::PointerMoved(target)]);
            g.frame(vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }]);
            g.frame(vec![
                primary_event(target, false),
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
        }
    }
}
