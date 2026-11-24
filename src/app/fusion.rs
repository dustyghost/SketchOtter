use super::*;
use crate::sculpt::Vec3;
use crate::{
    background_job::BackgroundJob,
    sculpt::{
        Mesh,
        fusion::{self, Boundary},
    },
};
use std::{collections::BTreeSet, sync::Arc};

enum ResultMesh {
    Openings(Mesh, [Vec<Boundary>; 2]),
    Preview(PreparedPreview),
}
struct PreparedPreview {
    mesh: Mesh,
    report: fusion::FusionReport,
    patch: Vec<[Vec3; 2]>,
    removed: Vec<[Vec3; 2]>,
}

impl PreparedPreview {
    fn build(
        source: &Mesh,
        mesh: Mesh,
        report: fusion::FusionReport,
        cancel: &dyn Fn() -> bool,
    ) -> Result<ResultMesh, String> {
        fn edges<'a>(
            vertices: &[Vec3],
            triangles: impl Iterator<Item = &'a [usize; 3]>,
            cancel: &dyn Fn() -> bool,
        ) -> Result<Vec<[Vec3; 2]>, String> {
            let mut unique = BTreeSet::new();
            for (i, &[a, b, c]) in triangles.enumerate() {
                if i % 256 == 0 {
                    fusion::cancelled(cancel)?;
                }
                for [v, w] in [[a, b], [b, c], [c, a]] {
                    unique.insert(crate::sculpt::uv::edge(v, w));
                }
            }
            Ok(unique.into_iter().map(|e| e.map(|v| vertices[v])).collect())
        }
        let (retained, patch) = mesh.triangles.split_at(report.retained_triangles);
        let patch = edges(&mesh.vertices, patch.iter(), cancel)?;
        // Cutting and culling retain source triangle order and vertex IDs until compaction.
        let mut keep = retained.iter().peekable();
        let mut removed_faces = Vec::new();
        for (i, triangle) in source.triangles.iter().enumerate() {
            if i % 256 == 0 {
                fusion::cancelled(cancel)?;
            }
            if keep.next_if_eq(&triangle).is_none() {
                removed_faces.push(triangle);
            }
        }
        if keep.next().is_some() {
            return Err("Could not match preview faces to the source meshes".into());
        }
        let removed = edges(&source.vertices, removed_faces.into_iter(), cancel)?;
        fusion::cancelled(cancel)?;
        Ok(ResultMesh::Preview(Self {
            mesh: fusion::compact(mesh),
            report,
            patch,
            removed,
        }))
    }
}

struct Worker {
    job: BackgroundJob<ResultMesh>,
    revision: u64,
    sources: [usize; 2],
    key: Option<crate::ui::FusionKey>,
}
struct Draft {
    mesh: Mesh,
    revision: u64,
    key: crate::ui::FusionKey,
}
#[derive(Default)]
pub(super) struct FusionState {
    pub display: Option<crate::viewport::JoinDisplay>,
    worker: Option<Worker>,
    prepared: Option<(Arc<Mesh>, u64, [usize; 2])>,
    draft: Option<Draft>,
}
impl FusionState {
    pub(super) fn has_worker(&self) -> bool {
        self.worker.is_some()
    }
}
#[cfg(test)]
impl FusionState {
    pub(super) fn preview_mesh(&self) -> &Mesh {
        &self.draft.as_ref().expect("completed preview").mesh
    }
}
impl App {
    pub(super) fn open_fusion(&mut self) -> Result<(), String> {
        if self.ui.boolean_enabled {
            self.toggle_boolean_preview();
        }
        self.viewport.cancel(&mut self.document);
        let first = self.document.active_model_index();
        let second = self.document.merge_down_target().unwrap_or(first);
        self.ui.fusion.open = true;
        self.ui.fusion.sources = [first, second];
        self.discover_fusion()
    }
    pub(super) fn clear_fusion(&mut self) {
        self.fusion.worker = None;
        self.fusion.draft = None;
        self.fusion.display = None;
        self.viewport.session.join_patch.clear();
        self.viewport.session.join_removed.clear();
        self.viewport.set_join_preview(None);
        self.ui.fusion.ready = false;
        self.ui.fusion.notice_is_error = false;
        self.ui.fusion.busy = false;
        self.ui.fusion.notice = if self.ui.fusion.intersections {
            "Preview the local intersection patches."
        } else {
            "Choose both openings, then preview the join."
        }
        .into();
    }
    pub(super) fn cancel_fusion(&mut self) {
        self.clear_fusion();
        self.fusion.prepared = None;
        self.ui.fusion.open = false;
        self.ui.fusion.loops = Default::default();
        self.viewport.session.join_rims.clear();
        self.viewport.session.join_pick = None;
        self.viewport.wait_for_release();
    }
    pub(super) fn discover_fusion(&mut self) -> Result<(), String> {
        self.clear_fusion();
        self.fusion.prepared = None;
        self.ui.fusion.loops = Default::default();
        self.ui.fusion.selected = [None, None];
        let sources = self.ui.fusion.sources;
        let [a, b] = self.document.fusion_sources(sources)?;
        let job = BackgroundJob::start("clay-join-openings", move |cancel| {
            fusion::cancelled(cancel)?;
            let first = fusion::boundaries_cancellable(&a, cancel)?;
            let mut second = if sources[0] == sources[1] {
                first.clone()
            } else {
                fusion::boundaries_cancellable(&b, cancel)?
            };
            let mesh = if sources[0] == sources[1] {
                a
            } else {
                for boundary in &mut second {
                    for v in &mut boundary.vertices {
                        *v += a.vertices.len();
                    }
                }
                fusion::merge(&a, &b)?
            };
            Ok(ResultMesh::Openings(mesh, [first, second]))
        })?;
        self.fusion.worker = Some(Worker {
            job,
            revision: self.document.display_revision(),
            sources,
            key: None,
        });
        self.ui.fusion.busy = true;
        self.ui.fusion.notice = "Finding open boundary loops…".into();
        Ok(())
    }
    pub(super) fn preview_fusion(&mut self) -> Result<(), String> {
        if self.ui.fusion.busy {
            return Err("Wait for the current preview or cancel it first".into());
        }
        let key = self.ui.fusion.key();
        let (mesh, revision, sources) = self
            .fusion
            .prepared
            .clone()
            .ok_or("Choose two valid source models first")?;
        if revision != self.document.display_revision() || sources != self.ui.fusion.sources {
            return Err("The sources changed; choose them again".into());
        }
        let selected = self.ui.fusion.selected;
        let options = self.ui.fusion.options;
        let intersections = self.ui.fusion.intersections;
        let collar = self.ui.fusion.collar;
        let job = if intersections {
            if sources[0] == sources[1] {
                return Err("Choose two different models for intersection fusion".into());
            }
            let [a, b] = self.document.fusion_sources(sources)?;
            BackgroundJob::start("clay-local-fusion", move |cancel| {
                fusion::fuse(&a, &b, collar, options, cancel).and_then(|(joined, report)| {
                    PreparedPreview::build(&mesh, joined, report, cancel)
                })
            })?
        } else {
            let first = self.ui.fusion.loops[0]
                .get(selected[0].ok_or("Choose the first opening")?)
                .ok_or("Choose a current first opening")?
                .vertices
                .clone();
            let second = self.ui.fusion.loops[1]
                .get(selected[1].ok_or("Choose the second opening")?)
                .ok_or("Choose a current second opening")?
                .vertices
                .clone();
            BackgroundJob::start("clay-local-join", move |cancel| {
                fusion::join_open_ends(&mesh, &first, &second, options, cancel).and_then(
                    |(joined, report)| PreparedPreview::build(&mesh, joined, report, cancel),
                )
            })?
        };
        self.clear_fusion();
        self.fusion.worker = Some(Worker {
            job,
            revision,
            sources,
            key: Some(key),
        });
        self.ui.fusion.busy = true;
        self.ui.fusion.notice = "Building the local connector…".into();
        Ok(())
    }
    pub(super) fn poll_fusion(&mut self) {
        if self.fusion.draft.as_ref().is_some_and(|d| {
            d.revision != self.document.display_revision() || d.key != self.ui.fusion.key()
        }) {
            self.clear_fusion();
            self.ui.fusion.notice = "Scene or join settings changed; preview the join again".into();
        }
        if let Some(result) = self.fusion.worker.as_mut().and_then(|w| w.job.poll()) {
            let worker = self.fusion.worker.take().unwrap();
            self.ui.fusion.busy = false;
            let result = if worker.revision != self.document.display_revision()
                || worker.sources != self.ui.fusion.sources
                || worker.key.is_some_and(|k| k != self.ui.fusion.key())
            {
                Err("Scene or join settings changed; preview the join again".into())
            } else {
                result
            };
            match result {
                Ok(ResultMesh::Openings(mesh, loops)) => {
                    self.ui.fusion.notice = if self.ui.fusion.intersections {
                        "Sources ready. Preview the intersection.".into()
                    } else {
                        format!(
                            "{} and {} openings found. Select the intended pair.",
                            loops[0].len(),
                            loops[1].len()
                        )
                    };
                    self.ui.fusion.loops = loops;
                    self.fusion.prepared = Some((Arc::new(mesh), worker.revision, worker.sources));
                }
                Ok(ResultMesh::Preview(PreparedPreview {
                    mesh,
                    report,
                    patch,
                    removed,
                })) => {
                    let result = self
                        .document
                        .fusion_geometry(mesh.clone(), worker.sources)
                        .and_then(|g| {
                            let mut d = Document::default();
                            d.replace_geometry_named(g, "Join preview")?;
                            Ok(d)
                        });
                    match result {
                        Ok(document) => {
                            self.viewport.session.join_patch = patch;
                            self.viewport.session.join_removed = removed;
                            self.fusion.display =
                                Some(crate::viewport::JoinDisplay::new(&document));
                            self.viewport.set_join_preview(self.fusion.display.clone());
                            self.ui.fusion.notice = if self.ui.fusion.intersections {
                                format!(
                                    "Preview ready · {} collar rings · {} segments · {} patch triangles. Removed {} + {} source polygons across {} join(s), retained {}.",
                                    self.ui.fusion.collar,
                                    self.ui.fusion.options.segments,
                                    mesh.triangles.len() - report.retained_triangles,
                                    report.removed_faces[0],
                                    report.removed_faces[1],
                                    report.joins,
                                    report.retained_faces
                                )
                            } else {
                                format!(
                                    "Preview ready · {} segments · alignment offset {} · curvature {:.2} · {} patch triangles.",
                                    self.ui.fusion.options.segments,
                                    self.ui.fusion.options.twist,
                                    self.ui.fusion.options.curvature,
                                    mesh.triangles.len() - report.retained_triangles
                                )
                            };
                            self.ui.fusion.has_removed_faces =
                                !self.viewport.session.join_removed.is_empty();
                            if report.enclosed_faces > 0 {
                                self.ui.fusion.notice.push_str(&format!(
                                    " Culled {} enclosed polygons.",
                                    report.enclosed_faces
                                ));
                            }
                            self.ui.fusion.ready = true;
                            self.fusion.draft = Some(Draft {
                                mesh,
                                revision: worker.revision,
                                key: worker.key.expect("preview job has settings"),
                            });
                        }
                        Err(e) => self.ui.fusion.show_error(e),
                    }
                }
                Err(e) => self.ui.fusion.show_error(e),
            }
        }
        if self.ui.fusion.open
            && let Some((side, index)) = self.viewport.session.join_pick.take()
        {
            self.ui.fusion.selected[side] = Some(index);
            self.clear_fusion();
        }
        self.viewport.session.join_rims.clear();
        if self.ui.fusion.open
            && !self.ui.fusion.intersections
            && (!self.ui.fusion.ready || self.ui.fusion.show_patch)
            && let Some((mesh, _, _)) = &self.fusion.prepared
        {
            for side in 0..2 {
                for (index, b) in self.ui.fusion.loops[side].iter().enumerate() {
                    self.viewport
                        .session
                        .join_rims
                        .push(crate::viewport::JoinRim {
                            side,
                            index,
                            selected: self.ui.fusion.selected[side] == Some(index),
                            pickable: side == self.ui.fusion.picking && !self.ui.fusion.busy,
                            points: b.vertices.iter().map(|&v| mesh.vertices[v]).collect(),
                        });
                }
            }
        }
    }
    pub(super) fn apply_fusion(&mut self) -> Result<(), String> {
        let draft = self
            .fusion
            .draft
            .take()
            .ok_or("Preview the join before applying it")?;
        if draft.key != self.ui.fusion.key() {
            self.clear_fusion();
            return Err("Join settings changed; preview the join again".into());
        }
        if let Err(error) =
            self.document
                .apply_fusion(draft.mesh, draft.key.sources, draft.revision)
        {
            self.clear_fusion();
            return Err(error);
        }
        self.cancel_fusion();
        self.ui.status = "Joined model created; original sources retained hidden.".into();
        Ok(())
    }
}
