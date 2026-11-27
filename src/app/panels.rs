//! Panel selection, private worker output and exact preview-to-Apply handoff.
use super::*;
use crate::{
    background_job::BackgroundJob,
    sculpt::PanelPreview,
    ui::{SurfaceMode, SurfaceReport},
};
impl App {
    pub(super) fn open_panels(&mut self) -> Result<(), String> {
        self.document.require_panel_target()?;
        if self.mesh_job.is_some() {
            return Err("Wait for the mesh operation first".into());
        }
        if self.ui.boolean_enabled {
            self.toggle_boolean_preview();
        }
        self.viewport.cancel(&mut self.document);
        self.clear_projection_preview();
        if self.ui.projection.panels.options == Default::default() {
            self.ui.projection.panels.options =
                self.document.mesh().fit_panel_border(Default::default());
        }
        self.ui.projection.panels.use_mask = self.document.mesh().mask.iter().any(|&m| m > 0.);
        let polygons = self.document.mesh().polygons();
        let selected: std::collections::BTreeSet<_> = self
            .viewport
            .modeling
            .selected_faces()
            .iter()
            .filter_map(|&i| polygons.get(i).map(|p| p.group))
            .filter(|&g| g > 0)
            .collect();
        self.ui.projection.panels.groups = selected.into_iter().collect();
        if self.ui.projection.panels.groups.is_empty() {
            if let Some((model, group)) = self.ui.selected_vertex_group
                && model == self.document.active_model_index()
                && self.document.mesh().polygroups.contains(&(group + 1))
            {
                self.ui.projection.panels.groups.push(group + 1);
            } else if let Some(group) = self
                .document
                .mesh()
                .polygroups
                .iter()
                .copied()
                .filter(|&g| g > 0)
                .min()
            {
                self.ui.projection.panels.groups.push(group);
            }
        }
        self.ui.projection.mode = SurfaceMode::Panels;
        self.ui.projection.open = true;
        self.ui.status =
            "Choose Extract panels or Panel cut, then preview the selected mask or groups.".into();
        self.start_panel_selection()
    }
    pub(super) fn start_panel_selection(&mut self) -> Result<(), String> {
        self.document.require_panel_target()?;
        let key = self.ui.projection.panels.key(&self.document);
        self.invalidate_panel_preview();
        if key.groups.is_empty() && key.mask_threshold.is_none() {
            return Ok(());
        }
        let mesh = self.document.mesh().clone();
        let worker_key = key.clone();
        let gate = self.projection.panel_gate.clone();
        let job = BackgroundJob::start("sketchotter-panel-selection", move |cancel| {
            panel_work(gate, cancel, || {
                let (mesh, groups, protect) = panel_input(mesh, &key, cancel)?;
                let crate::sculpt::PanelSelection {
                    display,
                    outlines,
                    count,
                } = mesh.preview_panel_selection(&groups, protect, &key.symmetry, &cancel)?;
                Ok(projection::SurfacePreview {
                    mesh: if key.cut { mesh } else { display },
                    report: SurfaceReport::PanelSelection { count },
                    retopology_paths: Vec::new(),
                    panels: Some(projection::PanelDraft {
                        cut: false,
                        panels: Vec::new(),
                        outlines,
                        key,
                    }),
                })
            })
        })?;
        self.projection.worker = Some(projection::Worker {
            job,
            revision: self.document.display_revision(),
            settings: self.capture_panel_edit_settings(),
            progress: None,
            panel_key: Some(worker_key),
        });
        self.ui.projection.busy = true;
        Ok(())
    }
    pub(super) fn start_panels(&mut self) -> Result<(), String> {
        self.document.require_panel_target()?;
        let key = self.ui.projection.panels.key(&self.document);
        if !key.cut {
            key.options.validate()?;
        }
        self.invalidate_panel_preview();
        self.viewport.cancel(&mut self.document);
        let mesh = self.document.mesh().clone();
        let slots = 64 - self.document.model_count();
        let worker_key = key.clone();
        let gate = self.projection.panel_gate.clone();
        let job = BackgroundJob::start("sketchotter-panel-preview", move |cancel| {
            panel_work(gate, cancel, || {
                let (mesh, groups, protect) = panel_input(mesh, &key, cancel)?;
                if key.cut {
                    let result = mesh.cut_panel_outlines(
                        &groups,
                        key.options.border,
                        protect,
                        &key.symmetry,
                        &cancel,
                    )?;
                    return Ok(projection::SurfacePreview {
                        report: SurfaceReport::PanelCut {
                            groups: result.groups,
                            vertices: result.added_vertices,
                        },
                        mesh: result.mesh,
                        retopology_paths: Vec::new(),
                        panels: Some(projection::PanelDraft {
                            cut: true,
                            panels: Vec::new(),
                            outlines: result.outlines,
                            key,
                        }),
                    });
                }
                let PanelPreview {
                    panels,
                    outlines,
                    display,
                } = mesh.generate_panels(&groups, key.options, protect, &key.symmetry, &cancel)?;
                if panels.len() > slots {
                    return Err(
                        "Panels plus the retained source exceed 64 submodels; select fewer groups"
                            .into(),
                    );
                }
                let report = SurfaceReport::Panels {
                    count: panels.len(),
                    triangles: display.triangles.len(),
                };
                Ok(projection::SurfacePreview {
                    mesh: display,
                    report,
                    retopology_paths: Vec::new(),
                    panels: Some(projection::PanelDraft {
                        cut: false,
                        panels,
                        outlines,
                        key,
                    }),
                })
            })
        })?;
        self.projection.worker = Some(projection::Worker {
            job,
            revision: self.document.display_revision(),
            settings: self.capture_panel_edit_settings(),
            progress: None,
            panel_key: Some(worker_key),
        });
        self.ui.projection.busy = true;
        self.ui.status = if self.ui.projection.panels.cut {
            "Cutting outline and support loops into the active surface…"
        } else {
            "Building panel borders, bevels and closed shells…"
        }
        .into();
        Ok(())
    }
    fn capture_panel_edit_settings(&self) -> crate::diagnostics::EditSettings {
        // Window controls may change after this frame's viewport diagnostic
        // capture. Record the exact settings handed to this preview worker.
        let mut settings = self.document.capture_edit_settings();
        let view = settings
            .view_input
            .get_or_insert_with(|| serde_json::json!({}));
        view["panel_generation"] = self.panel_diagnostic_settings();
        settings
    }
    pub(super) fn invalidate_panel_preview(&mut self) {
        let retained = self
            .projection
            .draft
            .take()
            .filter(|d| d.revision == self.document.display_revision())
            .and_then(|d| {
                d.panels.map(|p| projection::PanelDisplay {
                    key: p.key,
                    revision: d.revision,
                    mesh: d.mesh,
                    display: d.display,
                    outlines: p.outlines,
                })
            })
            .or_else(|| {
                self.projection
                    .panel_display
                    .take()
                    .filter(|d| d.revision == self.document.display_revision())
            });
        self.clear_projection_preview();
        self.projection.panel_display = retained;
    }
    pub(super) fn panels_draft_current(&self) -> bool {
        self.projection.draft.as_ref().is_none_or(|d| {
            d.panels.as_ref().is_none_or(|p| {
                d.revision == self.document.display_revision()
                    && p.key == self.ui.projection.panels.key(&self.document)
            })
        })
    }
    pub(super) fn sync_panel_outlines(&mut self) {
        if self.ui.projection.open && self.ui.projection.mode == SurfaceMode::Panels {
            self.viewport.session.join_rims = self
                .projection
                .draft
                .as_ref()
                .and_then(|d| d.panels.as_ref())
                .map(|p| &p.outlines)
                .or_else(|| {
                    self.projection
                        .panel_display
                        .as_ref()
                        .filter(|d| d.revision == self.document.display_revision())
                        .map(|d| &d.outlines)
                })
                .filter(|_| self.ui.projection.panels.outline)
                .map(|outlines| {
                    outlines
                        .iter()
                        .enumerate()
                        .map(|(i, points)| crate::viewport::JoinRim {
                            label: Some(format!("Source perimeter {}", i + 1)),
                            side: 0,
                            index: i,
                            selected: true,
                            pickable: false,
                            points: points.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
    }
}

// Cancellation never waits on the UI thread, but a retiring worker must finish
// before its replacement allocates reconstruction/validation buffers.
fn panel_work<T>(
    gate: std::sync::Arc<std::sync::Mutex<()>>,
    cancel: &dyn Fn() -> bool,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    loop {
        if cancel() {
            return Err("Panel preview cancelled".into());
        }
        match gate.try_lock() {
            Ok(_guard) => return run(),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(std::sync::TryLockError::Poisoned(error)) => {
                // The mutex owns no geometry; a failed worker leaves no shared mutable data.
                let _guard = error.into_inner();
                return run();
            }
        }
    }
}

// Mask-derived groups exist only on the private worker mesh until Apply.
fn panel_input(
    mut mesh: crate::sculpt::Mesh,
    key: &crate::ui::PanelKey,
    cancel: &dyn Fn() -> bool,
) -> Result<(crate::sculpt::Mesh, Vec<usize>, bool), String> {
    if cancel() {
        return Err("Panel preview cancelled".into());
    }
    if let Some(threshold) = key.mask_threshold {
        let name = mesh.unique_group_name(if key.cut { "Mask cut" } else { "Mask panel" });
        let group = mesh.create_vertex_group(name, threshold, false)? + 1;
        if !mesh.polygroups.contains(&group) {
            return Err("No complete faces meet the mask threshold; mask more or lower the selection threshold".into());
        }
        Ok((mesh, vec![group], false))
    } else {
        Ok((mesh, key.groups.clone(), key.protect))
    }
}
