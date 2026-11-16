use super::*;
use crate::{
    sculpt::Vec3,
    viewport::{Component, ComponentMode, ModelTool},
};

#[test]
#[ignore = "requires Vulkan; Ctrl detached copies, isolated-point picking, groups and history"]
fn detached_copy_offscreen_review() {
    let directory = std::path::PathBuf::from(std::env::var("CLAY_GUIDE_PREVIEW").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    for points in [false, true] {
        let mut g = Guide::new(&directory, [1100, 950]);
        g.interactive = true;
        g.toolbar = false;
        g.panel(Tool::Mesh, Dock::Right);
        let mut primitive = Primitive::for_kind(PrimitiveKind::Grid);
        primitive.divisions = [2, 1, 1];
        g.app
            .document
            .replace_geometry(crate::sculpt::Geometry::new(primitive.mesh().unwrap()))
            .unwrap();
        g.app.camera.yaw = 0.4;
        g.app.camera.pitch = -0.25;
        g.app.camera.zoom = 0.7;
        g.app.dispatch(Command::ModelingMode(true), &g.ctx);
        g.app.dispatch(
            Command::ModelingComponent(if points {
                ComponentMode::Vertex
            } else {
                ComponentMode::Face
            }),
            &g.ctx,
        );
        g.app.execute_action(
            Action::ModelTool(if points {
                ModelTool::PointExtrude
            } else {
                ModelTool::Extrude
            }),
            &g.ctx,
        );
        reach_sidebar_control(&mut g, "New vertex group after each edit");
        click_guide_label(&mut g, "New vertex group after each edit");
        assert!(g.app.ui.modeling.new_group);
        let rect = g.frame(vec![]).0;
        let project = |p: Vec3| {
            let (r, u, _) = g.app.camera.basis();
            rect.center()
                + g.app.camera.pan
                + Vec2::new(p.dot(r), -p.dot(u)) * g.app.camera.scale_at(rect.size(), p)
        };
        let mesh = g.app.document.mesh().clone();
        let root = 4;
        let start = project(if points {
            mesh.vertices[root]
        } else {
            mesh.polygons()[0]
                .vertices
                .iter()
                .fold(Vec3::default(), |p, &v| p + mesh.vertices[v])
                * 0.25
        });
        let target = start + Vec2::new(if points { 70. } else { 0. }, -75.);
        if !points {
            g.app.viewport.modeling.selected = [Component::Face(0), Component::Face(1)].into();
            g.frame(vec![]);
        }
        let before = g.app.document.geometry_snapshot();
        let history = g.app.document.history_counts().0;
        g.modifiers = egui::Modifiers::CTRL;
        g.frame(vec![
            egui::Event::PointerMoved(start),
            primary_event(start, true),
        ]);
        g.frame(vec![egui::Event::PointerMoved(target)]);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.frame(vec![primary_event(target, false)]);
        g.modifiers = egui::Modifiers::NONE;
        g.frame(vec![]);
        let after = g.app.document.geometry_snapshot();
        assert_eq!(&after.mesh.vertices[..mesh.vertices.len()], mesh.vertices);
        assert_eq!(
            &after.mesh.triangles[..mesh.triangles.len()],
            mesh.triangles
        );
        assert_eq!(
            after.mesh.vertex_groups.len(),
            mesh.vertex_groups.len() + 1,
            "{}",
            g.app.ui.status
        );
        assert_eq!(g.app.document.history_counts().0, history + 1);
        let group = after.mesh.vertex_groups.last().unwrap();
        assert!(
            group.weights[..mesh.vertices.len()]
                .iter()
                .all(|w| *w == 0.)
        );
        assert!(
            group.weights[mesh.vertices.len()..]
                .iter()
                .all(|w| *w == 1.)
        );
        if points {
            assert_eq!(after.mesh.loose_vertices, vec![mesh.vertices.len()]);
            assert!(after.mesh.loose_edges.is_empty());
        } else {
            assert_eq!(after.mesh.quads.len(), 4);
            assert_eq!(after.mesh.vertices.len(), mesh.vertices.len() * 2);
        }
        g.save(if points {
            "detached-copy-vertex"
        } else {
            "detached-copy-faces"
        });
        g.app.dispatch(Command::Undo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), before);
        g.app.dispatch(Command::Redo, &g.ctx);
        assert_eq!(g.app.document.geometry_snapshot(), after);
        let bytes = crate::persistence::json_bytes(&g.app.document.project()).unwrap();
        let restored: crate::sculpt::Project = serde_json::from_slice(&bytes).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.mesh, after.mesh);
        if points {
            g.app.execute_action(Action::ModelSelect, &g.ctx);
            g.app.execute_action(Action::ModelClearSelection, &g.ctx);
            g.frame(vec![]);
            g.frame(vec![
                egui::Event::PointerMoved(target),
                primary_event(target, true),
            ]);
            g.frame(vec![primary_event(target, false)]);
            assert_eq!(
                g.app.viewport.modeling.selected,
                [Component::Vertex(mesh.vertices.len())].into()
            );
            g.app
                .execute_action(Action::ModelTool(ModelTool::Move), &g.ctx);
            g.stroke(target, target + Vec2::new(25., 10.));
            assert_eq!(
                g.app.document.mesh().loose_vertices,
                after.mesh.loose_vertices
            );
        }
    }
}
