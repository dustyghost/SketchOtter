use super::*;
use crate::{
    document::ProjectionSource,
    sculpt::{Geometry, Mesh, ProjectionOptions},
};

pub(super) fn finish_projection(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.ui.projection.busy {
        app.poll_projection();
        assert!(std::time::Instant::now() < deadline, "projection timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
fn setup_projection() -> (App, egui::Context) {
    let mut app = App::default();
    app.document
        .replace_geometry(Geometry::new(Mesh::sphere()))
        .unwrap();
    let ctx = egui::Context::default();
    app.dispatch(Command::KeepProjectionReference, &ctx);
    let mut geometry = app.document.geometry_snapshot();
    for p in &mut geometry.mesh.vertices {
        *p = *p * 0.9;
    }
    app.document.replace_geometry(geometry).unwrap();
    app.document.mark_saved("projection.clay");
    (app, ctx)
}
#[test]
fn projection_preview_is_private_apply_is_one_undo_and_reference_persists() {
    let (mut app, ctx) = setup_projection();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    app.dispatch(Command::OpenProjection, &ctx);
    assert_eq!(
        app.ui.projection.source,
        Some(ProjectionSource::Submodel(1))
    );
    app.ui.projection.options.max_distance = 0.2;
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::SelectSubmodel(1), &ctx);
    app.dispatch(Command::Remesh, &ctx);
    finish_projection(&mut app);
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
    assert!(
        app.ui.projection.report.as_ref().unwrap().can_apply(),
        "{}",
        app.ui.status
    );
    let preview = app.projection.draft.as_ref().unwrap().mesh.clone();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.mesh(), preview.as_ref());
    assert_eq!(app.document.geometry_snapshot().submodels, before.submodels);
    assert_eq!(app.document.undo_label(), Some("Project details"));
    assert_eq!(app.document.history_counts().0, history.0 + 1);
    let after = app.document.geometry_snapshot();
    let encoded = serde_json::to_string(&app.document.project()).unwrap();
    let mut loaded = Document::default();
    loaded
        .replace_project(serde_json::from_str(&encoded).unwrap(), "saved.clay")
        .unwrap();
    assert_eq!(loaded.geometry_snapshot(), after);
    assert!(!loaded.model(1).unwrap().metadata.visible);
    app.document.undo();
    assert_eq!(app.document.geometry_snapshot(), before);
    app.document.redo();
    assert_eq!(app.document.geometry_snapshot(), after);
}
#[test]
fn projection_cancellation_noop_and_stale_draft_preserve_history_and_dirty_state() {
    let (mut app, ctx) = setup_projection();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    app.dispatch(Command::OpenProjection, &ctx);
    app.ui.projection.options.max_distance = 0.2;
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::CancelProjection, &ctx);
    app.poll_projection();
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
    app.dispatch(Command::OpenProjection, &ctx);
    app.ui.projection.options.strength = 0.;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    assert!(!app.document.is_dirty());
    app.dispatch(Command::OpenProjection, &ctx);
    app.ui.projection.options.strength = 1.;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    app.dispatch(Command::CancelProjection, &ctx);
    assert_eq!(app.document.geometry_snapshot(), before);
    app.dispatch(Command::OpenProjection, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    app.document.duplicate_model(false).unwrap(); // Defensive guard against external scene changes.
    let changed = app.document.geometry_snapshot();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.geometry_snapshot(), changed);
    assert!(app.ui.status.contains("scene changed"), "{}", app.ui.status);
}
#[test]
fn projection_preserved_reference_survives_remesh_and_lower_level_sources_keep_stack() {
    let mut app = App::default();
    let ctx = egui::Context::default();
    app.dispatch(Command::KeepProjectionReference, &ctx);
    let source = app.document.model(1).unwrap().mesh.clone();
    app.dispatch(Command::SetDynamicMesh(true), &ctx);
    app.dispatch(Command::SetRemeshResolution(24), &ctx);
    app.dispatch(Command::Remesh, &ctx);
    finish_mesh_job(&mut app);
    app.dispatch(Command::OpenProjection, &ctx);
    app.ui.projection.options.max_distance = 0.2;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.ui.projection.report.as_ref().unwrap().can_apply());
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.model(1).unwrap().mesh, &source);
    app.document.mesh().validate_closed().unwrap();

    let mut geometry = Geometry::new(Mesh::sphere());
    geometry.subdivide().unwrap();
    app.document.replace_geometry(geometry).unwrap();
    let low = app
        .document
        .projection_source(ProjectionSource::Level(1))
        .unwrap();
    let (result, _) = app
        .document
        .mesh()
        .project_details(&low, ProjectionOptions::default(), &|| false)
        .unwrap();
    app.document
        .apply_projection(result, app.document.display_revision(), None)
        .unwrap();
    assert_eq!(app.document.mesh_settings().levels(), 2);
    app.document.set_mesh_level(1).unwrap();
    app.document.set_mesh_level(2).unwrap();
    app.document
        .mesh_settings()
        .validate(app.document.mesh())
        .unwrap();
    assert!(
        app.document
            .projection_source(ProjectionSource::Level(2))
            .is_err()
    );
    assert!(
        app.document
            .projection_source(ProjectionSource::Level(7))
            .is_err()
    );
}
#[test]
fn projection_navigation_cannot_sculpt_and_escape_discards_preview() {
    let (mut app, ctx) = setup_projection();
    app.dispatch(Command::OpenProjection, &ctx);
    app.ui.projection.options.max_distance = 0.2;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    let before = app.document.geometry_snapshot();
    let rect = navigation_frame(&mut app, &ctx, vec![]);
    let pos = rect.center();
    let yaw = app.camera.yaw;
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(pos), primary_event(pos, true)],
    );
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(pos + Vec2::new(40., 10.))],
    );
    navigation_frame(
        &mut app,
        &ctx,
        vec![primary_event(pos + Vec2::new(40., 10.), false)],
    );
    assert_ne!(app.camera.yaw, yaw);
    assert_eq!(app.document.geometry_snapshot(), before);
    navigation_frame(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert!(!app.ui.projection.open);
    assert!(app.projection.draft.is_none());
    assert_eq!(app.document.geometry_snapshot(), before);
    assert!(!app.document.is_dirty());
}

#[test]
fn projection_rejects_incompatible_levels_and_noop_keeps_redo() {
    let mut document = Document::default();
    document.preserve_projection_reference().unwrap();
    document.undo();
    let history = document.history_counts();
    document.mark_saved("unchanged.clay");
    document
        .apply_projection(document.mesh().clone(), document.display_revision(), None)
        .unwrap();
    assert_eq!(document.history_counts(), history);
    assert!(document.can_redo());
    assert!(!document.is_dirty());
    let mut geometry = document.geometry_snapshot();
    geometry.subdivide().unwrap();
    geometry.settings.lower[0].triangles.pop();
    // Invalid correspondence is rejected before it reaches persistent state.
    assert!(document.replace_geometry(geometry).is_err());
    assert_eq!(document.history_counts(), history);
}

#[test]
fn relaxation_preview_apply_repeat_reference_undo_and_persistence() {
    let mut app = App::default();
    let ctx = egui::Context::default();
    app.document
        .replace_geometry(Geometry::new(crate::sculpt::relaxation_fixture()))
        .unwrap();
    app.document.duplicate_model(false).unwrap();
    let before = app.document.geometry_snapshot();
    let history = app.document.history_counts();
    app.document.mark_saved("relaxation.clay");
    app.dispatch(Command::Action(Action::RelaxMesh), &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(
        app.ui.projection.report.as_ref().unwrap().can_apply(),
        "{}",
        app.ui.status
    );
    assert_eq!(app.document.geometry_snapshot(), before);
    assert_eq!(app.document.history_counts(), history);
    app.dispatch(Command::CancelProjection, &ctx);
    assert!(!app.document.is_dirty());
    app.dispatch(Command::OpenRelaxation, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    let reference = app.projection.reference.as_ref().unwrap().1.clone();
    let draft = app.projection.draft.as_ref().unwrap().mesh.clone();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.mesh(), draft.as_ref());
    assert_eq!(app.document.undo_label(), Some("Relax mesh"));
    assert_eq!(app.document.history_counts().0, history.0 + 1);
    assert_eq!(app.document.geometry_snapshot().submodels, before.submodels);
    app.dispatch(Command::OpenRelaxation, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(std::sync::Arc::ptr_eq(
        &reference,
        &app.projection.reference.as_ref().unwrap().1
    ));
    app.dispatch(Command::ApplyProjection, &ctx);
    let after = app.document.geometry_snapshot();
    let encoded = serde_json::to_string(&app.document.project()).unwrap();
    let mut loaded = Document::default();
    loaded
        .replace_project(serde_json::from_str(&encoded).unwrap(), "saved.clay")
        .unwrap();
    assert_eq!(loaded.geometry_snapshot(), after);
    app.dispatch(Command::Undo, &ctx);
    app.dispatch(Command::Undo, &ctx);
    assert_eq!(app.document.geometry_snapshot(), before);
    app.dispatch(Command::Redo, &ctx);
    app.dispatch(Command::Redo, &ctx);
    assert_eq!(app.document.geometry_snapshot(), after);
    app.dispatch(Command::OpenRelaxation, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(!std::sync::Arc::ptr_eq(
        &reference,
        &app.projection.reference.as_ref().unwrap().1
    ));
    app.document.duplicate_model(false).unwrap();
    let changed = app.document.geometry_snapshot();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.geometry_snapshot(), changed);
    assert!(app.ui.status.contains("scene changed"));
}

#[test]
fn relaxation_retains_subdivision_stack_and_noop_preserves_redo() {
    let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
    p.divisions = [4; 3];
    let mut mesh = p.mesh().unwrap();
    for v in &mut mesh.vertices {
        v.0 += 0.2 * (v.0 * std::f32::consts::PI).sin();
    }
    let mut geometry = Geometry::new(mesh);
    geometry.subdivide().unwrap();
    let mut document = Document::default();
    document.replace_geometry(geometry).unwrap();
    let before = document.geometry_snapshot();
    let (relaxed, report) = document
        .mesh()
        .relax_mesh(document.mesh(), Default::default(), &|| false)
        .unwrap();
    assert!(report.moved > 0);
    document
        .apply_relaxation(relaxed.clone(), document.display_revision(), None)
        .unwrap();
    assert_eq!(document.mesh_settings().lower, before.settings.lower);
    assert_eq!(document.mesh_settings().levels(), 2);
    document.set_mesh_level(1).unwrap();
    document.set_mesh_level(2).unwrap();
    document.mesh_settings().validate(document.mesh()).unwrap();
    assert_eq!(document.mesh(), &relaxed);
    document.undo();
    document.undo();
    document.undo();
    assert_eq!(document.geometry_snapshot(), before);
    let history = document.history_counts();
    document
        .apply_relaxation(document.mesh().clone(), document.display_revision(), None)
        .unwrap();
    assert_eq!(document.history_counts(), history);
    assert!(document.can_redo());
}

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
    let normals = mesh.normals();
    let ring = app
        .document
        .create_path(
            "Unselected ring".into(),
            PathMode::Surface,
            (9 * 32..10 * 32)
                .map(|i| crate::sculpt::paths::PathPoint {
                    position: mesh.vertices[i],
                    normal: normals[i],
                })
                .collect(),
            true,
            0.,
        )
        .unwrap();
    let free = app
        .document
        .create_path(
            "Free path".into(),
            PathMode::Free,
            app.document.path(id).unwrap().points.clone(),
            false,
            0.,
        )
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
    app.paths.reproject_distance = 1e-8;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(
        app.projection.draft.is_none(),
        "reprojection failure rejects the private result"
    );
    assert!(
        app.ui.status.contains("could not attach"),
        "{}",
        app.ui.status
    );
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts().0, undo);
    app.paths.reproject_distance = 0.2;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.document.project() == before && !app.document.is_dirty());
    let draft = app
        .projection
        .draft
        .as_ref()
        .expect("private layout with paths");
    assert_eq!(draft.retopology_paths.len(), 2);
    let surface = crate::sculpt::projection::Surface::new(&draft.mesh, &|| false).unwrap();
    let mut budget = 30_000_000;
    for path in &draft.retopology_paths {
        assert!(!path.is_stale(&draft.mesh));
        for point in &path.attached_route.as_ref().unwrap().points {
            assert!(
                surface
                    .nearest(point.position, 1e-5, None, &mut budget)
                    .unwrap()
                    .is_some(),
                "saved route lies on the private preview without another projection"
            );
        }
        let samples = path.samples_on_mesh(&draft.mesh, 0.025).unwrap();
        assert!(crate::sculpt::paths::sample_surface_error(&draft.mesh, &samples) < 1e-5);
    }
    let crate::ui::SurfaceReport::Retopology { guides, .. } =
        app.ui.projection.report.as_ref().unwrap()
    else {
        panic!("retopology report")
    };
    assert_eq!(guides.len(), 1, "only selected paths are overlaid");
    assert!(crate::sculpt::paths::sample_surface_error(&draft.mesh, &guides[0]) < 1e-5);
    let preview_mesh = (*draft.mesh).clone();
    let revision = draft.revision;
    assert!(
        app.document
            .apply_retopology(preview_mesh, Vec::new(), revision, None)
            .is_err()
    );
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts().0, undo);
    app.dispatch(Command::CancelProjection, &ctx);
    assert!(app.document.project() == before && !app.document.is_dirty());
    app.dispatch(Command::OpenRetopology, &ctx);
    assert_eq!(app.ui.projection.flow_paths, vec![id]);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
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
    assert_ne!(reference.metadata.model_id, app.document.active_model_id());
    for path_id in [id, ring] {
        let path = app.document.path(path_id).unwrap();
        assert_eq!(path.target, app.document.active_model_id());
        assert!(!path.is_stale(app.document.mesh()));
    }
    assert!(app.document.path(ring).unwrap().closed);
    assert_eq!(
        app.document.path(free),
        before.paths.iter().find(|p| p.id == free)
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
fn integer_grid_retopology_preview_apply_history_and_persistence() {
    use crate::sculpt::{Material, Primitive, PrimitiveKind, retopology::Method};
    let mut primitive = Primitive::for_kind(PrimitiveKind::Icosphere);
    primitive.subdivisions = 2;
    let mut mesh = primitive.mesh().unwrap();
    mesh.mask = mesh
        .vertices
        .iter()
        .map(|p| (p.1 + 0.5).clamp(0., 1.))
        .collect();
    mesh.materials = mesh
        .mask
        .iter()
        .map(|&w| Material::from_srgb([0.2 + 0.5 * w, 0.3, 0.4], 0.5, w))
        .collect();
    let mut app = App::default();
    app.document
        .replace_geometry(Geometry::new(mesh.clone()))
        .unwrap();
    app.document.mark_saved("integer-grid-source.clay");
    let before = app.document.project();
    let history = app.document.history_counts();
    let ctx = egui::Context::default();
    app.dispatch(Command::OpenRetopology, &ctx);
    assert_eq!(app.ui.projection.layout.method, Method::IntegerGrid);
    assert!(app.ui.projection.layout.preserve_sharp);
    app.ui.projection.layout.target = 80;
    app.ui.projection.layout.symmetry_axis = Some(crate::sculpt::Axis::X);
    app.ui.projection.layout.preserve_sharp = true;
    app.dispatch(Command::PreviewProjection, &ctx);
    assert!(!app.ui.projection.busy && app.projection.draft.is_none());
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts(), history);
    app.ui.projection.layout.symmetry_axis = None;
    app.dispatch(Command::PreviewProjection, &ctx);
    app.dispatch(Command::CancelProjection, &ctx);
    app.poll_projection();
    assert!(!app.ui.projection.busy && app.projection.draft.is_none());
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts(), history);
    app.dispatch(Command::OpenRetopology, &ctx);
    app.ui.projection.layout.target = 80;
    app.ui.projection.layout.symmetry_axis = None;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    let draft = app
        .projection
        .draft
        .as_ref()
        .unwrap_or_else(|| panic!("{} {:?}", app.ui.status, app.ui.projection.error));
    assert_eq!(
        draft.settings.view_input.as_ref().unwrap()["guided_retopology"]["backend"],
        "native_miq_integer_grid_experimental"
    );
    draft.mesh.validate_closed().unwrap();
    assert!(!draft.mesh.quads.is_empty());
    assert!(app.document.project() == before && !app.document.is_dirty());
    app.dispatch(Command::ApplyProjection, &ctx);
    assert_eq!(app.document.history_counts().0, history.0 + 1);
    assert_eq!(app.document.model(1).unwrap().mesh, &mesh);
    let result = app.document.mesh();
    assert!(result.mask.iter().any(|&w| w < 0.1) && result.mask.iter().any(|&w| w > 0.9));
    assert!(
        result
            .materials
            .iter()
            .zip(&result.mask)
            .all(|(m, w)| (m.metallic - w).abs() < 1e-6)
    );
    let saved = app.document.project();
    let mut reopened = Document::default();
    reopened
        .replace_project(
            serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap(),
            "integer-grid.clay",
        )
        .unwrap();
    assert!(reopened.project() == saved);
    app.document.undo();
    assert!(app.document.project() == before);
    app.document.redo();
    assert!(app.document.project() == saved);

    app.document.undo();
    app.dispatch(Command::OpenRetopology, &ctx);
    app.ui.projection.layout.target = 80;
    app.ui.projection.layout.symmetry_axis = None;
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.projection.draft.is_some(), "{}", app.ui.status);
    app.document.duplicate_model(false).unwrap();
    let changed = app.document.project();
    let history = app.document.history_counts();
    app.dispatch(Command::ApplyProjection, &ctx);
    assert!(app.document.project() == changed);
    assert_eq!(app.document.history_counts(), history);
    assert!(
        app.ui.status.to_lowercase().contains("scene changed"),
        "{}",
        app.ui.status
    );
}

#[test]
fn guided_retopology_input_limit_allows_larger_preview_and_apply() {
    use crate::sculpt::{Primitive, PrimitiveKind};
    let mut primitive = Primitive::for_kind(PrimitiveKind::Icosphere);
    primitive.subdivisions = 2;
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
    app.ui.projection.layout.target = 80;
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
fn guided_retopology_rejects_open_surfaces_without_changing_the_document() {
    use crate::sculpt::{Primitive, PrimitiveKind};
    let mesh = Primitive::for_kind(PrimitiveKind::Plane).mesh().unwrap();
    let mut app = App::default();
    app.document.replace_geometry(Geometry::new(mesh)).unwrap();
    app.document.mark_saved("open-retopology-source.clay");
    let before = app.document.project();
    let history = app.document.history_counts();
    let ctx = egui::Context::default();
    app.dispatch(Command::OpenRetopology, &ctx);
    app.dispatch(Command::PreviewProjection, &ctx);
    finish_projection(&mut app);
    assert!(app.projection.draft.is_none());
    assert!(app.ui.status.contains("closed"), "{}", app.ui.status);
    assert!(app.document.project() == before && !app.document.is_dirty());
    assert_eq!(app.document.history_counts(), history);
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
fn guided_retopology_does_not_inherit_unsupported_layout_symmetry() {
    use crate::sculpt::{Mesh, Symmetry, Vec3, retopology::Method};
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
    assert_eq!(app.ui.projection.layout.symmetry_axis, None);
    assert_eq!(app.ui.projection.layout.method, Method::IntegerGrid);
    assert_eq!(app.document.symmetry().center, center);
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
