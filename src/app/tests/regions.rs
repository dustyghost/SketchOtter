use super::*;
use crate::{
    sculpt::{Geometry, Mesh, TopologyEdit, Vec3},
    viewport::{Component, ComponentMode},
};

pub(super) fn fixture(bridge: bool) -> Mesh {
    if bridge {
        Mesh {
            vertices: vec![
                Vec3(-2., -1., 0.),
                Vec3(-1., -1., 0.),
                Vec3(-1., 1., 0.),
                Vec3(-2., 1., 0.),
                Vec3(1., -1., 0.),
                Vec3(2., -1., 0.),
                Vec3(2., 1., 0.),
                Vec3(1., 1., 0.),
            ],
            mask: vec![0.; 8],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            quads: vec![[0, 1, 2, 3], [4, 5, 6, 7]],
            ..Default::default()
        }
    } else {
        Mesh {
            vertices: vec![
                Vec3(-1., -1., 0.),
                Vec3(1., -1., 0.),
                Vec3(1., 1., 0.),
                Vec3(-1., 1., 0.),
                Vec3(0., 0., 0.),
            ],
            mask: vec![0.; 5],
            triangles: vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
            ..Default::default()
        }
    }
}
fn setup(edit: TopologyEdit) -> (App, egui::Context) {
    let mut app = App::default();
    let ctx = egui::Context::default();
    let bridge = edit == TopologyEdit::BridgeChains;
    app.document
        .replace_geometry(Geometry::new(fixture(bridge)))
        .unwrap();
    app.dispatch(Command::ModelingMode(true), &ctx);
    app.dispatch(
        Command::ModelingComponent(if bridge {
            ComponentMode::Edge
        } else {
            ComponentMode::Face
        }),
        &ctx,
    );
    select(&mut app, bridge);
    app.ui.modeling.bridge_segments = 3;
    (app, ctx)
}
fn select(app: &mut App, bridge: bool) {
    app.viewport.modeling.sync(&app.document);
    app.viewport.modeling.selected = if bridge {
        [Component::Edge([1, 2]), Component::Edge([4, 7])].into()
    } else {
        (0..4).map(Component::Face).collect()
    };
}

#[test]
fn topology_regions_controller_commits_immediately_with_groups_undo_redo_and_save() {
    for edit in [
        TopologyEdit::BridgeChains,
        TopologyEdit::FlipFaces,
        TopologyEdit::DissolveFaces,
    ] {
        for group in [false, true] {
            let (mut app, ctx) = setup(edit);
            app.ui.modeling.new_group = group;
            let before = app.document.geometry_snapshot();
            let counts = app.document.history_counts();
            app.dispatch(Command::ModelTopology(edit), &ctx);
            let after = app.document.geometry_snapshot();
            assert_ne!(after, before, "{}", app.ui.status);
            assert!(app.viewport.modeling.preview().is_none());
            assert_eq!(app.document.history_counts().0, counts.0 + 1);
            assert_eq!(after.mesh.vertex_groups.len(), usize::from(group));
            assert!(app.viewport.modeling.selected.is_empty());
            match edit {
                TopologyEdit::BridgeChains => assert_eq!(after.mesh.quads.len(), 5),
                TopologyEdit::DissolveFaces => assert_eq!(after.mesh.quads.len(), 1),
                TopologyEdit::FlipFaces => assert_eq!(after.mesh.triangles[0], [0, 4, 1]),
                _ => unreachable!(),
            }
            app.dispatch(Command::Undo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), before);
            app.dispatch(Command::Redo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), after);
            let saved = serde_json::to_vec(&app.document.project()).unwrap();
            let reopened: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
            reopened.validate().unwrap();
            assert_eq!(reopened.mesh, after.mesh);
            // A failed edit after Undo must preserve geometry, selection and Redo.
            app.dispatch(Command::Undo, &ctx);
            app.viewport.modeling.sync(&app.document);
            let selection = if edit == TopologyEdit::BridgeChains {
                [Component::Edge([1, 2])].into()
            } else {
                [Component::Face(0)].into()
            };
            app.viewport.modeling.selected = selection;
            let selected = app.viewport.modeling.selected.clone();
            let history = app.document.history_counts();
            app.dispatch(Command::ModelTopology(edit), &ctx);
            assert_eq!(
                app.document.geometry_snapshot(),
                before,
                "{}",
                app.ui.status
            );
            assert_eq!(app.document.history_counts(), history);
            assert_eq!(app.viewport.modeling.selected, selected);
            app.dispatch(Command::Redo, &ctx);
            assert_eq!(app.document.geometry_snapshot(), after);
        }
    }
}

#[test]
fn topology_regions_controller_protection_and_failed_retry_are_atomic() {
    for edit in [
        TopologyEdit::BridgeChains,
        TopologyEdit::FlipFaces,
        TopologyEdit::DissolveFaces,
    ] {
        let (mut app, ctx) = setup(edit);
        let before = app.document.geometry_snapshot();
        let mut masked = before.clone();
        masked.mesh.mask.fill(1.);
        app.document.replace_geometry(masked.clone()).unwrap();
        select(&mut app, edit == TopologyEdit::BridgeChains);
        let history = app.document.history_counts();
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert!(app.ui.status.contains("masked"), "{}", app.ui.status);
        assert_eq!(app.document.geometry_snapshot(), masked);
        assert_eq!(app.document.history_counts(), history);
        app.ui.modeling.protect_mask = false;
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert_ne!(
            app.document.geometry_snapshot(),
            masked,
            "{}",
            app.ui.status
        );
        let mut hidden = before;
        hidden.mesh.hidden_faces = vec![true; hidden.mesh.triangles.len()];
        app.document.replace_geometry(hidden.clone()).unwrap();
        select(&mut app, edit == TopologyEdit::BridgeChains);
        let history = app.document.history_counts();
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert!(app.ui.status.contains("hidden"), "{}", app.ui.status);
        assert_eq!(app.document.geometry_snapshot(), hidden);
        assert_eq!(app.document.history_counts(), history);
    }
}

#[test]
fn topology_regions_bridge_uses_current_segments_and_presets_remain_compatible() {
    let (mut app, ctx) = setup(TopologyEdit::BridgeChains);
    app.ui.modeling.bridge_segments = 4;
    app.dispatch(Command::ModelTopology(TopologyEdit::BridgeChains), &ctx);
    assert_eq!(app.document.mesh().quads.len(), 6, "{}", app.ui.status);
    let options = app.ui.modeling.options();
    let mut legacy = serde_json::to_value(options).unwrap();
    legacy.as_object_mut().unwrap().remove("bridge_segments");
    let restored: crate::viewport::ModelingOptions = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.bridge_segments, 1);
    restored.validate().unwrap();
    let restored: crate::viewport::ModelingOptions =
        serde_json::from_value(serde_json::to_value(options).unwrap()).unwrap();
    assert_eq!(restored, options);
    assert!(
        crate::viewport::ModelingOptions {
            bridge_segments: 0,
            ..options
        }
        .validate()
        .is_err()
    );
}

#[test]
fn topology_regions_preserve_loose_edges_and_detached_points() {
    for edit in [
        TopologyEdit::BridgeChains,
        TopologyEdit::FlipFaces,
        TopologyEdit::DissolveFaces,
    ] {
        let (mut app, ctx) = setup(edit);
        let mut source = app.document.geometry_snapshot();
        let mesh = &mut source.mesh;
        let n = mesh.vertices.len();
        let points = [Vec3(3., 0., 1.), Vec3(3., 1., 1.), Vec3(4., 0., 1.)];
        mesh.vertices.extend(points);
        mesh.mask.resize(mesh.vertices.len(), 0.);
        mesh.loose_edges.push([n, n + 1]);
        mesh.loose_vertices.push(n + 2);
        app.document.replace_geometry(source.clone()).unwrap();
        select(&mut app, edit == TopologyEdit::BridgeChains);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        let mesh = app.document.mesh();
        assert_ne!(mesh, &source.mesh, "{}", app.ui.status);
        assert_eq!(mesh.loose_edges.len(), 1);
        assert_eq!(mesh.loose_vertices.len(), 1);
        assert_eq!(
            mesh.loose_edges[0].map(|v| mesh.vertices[v]),
            [points[0], points[1]]
        );
        assert_eq!(mesh.vertices[mesh.loose_vertices[0]], points[2]);
        mesh.validate().unwrap();
    }
}
