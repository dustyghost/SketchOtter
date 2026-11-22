//! Retopology application integration, shared by both test partitions.
use super::test_support::*;
use super::*;
use crate::{document::ProjectionSource, sculpt::Geometry};

#[test]
fn guided_retopology_keeps_source_and_attributes_and_roundtrips_as_one_edit() {
    use crate::sculpt::{Material, paths::PathMode, retopology::tests::fixture};
    let (mut mesh, path) = fixture(true);
    mesh.create_vertex_group("Painted density".into(), 0.5, true)
        .unwrap();
    mesh.vertex_groups[0].weights.clone_from(&mesh.mask);
    mesh.materials = mesh
        .vertices
        .iter()
        .zip(&mesh.mask)
        .map(|(v, &heat)| Material::from_srgb([0.5 + 0.25 * v.1, 0.2, 0.3], 0.5, heat))
        .collect();
    let mut app = App::default();
    app.document
        .replace_geometry(Geometry::new(mesh.clone()))
        .unwrap();
    let id = app
        .document
        .create_path("Guide".into(), PathMode::Surface, path.points, false, 0.)
        .unwrap();
    app.document.mark_saved("source.clay");
    let before = app.document.project();
    let undo = app.document.history_counts().0;
    let ctx = egui::Context::default();
    app.dispatch(Command::Action(Action::GuidedRetopology), &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::CancelProjection, &ctx);
    assert!(app.document.project() == before && !app.document.is_dirty());
    app.dispatch(Command::OpenRetopology, &ctx);
    app.ui.projection.flow_paths = vec![id];
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.document.project() == before && !app.document.is_dirty());
    let result = app
        .projection
        .draft
        .as_ref()
        .expect("private layout")
        .mesh
        .clone();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.mesh(), result.as_ref());
    assert_eq!(app.document.history_counts().0, undo + 1);
    let reference = app.document.model(1).unwrap();
    assert_eq!(reference.mesh, &mesh);
    assert!(!reference.metadata.visible);
    assert_eq!(
        reference.metadata.model_id,
        app.document.path(id).unwrap().target
    );
    assert_eq!(result.mask.len(), result.vertices.len());
    assert_eq!(result.materials.len(), result.vertices.len());
    assert_eq!(result.vertex_groups[0].name, "Painted density");
    assert_eq!(result.vertex_groups[0].weights, result.mask);
    assert!(
        result
            .materials
            .iter()
            .zip(&result.mask)
            .all(|(m, w)| (m.metallic - w).abs() < 1e-6)
    );
    assert!(result.polygroups.iter().all(|&id| id == 1));
    assert!(result.mask.iter().any(|w| *w > 0.9));
    assert!(result.mask.iter().any(|w| *w < 0.1));
    let saved = app.document.project();
    let mut reopened = Document::default();
    reopened
        .replace_project(
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap(),
            "layout.clay",
        )
        .unwrap();
    assert!(reopened.project() == saved);
    app.document.undo();
    assert!(app.document.project() == before);
    app.document.redo();
    assert!(app.document.project() == saved);
    // A subdivision adds samples between surviving vertices for detail recovery.
    app.dispatch(Command::Subdivide, &ctx);
    finish_mesh_job(&mut app);
    let topology = app.document.mesh().quads.clone();
    app.dispatch(Command::OpenProjection, &ctx);
    assert_eq!(
        app.ui.projection.source,
        Some(ProjectionSource::Submodel(1))
    );
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.ui.projection.report.as_ref().unwrap().can_apply());
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.model(1).unwrap().mesh, &mesh);
    assert_eq!(app.document.mesh().quads, topology);
}

#[test]
fn guided_retopology_input_limit_allows_larger_preview_and_apply() {
    use crate::sculpt::{Primitive, PrimitiveKind};
    let mut primitive = Primitive::for_kind(PrimitiveKind::Plane);
    primitive.divisions = [16, 16, 1];
    let mesh = primitive.mesh().unwrap();
    let triangles = mesh.triangles.len();
    let mut app = App::default();
    app.document.replace_geometry(Geometry::new(mesh)).unwrap();
    app.document.mark_saved("large-retopology-source.clay");
    let before = app.document.project();
    let history = app.document.history_counts();
    let ctx = egui::Context::default();
    app.dispatch(Command::Action(Action::GuidedRetopology), &ctx);
    assert!(
        app.ui.projection.open,
        "Larger sources can reach the limit control"
    );
    app.ui.projection.layout.input_triangle_limit = triangles - 1;
    app.dispatch(Command::PreviewProjection, &ctx);
    assert!(!app.ui.projection.busy);
    assert!(app.projection.draft.is_none());
    assert!(
        app.ui.status.contains("Input triangle limit"),
        "{}",
        app.ui.status
    );
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts(), history);

    app.ui.projection.layout.input_triangle_limit = triangles;
    app.ui.projection.layout.target = 64;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    let draft = app
        .projection
        .draft
        .as_ref()
        .expect("Raised limit permits preview");
    assert!(!draft.mesh.quads.is_empty());
    assert_eq!(
        draft.settings.view_input.as_ref().unwrap()["guided_retopology"]["input_triangle_limit"],
        triangles
    );
    assert!(app.document.project() == before && !app.document.is_dirty());
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.model_count(), 2);
    assert!(!app.document.mesh().quads.is_empty());
    assert_eq!(
        app.document.model(1).unwrap().mesh.triangles.len(),
        triangles
    );
    app.dispatch(Command::Undo, &ctx);
    assert!(app.document.project() == before);
    app.dispatch(Command::OpenRetopology, &ctx);
    assert_eq!(app.ui.projection.layout.input_triangle_limit, triangles);
}

#[test]
fn guided_retopology_rejects_failed_and_stale_previews_without_adding_models() {
    use crate::sculpt::retopology::tests::fixture;
    let (mut mesh, _) = fixture(false);
    mesh.triangles.push(mesh.triangles[0]);
    let mut app = App::default();
    app.document.replace_geometry(Geometry::new(mesh)).unwrap();
    let ctx = egui::Context::default();
    let before = app.document.project();
    let history = app.document.history_counts();
    app.dispatch(Command::OpenRetopology, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.projection.draft.is_none());
    assert!(app.ui.status.contains("manifold"));
    assert!(app.document.project() == before);
    assert_eq!(app.document.history_counts(), history);

    app.dispatch(Command::CancelProjection, &ctx);
    let (mesh, _) = fixture(false);
    app.document.replace_geometry(Geometry::new(mesh)).unwrap();
    app.dispatch(Command::OpenRetopology, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    app.document.clear_mask();
    let edited = app.document.project();
    let history = app.document.history_counts();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert!(app.document.project() == edited);
    assert_eq!(app.document.history_counts(), history);
    assert!(app.ui.status.contains("Scene changed"));
}

#[test]
fn guided_retopology_inherits_a_single_active_mirror_plane() {
    use crate::sculpt::{Axis, Mesh, Symmetry, Vec3, retopology::SymmetrySource};
    let mut app = App::default();
    app.document
        .replace_geometry(Geometry::new(Mesh::starting_sphere()))
        .unwrap();
    let ctx = egui::Context::default();
    let center = Vec3(0.2, -0.3, 0.4);
    app.document
        .set_symmetry(Symmetry {
            enabled: true,
            mirror: [false, false, true],
            center,
            ..Default::default()
        })
        .unwrap();
    app.dispatch(Command::OpenRetopology, &ctx);
    assert_eq!(app.ui.projection.layout.symmetry_axis, Some(Axis::Z));
    assert_eq!(app.ui.projection.layout.symmetry_center, center);
    assert_eq!(
        app.ui.projection.layout.symmetry_source,
        SymmetrySource::Original
    );
    app.dispatch(Command::CancelProjection, &ctx);
    let mut symmetry = app.document.symmetry().clone();
    symmetry.mirror[0] = true;
    app.document.set_symmetry(symmetry).unwrap();
    app.dispatch(Command::OpenRetopology, &ctx);
    assert_eq!(app.ui.projection.layout.symmetry_axis, None);
}

#[test]
fn guided_retopology_opens_after_path_brush_work_is_complete() {
    use crate::{command::PathCommand, sculpt::paths::PathPoint, viewport::paths::PathEditor};

    let (mut app, ctx) = setup_projection();
    app.dispatch(Command::Path(PathCommand::ActivateBrush), &ctx);
    assert!(app.ui.path_brush_active);
    let mut draft = PathEditor::new_draft(&app.document, crate::sculpt::paths::PathMode::Free);
    draft.points.push(PathPoint {
        position: crate::sculpt::Vec3::default(),
        normal: crate::sculpt::Vec3(0., 0., 1.),
    });
    app.viewport.paths.select(draft.clone());
    app.dispatch(Command::OpenRetopology, &ctx);
    assert!(!app.ui.projection.open);
    assert_eq!(app.viewport.paths.draft.as_ref(), Some(&draft));
    app.dispatch(Command::Path(PathCommand::Discard), &ctx);
    app.dispatch(Command::OpenRetopology, &ctx);
    assert!(app.ui.projection.open, "{}", app.ui.status);
    assert!(!app.viewport.paths.active);
}
