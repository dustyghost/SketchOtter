//! Shared private, cancellable surface-edit drafts; only Apply changes the document.
use super::*;
use crate::background_job::BackgroundJob;
use crate::sculpt::paths::{PathMode, SurfacePath};
use crate::sculpt::{Mesh, ProjectionOptions};
use crate::ui::{SurfaceMode, SurfaceReport};
use std::sync::Arc;

pub(super) struct ProjectionDraft {
    pub mesh: Arc<Mesh>,
    pub display: Option<crate::viewport::JoinDisplay>,
    pub revision: u64,
    pub settings: crate::diagnostics::EditSettings,
    pub retopology_paths: Vec<SurfacePath>,
    pub panels: Option<PanelDraft>,
}
pub(super) struct PanelDraft {
    pub cut: bool,
    pub panels: Vec<crate::sculpt::GeneratedPanel>,
    pub outlines: Vec<Vec<crate::sculpt::Vec3>>,
    pub key: crate::ui::PanelKey,
}
pub(super) struct SurfacePreview {
    pub mesh: Mesh,
    pub report: SurfaceReport,
    pub retopology_paths: Vec<SurfacePath>,
    pub panels: Option<PanelDraft>,
}
pub(super) struct Worker {
    pub job: BackgroundJob<SurfacePreview>,
    pub revision: u64,
    pub settings: crate::diagnostics::EditSettings,
    pub progress: Option<Arc<std::sync::Mutex<String>>>,
    pub panel_key: Option<crate::ui::PanelKey>,
}
pub(super) struct PanelDisplay {
    pub key: crate::ui::PanelKey,
    pub revision: u64,
    pub mesh: Arc<Mesh>,
    pub display: Option<crate::viewport::JoinDisplay>,
    pub outlines: Vec<Vec<crate::sculpt::Vec3>>,
}
#[derive(Default)]
pub(super) struct ProjectionState {
    pub panel_display: Option<PanelDisplay>,
    pub panel_gate: Arc<std::sync::Mutex<()>>,
    pub worker: Option<Worker>,
    pub draft: Option<ProjectionDraft>,
    pub reference: Option<(u64, Arc<Mesh>)>,
    pub density_display: Option<(u64, Arc<Mesh>)>,
    pub last_retopology_settings: Option<serde_json::Value>,
}
impl ProjectionState {
    pub(super) fn has_worker(&self) -> bool {
        self.worker.is_some()
    }
}
impl App {
    pub(super) fn open_projection(&mut self) -> Result<(), String> {
        self.document.require_projection_target()?;
        if self.mesh_job.is_some() {
            return Err("Wait for the mesh operation first".into());
        }
        self.viewport.cancel(&mut self.document);
        self.ui.projection.open = true;
        self.ui.projection.mode = SurfaceMode::Projection;
        if self
            .ui
            .projection
            .source
            .is_none_or(|source| self.document.projection_source(source).is_err())
        {
            self.ui.projection.source = self
                .document
                .merge_down_target()
                .map(crate::document::ProjectionSource::Submodel);
        }
        self.ui.status = "Choose the reference, then preview the active target".into();
        Ok(())
    }
    pub(super) fn clear_projection_preview(&mut self) {
        self.projection.worker = None;
        self.projection.draft = None;
        self.projection.panel_display = None;
        self.viewport.session.projection_preview = None;
        if self.ui.projection.mode == SurfaceMode::Panels {
            self.viewport.session.join_rims.clear();
            self.viewport.set_join_preview(None);
        }
        self.ui.projection.report = None;
        self.ui.projection.busy = false;
        self.ui.projection.progress.clear();
        self.ui.projection.error = None;
    }
    pub(super) fn cancel_projection(&mut self) {
        self.clear_projection_preview();
        self.ui.projection.open = false;
        self.ui.projection.density_paint = None;
        self.projection.density_display = None;
        self.viewport.wait_for_release();
    }
    pub(super) fn start_projection(&mut self) -> Result<(), String> {
        if !self.ui.projection.open || self.mesh_job.is_some() {
            return Err("Open a surface preview when no mesh job is running".into());
        }
        if self.ui.projection.mode == SurfaceMode::Panels {
            return self.start_panels();
        }
        if self.ui.projection.mode == SurfaceMode::Retopology {
            return self.start_retopology();
        }
        if self.ui.projection.mode == SurfaceMode::Relaxation {
            return self.start_relaxation();
        }
        let source = self
            .ui
            .projection
            .source
            .ok_or("Choose a reference submodel or subdivision level")?;
        let reference = source;
        let source = self.document.projection_source(source)?;
        let options: ProjectionOptions = self.ui.projection.options;
        options.validate()?;
        self.clear_projection_preview();
        self.viewport.cancel(&mut self.document);
        let target = self.document.mesh().clone();
        let job = BackgroundJob::start("clay-detail-projection", move |cancel| {
            target
                .project_details(&source, options, &cancel)
                .map(|(mesh, report)| SurfacePreview {
                    mesh,
                    report: SurfaceReport::Projection(report),
                    retopology_paths: Vec::new(),
                    panels: None,
                })
        })?;
        let mut settings = self.document.capture_edit_settings();
        let view = settings
            .view_input
            .get_or_insert_with(|| serde_json::json!({}));
        view["detail_projection"] = serde_json::json!({
            "reference": match reference {
                crate::document::ProjectionSource::Submodel(i) => serde_json::json!({"submodel_index": i}),
                crate::document::ProjectionSource::Level(i) => serde_json::json!({"subdivision_level": i}),
            },
            "target_submodel_index": self.document.active_model_index(),
            "strength": options.strength,
            "max_distance": options.max_distance,
        });
        self.projection.worker = Some(Worker {
            job,
            progress: None,
            panel_key: None,
            revision: self.document.display_revision(),
            settings,
        });
        self.ui.projection.busy = true;
        self.ui.status = "Computing detail projection…".into();
        Ok(())
    }
    pub(super) fn poll_projection(&mut self) {
        if !self.panels_draft_current() {
            self.invalidate_panel_preview();
            self.ui.status = "Scene or panel settings changed; preview the panels again".into();
        }
        if let Some(progress) = self
            .projection
            .worker
            .as_ref()
            .and_then(|w| w.progress.as_ref())
            && let Ok(label) = progress.lock()
        {
            self.ui.projection.progress = label.clone();
        }
        let result = self.projection.worker.as_mut().and_then(|w| w.job.poll());
        if let Some(result) = result {
            let worker = self.projection.worker.take().unwrap();
            self.ui.projection.busy = false;
            if worker.revision != self.document.display_revision()
                || worker
                    .panel_key
                    .as_ref()
                    .is_some_and(|k| *k != self.ui.projection.panels.key(&self.document))
            {
                self.ui.status = "Scene changed; preview the surface edit again".into();
                return;
            }
            match result {
                Ok(SurfacePreview {
                    mesh,
                    report,
                    retopology_paths,
                    panels,
                }) => {
                    self.projection.panel_display = None;
                    self.ui.status = report.summary();
                    self.ui.projection.report = Some(report);
                    self.projection.draft = Some(ProjectionDraft {
                        display: panels.as_ref().map(|_| {
                            crate::viewport::JoinDisplay::with_active_mesh(&self.document, &mesh)
                        }),
                        mesh: Arc::new(mesh),
                        revision: worker.revision,
                        settings: worker.settings.clone(),
                        retopology_paths,
                        panels,
                    });
                }
                Err(error) => {
                    self.ui.projection.error = Some(error.clone());
                    self.ui.status = error;
                }
            }
        }
    }
    pub(super) fn apply_projection(&mut self) -> Result<(), String> {
        if !self.panels_draft_current() {
            self.invalidate_panel_preview();
            return Err(
                "The scene changed or panel settings changed; preview the panels again".into(),
            );
        }
        let draft = self
            .projection
            .draft
            .take()
            .ok_or("Preview the surface edit before applying it")?;
        if let Some(panels) = draft.panels {
            if panels.cut {
                let result = self.document.apply_panel_cut(
                    (*draft.mesh).clone(),
                    draft.revision,
                    Some(draft.settings),
                );
                self.cancel_projection();
                if result.is_ok() {
                    self.ui.status = "Panel outline cut into the active surface. Undo restores the original topology.".into();
                }
                return result;
            }
            if panels.panels.is_empty() {
                return Err("Preview panels before applying the shell".into());
            }
            let result =
                self.document
                    .apply_panels(panels.panels, draft.revision, Some(draft.settings));
            self.cancel_projection();
            if result.is_ok() {
                self.ui.status =
                    "Panels applied. The original source is hidden below them in Submodels.".into();
            }
            return result;
        }
        let path_count = draft.retopology_paths.len();
        let result = if self.ui.projection.mode == SurfaceMode::Retopology {
            self.document.apply_retopology(
                (*draft.mesh).clone(),
                draft.retopology_paths,
                draft.revision,
                Some(draft.settings),
            )
        } else {
            let apply = match self.ui.projection.mode {
                SurfaceMode::Relaxation => Document::apply_relaxation,
                _ => Document::apply_projection,
            };
            apply(
                &mut self.document,
                (*draft.mesh).clone(),
                draft.revision,
                Some(draft.settings),
            )
        };
        if result.is_ok()
            && self.ui.projection.mode == SurfaceMode::Relaxation
            && let Some((revision, _)) = &mut self.projection.reference
        {
            *revision = self.document.display_revision();
        }
        if result.is_ok() && self.ui.projection.mode == SurfaceMode::Retopology {
            self.ui.projection.source = Some(crate::document::ProjectionSource::Submodel(
                self.document.active_model_index() + 1,
            ));
            self.ui.status =
                "Layout applied. The hidden model below is the detail projection reference.".into();
            if path_count > 0 {
                self.ui.status.push_str(&format!(
                    " {path_count} surface paths reattached to the new model."
                ));
            }
            self.refresh_path_after_history();
        }
        self.cancel_projection();
        result
    }
    pub(super) fn open_relaxation(&mut self) -> Result<(), String> {
        self.open_projection()?;
        self.ui.projection.mode = SurfaceMode::Relaxation;
        self.ui.status = "Mask protected areas, then preview mesh relaxation".into();
        Ok(())
    }
    pub(super) fn open_retopology(&mut self) -> Result<(), String> {
        self.document.require_retopology_target()?;
        if self.viewport.paths.brush.is_some() {
            if self.viewport.paths.has_unfinished(&self.document) {
                return Err(
                    "Complete or cancel the current path before opening Guided retopology".into(),
                );
            }
            self.dispatch_path(crate::command::PathCommand::Close)?;
        }
        self.viewport.cancel(&mut self.document);
        self.clear_projection_preview();
        let state = &mut self.ui.projection;
        state.mode = SurfaceMode::Retopology;
        state.open = true;
        // Start large sculpts at a bounded, reviewable budget. The explicit
        // control still permits up to 50,000 quads.
        state.layout.method = crate::sculpt::retopology::Method::IntegerGrid;
        state.layout.symmetry_axis = None;
        state.layout.target = (self.document.mesh().triangles.len() / 10).clamp(4, 10_000);
        state.flow_paths.retain(|&id| {
            self.document.path(id).is_some_and(|p| {
                p.target == self.document.active_model_id()
                    && p.mode == crate::sculpt::paths::PathMode::Surface
            })
        });
        state.density_paint = None;
        if state.density_radius == 0. {
            state.density_radius = 50.;
            state.density_brush_strength = 0.5;
        }
        Ok(())
    }
    fn start_retopology(&mut self) -> Result<(), String> {
        let mut diagnostic_settings = self.retopology_diagnostic_settings();
        diagnostic_settings["source_surface_signature"] =
            crate::sculpt::paths::mesh_signature(self.document.mesh()).into();
        self.projection.last_retopology_settings = Some(diagnostic_settings.clone());
        self.document.require_retopology_target()?;
        self.clear_projection_preview();
        self.viewport.cancel(&mut self.document);
        let options = self.ui.projection.layout;
        options.require_method_settings()?;
        options.require_input_budget(self.document.mesh())?;
        self.ui.projection.density_paint = None;
        let paths = self
            .ui
            .projection
            .flow_paths
            .iter()
            .map(|&id| {
                self.document
                    .path(id)
                    .filter(|p| p.target == self.document.active_model_id())
                    .cloned()
                    .ok_or("Select a surface path on the active model")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mesh = self.document.mesh().clone();
        let mut retopology_paths: Vec<_> = self
            .document
            .paths()
            .iter()
            .filter(|p| p.target == self.document.active_model_id() && p.mode == PathMode::Surface)
            .cloned()
            .collect();
        let reproject_distance = self.paths.reproject_distance;
        let progress = Arc::new(std::sync::Mutex::new(String::from("Preparing surface…")));
        let worker_progress = progress.clone();
        let job = BackgroundJob::start("clay-guided-retopology", move |cancel| {
            let layout =
                mesh.guided_retopology_with_progress(options, &paths, cancel, &|stage| {
                    if let Ok(mut label) = worker_progress.lock() {
                        *label = crate::sculpt::retopology::progress_label(stage);
                    }
                })?;
            if let Ok(mut label) = worker_progress.lock() {
                *label = "Reprojecting saved paths…".into();
            }
            let mut guides = Vec::new();
            for path in &mut retopology_paths {
                path.reproject_cancelled(&layout.mesh, reproject_distance, &cancel)
                    .map_err(|e| {
                        format!("Path ‘{}’ could not attach to the layout: {e}", path.name)
                    })?;
                if paths.iter().any(|p| p.id == path.id) {
                    let length: f32 = path
                        .surface_curve()
                        .windows(2)
                        .map(|p| (p[1].position - p[0].position).len())
                        .sum();
                    let mut samples = path.samples_on_mesh_cancelled(
                        &layout.mesh,
                        (length / 512.).max(0.0001),
                        &cancel,
                    )?;
                    if path.closed {
                        samples.push(*samples.first().ok_or("Guide has no samples")?);
                    }
                    if options.mirror_path_x {
                        guides.push(
                            samples
                                .iter()
                                .map(|s| {
                                    let mirror = |v: crate::sculpt::Vec3| {
                                        crate::sculpt::Vec3(-v.0, v.1, v.2)
                                    };
                                    crate::sculpt::paths::PathSample {
                                        position: mirror(s.position),
                                        normal: mirror(s.normal),
                                        tangent: mirror(s.tangent),
                                        binormal: mirror(s.binormal) * -1.,
                                        ..*s
                                    }
                                })
                                .collect(),
                        );
                    }
                    guides.push(samples);
                }
            }
            let report = SurfaceReport::Retopology {
                quads: layout.mesh.quads.len(),
                polygons: layout.polygons,
                changed: layout.mesh != mesh,
                guides: guides.into(),
            };
            Ok(SurfacePreview {
                mesh: layout.mesh,
                report,
                retopology_paths,
                panels: None,
            })
        })?;
        let mut settings = self.document.capture_edit_settings();
        let view = settings
            .view_input
            .get_or_insert_with(|| serde_json::json!({}));
        view["guided_retopology"] = diagnostic_settings;
        self.projection.worker = Some(Worker {
            job,
            progress: Some(progress),
            panel_key: None,
            revision: self.document.display_revision(),
            settings,
        });
        self.ui.projection.busy = true;
        self.ui.status = "Computing retopology preview…".into();
        Ok(())
    }
    fn start_relaxation(&mut self) -> Result<(), String> {
        self.document.require_projection_target()?;
        let options = self.ui.projection.relaxation_options;
        options.validate()?;
        self.clear_projection_preview();
        self.viewport.cancel(&mut self.document);
        let revision = self.document.display_revision();
        if self
            .projection
            .reference
            .as_ref()
            .is_none_or(|(r, _)| *r != revision)
        {
            self.projection.reference = Some((revision, Arc::new(self.document.mesh().clone())));
        }
        let reference = self.projection.reference.as_ref().unwrap().1.clone();
        let target = self.document.mesh().clone();
        let job = BackgroundJob::start("clay-mesh-relaxation", move |cancel| {
            target
                .relax_mesh(&reference, options, &cancel)
                .map(|(mesh, report)| SurfacePreview {
                    mesh,
                    report: SurfaceReport::Relaxation(report),
                    retopology_paths: Vec::new(),
                    panels: None,
                })
        })?;
        let mut settings = self.document.capture_edit_settings();
        let view = settings
            .view_input
            .get_or_insert_with(|| serde_json::json!({}));
        view["relaxation"] = serde_json::json!({"strength":options.strength,"iterations":options.iterations,"max_deviation":options.max_deviation});
        self.projection.worker = Some(Worker {
            job,
            progress: None,
            panel_key: None,
            revision,
            settings,
        });
        self.ui.projection.busy = true;
        self.ui.status = "Computing mesh relaxation…".into();
        Ok(())
    }
}
