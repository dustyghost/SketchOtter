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
fn topology_regions_controller_previews_cancel_commit_once_undo_redo_and_save() {
    for edit in [
        TopologyEdit::BridgeChains,
        TopologyEdit::FlipFaces,
        TopologyEdit::DissolveFaces,
    ] {
        let (mut app, ctx) = setup(edit);
        let before = app.document.geometry_snapshot();
        let counts = app.document.history_counts();
        app.dispatch(Command::ModelTopology(edit), &ctx);
        let preview = app
            .viewport
            .modeling
            .preview()
            .expect(&app.ui.status)
            .clone();
        assert_eq!(app.document.geometry_snapshot(), before);
        assert_eq!(app.document.history_counts(), counts);
        app.dispatch(Command::CancelModelTopology, &ctx);
        assert!(app.viewport.modeling.topology_preview_label().is_none());
        assert_eq!(app.document.history_counts(), counts);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        app.dispatch(Command::ApplyModelTopology, &ctx);
        assert_eq!(app.document.mesh(), &preview, "{}", app.ui.status);
        assert_eq!(app.document.history_counts().0, counts.0 + 1);
        app.dispatch(Command::Undo, &ctx);
        assert_eq!(app.document.geometry_snapshot(), before);
        app.dispatch(Command::Redo, &ctx);
        assert_eq!(app.document.mesh(), &preview);
        let saved = serde_json::to_vec(&app.document.project()).unwrap();
        let reopened: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
        reopened.validate().unwrap();
        assert_eq!(reopened.mesh, preview);
    }
}

#[test]
fn topology_regions_controller_protection_stale_settings_selection_and_failed_retry() {
    for edit in [
        TopologyEdit::BridgeChains,
        TopologyEdit::FlipFaces,
        TopologyEdit::DissolveFaces,
    ] {
        let (mut app, ctx) = setup(edit);
        let before = app.document.geometry_snapshot();
        app.dispatch(Command::ModelTopology(edit), &ctx);
        app.ui.modeling.protect_mask = false;
        app.dispatch(Command::ApplyModelTopology, &ctx);
        assert!(app.ui.status.contains("changed"));
        assert_eq!(app.document.geometry_snapshot(), before);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        app.viewport.modeling.selected.clear();
        app.dispatch(Command::ApplyModelTopology, &ctx);
        assert_eq!(app.document.geometry_snapshot(), before);
        select(&mut app, edit == TopologyEdit::BridgeChains);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        app.dispatch(Command::ModelTopology(TopologyEdit::CreateFace), &ctx);
        assert!(app.viewport.modeling.topology_preview_label().is_none());
        app.ui.modeling.protect_mask = true;
        let mut masked = before.clone();
        masked.mesh.mask.fill(1.);
        app.document.replace_geometry(masked).unwrap();
        select(&mut app, edit == TopologyEdit::BridgeChains);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert!(app.ui.status.contains("masked"), "{}", app.ui.status);
        assert!(app.viewport.modeling.topology_preview_label().is_none());
        app.ui.modeling.protect_mask = false;
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert!(
            app.viewport.modeling.topology_preview_label().is_some(),
            "{}",
            app.ui.status
        );
        let mut hidden = before;
        hidden.mesh.hidden_faces = vec![true; hidden.mesh.triangles.len()];
        app.document.replace_geometry(hidden).unwrap();
        select(&mut app, edit == TopologyEdit::BridgeChains);
        app.dispatch(Command::ModelTopology(edit), &ctx);
        assert!(app.ui.status.contains("hidden"), "{}", app.ui.status);
        assert!(app.viewport.modeling.topology_preview_label().is_none());
    }
}

#[test]
fn topology_regions_bridge_options_invalidate_preview_and_presets_remain_compatible() {
    let (mut app, ctx) = setup(TopologyEdit::BridgeChains);
    app.dispatch(Command::ModelTopology(TopologyEdit::BridgeChains), &ctx);
    let before = app.document.geometry_snapshot();
    app.ui.modeling.bridge_segments = 4;
    app.dispatch(Command::ApplyModelTopology, &ctx);
    assert!(app.ui.status.contains("changed"));
    assert_eq!(app.document.geometry_snapshot(), before);
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
