//! Shared CPU worker helpers; test bodies live in their owning module.
use super::*;
use crate::sculpt::{Geometry, Mesh};

pub(super) fn finish_mesh_job(app: &mut App) {
    finish_mesh_job_with_timeout(app, std::time::Duration::from_secs(10));
}
pub(super) fn finish_mesh_job_with_timeout(app: &mut App, timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    while app.mesh_job.is_some() {
        app.poll_mesh_job();
        assert!(std::time::Instant::now() < deadline, "mesh job timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
pub(super) fn finish_projection(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.ui.projection.busy {
        app.poll_projection();
        assert!(std::time::Instant::now() < deadline, "projection timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
pub(super) fn setup_projection() -> (App, egui::Context) {
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
