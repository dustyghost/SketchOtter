//! Explicit source/target snapping. The document remains the stable source until Apply.
use super::*;
use crate::sculpt::ComponentTransform;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapTarget {
    #[default]
    Vertex,
    Edge,
    Surface,
}
impl SnapTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::Edge => "Edge",
            Self::Surface => "Surface",
        }
    }
}
pub(super) fn snap_radius_default() -> f32 {
    12.
}
pub(super) struct SnapPreview {
    options: ModelingOptions,
    selection: BTreeSet<Component>,
    symmetry: Symmetry,
    revision: u64,
    vertices: Vec<usize>,
    excluded: BTreeSet<usize>,
    visible_vertices: Vec<usize>,
    edges: Vec<[usize; 2]>,
    excluded_faces: BTreeSet<[usize; 3]>,
    source: Option<usize>,
    target: Option<Vec3>,
    result: Option<Mesh>,
    amount: Option<f32>,
    error: Option<String>,
}

fn snap_transform(
    tool: ModelTool,
    source: Vec3,
    target: Vec3,
    pivot: Vec3,
    axis: Option<Vec3>,
) -> Result<(ComponentTransform, f32), String> {
    let a = source - pivot;
    let b = target - pivot;
    let axis = axis.map(Vec3::norm);
    match tool {
        ModelTool::Move => {
            let d = target - source;
            let d = axis.map_or(d, |axis| axis * d.dot(axis));
            Ok((ComponentTransform::Move(d), d.len()))
        }
        ModelTool::Rotate => {
            let (axis, angle) = if let Some(axis) = axis {
                let a = a - axis * a.dot(axis);
                let b = b - axis * b.dot(axis);
                if a.len() < 1e-6 || b.len() < 1e-6 {
                    return Err("Choose source and target away from the rotation axis".into());
                }
                (axis, axis.dot(a.cross(b)).atan2(a.dot(b)))
            } else {
                if a.len() < 1e-6 || b.len() < 1e-6 {
                    return Err("Choose source and target away from the selection center".into());
                }
                let cross = a.cross(b);
                if cross.len() < 1e-7 && a.dot(b) < 0. {
                    return Err("Opposite directions need an explicit rotation axis".into());
                }
                let axis = if cross.len() < 1e-7 {
                    Vec3(1., 0., 0.)
                } else {
                    cross.norm()
                };
                (axis, cross.len().atan2(a.dot(b)))
            };
            Ok((ComponentTransform::Rotate(axis, angle), angle.to_degrees()))
        }
        ModelTool::Scale => {
            let factor = if let Some(axis) = axis {
                let distance = a.dot(axis);
                if distance.abs() < 1e-6 {
                    return Err(
                        "Choose a source away from the scale plane through the selection center"
                            .into(),
                    );
                }
                b.dot(axis) / distance
            } else {
                if a.len() < 1e-6 {
                    return Err("Choose a source away from the selection center".into());
                }
                b.len() / a.len()
            };
            if !factor.is_finite() || !(0.001..=1000.).contains(&factor) {
                return Err("Snap scale must be between 0.001 and 1000; choose a target on the same side of the center".into());
            }
            Ok((
                ComponentTransform::Scale(axis.unwrap_or_default(), factor),
                factor,
            ))
        }
        _ => Err("Choose Move, Rotate or Scale before snapping".into()),
    }
}

impl ModelingState {
    pub fn snap_active(&self) -> bool {
        self.snap_preview.is_some()
    }
    pub fn snap_ready(&self) -> bool {
        self.snap_preview
            .as_ref()
            .is_some_and(|p| p.result.is_some())
    }
    pub fn snap_hint(&self) -> Option<String> {
        let p = self.snap_preview.as_ref()?;
        Some(if let Some(e) = &p.error {
            format!("{e} · pick another target or Cancel")
        } else if let Some(amount) = p.amount {
            let unit = match p.options.tool {
                ModelTool::Rotate => "°",
                ModelTool::Scale => "×",
                _ => " units",
            };
            format!(
                "{} {amount:.6}{unit} · Apply snap or Cancel",
                p.options.tool.label()
            )
        } else if p.source.is_some() {
            format!(
                "Pick a visible {} target on the active mesh",
                p.options.snap_target.label().to_lowercase()
            )
        } else {
            "Pick a visible vertex in the selection as the snap source".into()
        })
    }
    pub fn begin_snap(
        &mut self,
        document: &Document,
        options: ModelingOptions,
    ) -> Result<(), String> {
        self.sync(document);
        self.cancel();
        document.require_modeling()?;
        if document.mesh_settings().levels() != 1 {
            return Err("Use a single subdivision level before component snapping".into());
        }
        if !options.tool.is_transform() {
            return Err("Choose Move, Rotate or Scale before snapping".into());
        }
        if !options.snap_radius.is_finite() || !(4. ..=40.).contains(&options.snap_radius) {
            return Err("Snap radius must be 4–40 pixels".into());
        }
        let symmetry = ModelingSymmetry::new(document.mesh(), document.symmetry())?;
        let vertices = self.selected_vertices(document.mesh());
        let excluded: BTreeSet<_> = vertices
            .iter()
            .flat_map(|&v| symmetry.vertex_copies(v))
            .collect();
        document.mesh().protect_model_vertices(
            &excluded.iter().copied().collect::<Vec<_>>(),
            options.protect_mask,
        )?;
        let polygons = document.mesh().polygons();
        let wires = document.mesh().visible_loose_edges();
        let visible_vertices = polygons
            .iter()
            .filter(|p| p.visible)
            .flat_map(|p| p.vertices.iter().copied())
            .chain(wires.iter().flatten().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let edges = polygons
            .iter()
            .filter(|p| p.visible)
            .flat_map(|p| p.edges())
            .map(|(a, b)| [a.min(b), a.max(b)])
            .chain(wires)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut excluded_faces = BTreeSet::new();
        for polygon in &polygons {
            if !polygon.visible || polygon.vertices.iter().any(|v| excluded.contains(v)) {
                for a in 0..polygon.vertices.len() {
                    for b in a + 1..polygon.vertices.len() {
                        for c in b + 1..polygon.vertices.len() {
                            let mut face = [
                                polygon.vertices[a],
                                polygon.vertices[b],
                                polygon.vertices[c],
                            ];
                            face.sort_unstable();
                            excluded_faces.insert(face);
                        }
                    }
                }
            }
        }
        // Initialize the usual symmetry cache so the next frame does not cancel this fresh draft.
        self.symmetry = Some((document.symmetry().clone(), Ok(symmetry)));
        self.snap_preview = Some(SnapPreview {
            options,
            selection: self.selected.clone(),
            symmetry: document.symmetry().clone(),
            revision: document.display_revision(),
            vertices,
            excluded,
            visible_vertices,
            edges,
            excluded_faces,
            source: None,
            target: None,
            result: None,
            amount: None,
            error: None,
        });
        Ok(())
    }
    pub fn reset_snap_target(&mut self) {
        if let Some(p) = &mut self.snap_preview {
            p.target = None;
            p.result = None;
            p.amount = None;
            p.error = None;
        }
    }
    fn validate_snap(&self, document: &Document, options: ModelingOptions) -> Result<(), String> {
        let p = self
            .snap_preview
            .as_ref()
            .ok_or("Start Snap to target first")?;
        if p.revision != document.display_revision()
            || p.selection != self.selected
            || p.symmetry != *document.symmetry()
            || p.options != options
        {
            return Err("Snap settings or mesh changed; pick the source and target again".into());
        }
        document.require_modeling()?;
        if document.mesh_settings().levels() != 1 {
            return Err("Use a single subdivision level before component snapping".into());
        }
        Ok(())
    }
    pub fn apply_snap(
        &mut self,
        document: &mut Document,
        options: ModelingOptions,
    ) -> Result<(), String> {
        if let Err(e) = self.validate_snap(document, options) {
            self.snap_preview = None;
            return Err(e);
        }
        let p = self.snap_preview.as_mut().unwrap();
        let mesh = p
            .result
            .take()
            .ok_or("Choose a valid target before applying the snap")?;
        let source = p.source.unwrap();
        let anchor = mesh.vertices[source];
        let changed = mesh != *document.mesh();
        document.commit_modeling(mesh, "Snap components", p.revision)?;
        let selected = p.selection.clone();
        self.snap_preview = None;
        self.sync(document);
        self.selected = selected;
        self.selection_anchor = Some(anchor);
        self.notice = Some(if changed {
            "Snap applied · Undo restores the previous geometry"
        } else {
            "Snap made no change"
        });
        // Repeat must not silently replay an older, unrelated transform.
        self.last_edit = None;
        Ok(())
    }
    fn choose_snap_source(&mut self, v: usize) {
        if let Some(p) = &mut self.snap_preview {
            p.source = Some(v);
            p.error = None;
        }
    }
    fn choose_snap_target(&mut self, document: &Document, target: Vec3) -> Result<(), String> {
        let p = self.snap_preview.as_mut().ok_or("Start snapping first")?;
        p.result = None;
        p.amount = None;
        p.target = Some(target);
        p.error = None;
        let result: Result<(Mesh, f32), String> = (|| {
            if ![target.0, target.1, target.2]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err("Snap target is not finite".into());
            }
            let source = document.mesh().vertices[p.source.ok_or("Pick a source vertex first")?];
            let symmetry = ModelingSymmetry::new(document.mesh(), document.symmetry())?;
            let (_, pivot) =
                document
                    .mesh()
                    .component_transform_frame(&p.vertices, source, &symmetry);
            let axis = if p.options.interaction != ModelInteraction::Axis
                || p.options.axis == ModelAxis::Uniform
            {
                None
            } else if p.options.axis == ModelAxis::Normal {
                let mut normal = Vec3::default();
                let selected: BTreeSet<_> = p.vertices.iter().copied().collect();
                for face in document.mesh().polygons() {
                    if face.vertices.iter().all(|v| selected.contains(v)) {
                        let a = document.mesh().vertices[face.vertices[0]];
                        normal = normal
                            + (document.mesh().vertices[face.vertices[1]] - a)
                                .cross(document.mesh().vertices[face.vertices[2]] - a);
                    }
                }
                if normal.len() < 1e-6 {
                    return Err(
                        "Normal snapping needs a selected face region with a shared direction"
                            .into(),
                    );
                }
                Some(normal.norm())
            } else {
                Some(p.options.axis.vector())
            };
            let (transform, amount) = snap_transform(p.options.tool, source, target, pivot, axis)?;
            let mesh = document.mesh().transform_components(
                &p.vertices,
                source,
                transform,
                p.options.protect_mask,
                &symmetry,
            )?;
            mesh.validate()?;
            Ok((mesh, amount))
        })();
        match result {
            Ok((mesh, amount)) => {
                p.result = Some(mesh);
                p.amount = Some(amount);
                Ok(())
            }
            Err(e) => {
                p.error = Some(e.clone());
                Err(e)
            }
        }
    }
    pub(super) fn sync_snap(&mut self, document: &Document, options: ModelingOptions) {
        if self.snap_active() && self.validate_snap(document, options).is_err() {
            self.snap_preview = None;
            self.notice = Some("Snap cancelled because its target or settings changed");
        }
    }
}

fn visible_point(scene: &Scene, point: Vec3) -> bool {
    scene.point_unoccluded(point)
}

pub(super) fn nearest_edge_point(scene: &Scene, pointer: Pos2, a: Vec3, b: Vec3) -> Vec3 {
    let (origin, ray) = scene.ray(pointer);
    let d = b - a;
    let w = a - origin;
    let denom = d.dot(d) - d.dot(ray).powi(2);
    let t = if denom.abs() > 1e-12 {
        (d.dot(ray) * w.dot(ray) - d.dot(w)) / denom
    } else {
        0.
    };
    a + d * t.clamp(0., 1.)
}
impl SnapPreview {
    pub(super) fn mesh(&self) -> Option<&Mesh> {
        self.result.as_ref()
    }
    fn candidate(
        &self,
        document: &Document,
        scene: &Scene,
        pointer: Pos2,
    ) -> Option<(Option<usize>, Vec3)> {
        let mesh = document.mesh();
        if self.source.is_some() && self.options.snap_target == SnapTarget::Surface {
            let f = scene.pick_face(pointer)?;
            let mut face = mesh.triangles[f];
            face.sort_unstable();
            if self.excluded_faces.contains(&face) {
                return None;
            }
            return scene.pick(pointer).map(|(p, _)| (None, p));
        }
        let candidates: Box<dyn Iterator<Item = (Option<usize>, Vec3)> + '_> =
            if self.source.is_none() || self.options.snap_target == SnapTarget::Vertex {
                Box::new(
                    self.visible_vertices
                        .iter()
                        .filter(|v| {
                            if self.source.is_none() {
                                self.excluded.contains(v)
                            } else {
                                !self.excluded.contains(v)
                            }
                        })
                        .map(|&v| (Some(v), mesh.vertices[v])),
                )
            } else {
                Box::new(
                    self.edges
                        .iter()
                        .filter(|e| !e.iter().any(|v| self.excluded.contains(v)))
                        .map(|e| {
                            (
                                None,
                                nearest_edge_point(
                                    scene,
                                    pointer,
                                    mesh.vertices[e[0]],
                                    mesh.vertices[e[1]],
                                ),
                            )
                        }),
                )
            };
        candidates
            .filter_map(|(v, p)| {
                let distance = scene.project(p).distance(pointer);
                (distance <= self.options.snap_radius && visible_point(scene, p))
                    .then_some((distance, v, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, v, p)| (v, p))
    }
}
impl Viewport {
    pub(super) fn update_snap(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        painter: &egui::Painter,
        scene: &Scene,
        document: &Document,
        eligible: Option<Pos2>,
    ) -> bool {
        let Some(preview) = &self.modeling.snap_preview else {
            return false;
        };
        let color = Color32::from_rgb(255, 205, 95);
        let marker = |p: Vec3, label: &str| {
            if scene.visible_depth(p) {
                let pos = scene.project(p);
                painter.circle_stroke(pos, 7., (2., color));
                painter.text(
                    pos + Vec2::new(10., -10.),
                    egui::Align2::LEFT_BOTTOM,
                    label,
                    egui::FontId::proportional(13.),
                    color,
                );
            }
        };
        if let Some(source) = preview.source {
            let mesh = preview.result.as_ref().unwrap_or(document.mesh());
            let position = mesh.vertices[source];
            let coincident = preview
                .target
                .is_some_and(|target| scene.project(position).distance(scene.project(target)) < 3.);
            marker(
                position,
                if coincident {
                    "Source = target"
                } else {
                    "Source"
                },
            );
            if let Some(target) = preview.target {
                if !coincident {
                    marker(target, "Target");
                }
                if let Some(line) = scene.clip_segment([mesh.vertices[source], target]) {
                    painter.line_segment(line, (1.5, color));
                }
            }
        }
        Self::paint_components(
            preview.selection.iter().copied(),
            preview.result.as_ref().unwrap_or(document.mesh()),
            scene,
            painter,
            Color32::from_rgb(70, 225, 240),
        );
        let candidate = if preview.target.is_none() || preview.error.is_some() {
            eligible.and_then(|p| preview.candidate(document, scene, p))
        } else {
            None
        };
        if let Some((_, p)) = candidate {
            marker(
                p,
                if preview.source.is_none() {
                    "Pick source"
                } else {
                    "Pick target"
                },
            );
        }
        let hint = self.modeling.snap_hint().unwrap();
        painter.text(
            navigation::inner(response.rect).left_bottom() + Vec2::new(8., -44.),
            egui::Align2::LEFT_BOTTOM,
            hint,
            egui::FontId::proportional(13.),
            Color32::WHITE,
        );
        if self.session.gesture.is_some_and(|g| g != Gesture::Modeling) {
            return false;
        }
        // A click is intentional; a drag on empty space remains navigation. Coalesced pen taps work too.
        if response.clicked_by(egui::PointerButton::Primary)
            && !ui.input(|i| i.modifiers.alt || i.modifiers.ctrl || i.modifiers.command)
        {
            if let Some((v, target)) = candidate {
                if self
                    .modeling
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .source
                    .is_none()
                {
                    self.modeling.choose_snap_source(v.unwrap());
                } else if let Err(e) = self.modeling.choose_snap_target(document, target) {
                    self.status = Some(e);
                }
            }
            return true;
        }
        candidate.is_some() || eligible.is_some_and(|p| scene.pick_model(p).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn moved(t: ComponentTransform, p: Vec3, pivot: Vec3) -> Vec3 {
        match t {
            ComponentTransform::Move(d) => p + d,
            ComponentTransform::Rotate(a, r) => pivot + (p - pivot).rotated(a, r),
            ComponentTransform::Scale(a, f) => {
                if a.len() < 1e-6 {
                    pivot + (p - pivot) * f
                } else {
                    p + a * ((p - pivot).dot(a) * (f - 1.))
                }
            }
        }
    }
    #[test]
    fn precision_snap_move_rotate_scale_and_axis_constraints() {
        let pivot = Vec3(2., 3., 4.);
        let source = pivot + Vec3(1., 1., 0.);
        let target = pivot + Vec3(-2., 2., 2.);
        let (t, _) = snap_transform(ModelTool::Move, source, target, pivot, None).unwrap();
        assert!((moved(t, source, pivot) - target).len() < 1e-6);
        let (t, _) = snap_transform(
            ModelTool::Move,
            source,
            target,
            pivot,
            Some(Vec3(1., 0., 0.)),
        )
        .unwrap();
        assert_eq!(moved(t, source, pivot), Vec3(target.0, source.1, source.2));
        let (t, angle) = snap_transform(
            ModelTool::Rotate,
            source,
            target,
            pivot,
            Some(Vec3(0., 0., 1.)),
        )
        .unwrap();
        assert!((angle - 90.).abs() < 1e-4);
        assert!((moved(t, source, pivot) - (pivot + Vec3(-1., 1., 0.))).len() < 1e-5);
        let (t, _) = snap_transform(ModelTool::Rotate, source, target, pivot, None).unwrap();
        assert!(((moved(t, source, pivot) - pivot).norm() - (target - pivot).norm()).len() < 1e-5);
        let (t, f) = snap_transform(ModelTool::Scale, source, target, pivot, None).unwrap();
        assert!((f - 6_f32.sqrt()).abs() < 1e-5);
        assert!(((moved(t, source, pivot) - pivot).len() - (target - pivot).len()).abs() < 1e-5);
        let (t, f) = snap_transform(
            ModelTool::Scale,
            source,
            target,
            pivot,
            Some(Vec3(0., 1., 0.)),
        )
        .unwrap();
        assert_eq!(f, 2.);
        assert_eq!(moved(t, source, pivot), pivot + Vec3(1., 2., 0.));
        assert!(
            snap_transform(
                ModelTool::Scale,
                source,
                target,
                pivot,
                Some(Vec3(1., 0., 0.))
            )
            .is_err()
        );
        assert!(snap_transform(ModelTool::Rotate, pivot, target, pivot, None).is_err());
        assert!(
            snap_transform(
                ModelTool::Rotate,
                source,
                pivot - (source - pivot),
                pivot,
                None
            )
            .is_err()
        );
    }
    fn document() -> Document {
        let mut d = Document::default();
        let mesh = Mesh {
            vertices: vec![
                Vec3(-1., -0.4, 0.),
                Vec3(-0.4, -0.4, 0.),
                Vec3(-0.4, 0.4, 0.),
                Vec3(-1., 0.4, 0.),
                Vec3(0.4, -0.4, 0.),
                Vec3(1., -0.4, 0.),
                Vec3(1., 0.4, 0.),
                Vec3(0.4, 0.4, 0.),
            ],
            quads: vec![[0, 1, 2, 3], [4, 5, 6, 7]],
            triangles: vec![[0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7]],
            mask: vec![0.; 8],
            ..Default::default()
        };
        d.replace_geometry(crate::sculpt::Geometry::new(mesh))
            .unwrap();
        d
    }
    #[test]
    fn precision_snap_hidden_occluded_and_distant_targets() {
        let mut document = document();
        let mut mesh = document.mesh().clone();
        mesh.hidden_faces = vec![false, false, true, true];
        document
            .replace_geometry(crate::sculpt::Geometry::new(mesh))
            .unwrap();
        let ctx = egui::Context::default();
        let painter = ctx.layer_painter(egui::LayerId::background());
        let camera = crate::camera::Camera {
            yaw: 0.,
            pitch: 0.,
            ..Default::default()
        };
        let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 700.));
        let scene = Scene::paint(
            &painter,
            rect,
            &document,
            &camera,
            false,
            crate::viewport::WireMode::default(),
            false,
        );
        let mut state = ModelingState::default();
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        let mut ui = crate::ui::UiState::default();
        ui.modeling.tool = ModelTool::Move;
        state.begin_snap(&document, ui.modeling.options()).unwrap();
        state.choose_snap_source(1);
        for target in [SnapTarget::Vertex, SnapTarget::Edge, SnapTarget::Surface] {
            state.snap_preview.as_mut().unwrap().options.snap_target = target;
            assert!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(
                        &document,
                        &scene,
                        scene.project(document.mesh().vertices[4])
                    )
                    .is_none()
            );
        }
        let mut mesh = document.mesh().clone();
        mesh.hidden_faces.clear();
        document
            .replace_geometry(crate::sculpt::Geometry::new(mesh))
            .unwrap();
        let scene = Scene::paint(
            &painter,
            rect,
            &document,
            &camera,
            false,
            crate::viewport::WireMode::default(),
            false,
        );
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        state.begin_snap(&document, ui.modeling.options()).unwrap();
        state.choose_snap_source(1);
        assert!(
            state
                .snap_preview
                .as_ref()
                .unwrap()
                .candidate(
                    &document,
                    &scene,
                    scene.project(document.mesh().vertices[4]) + Vec2::new(-25., 0.)
                )
                .is_none()
        );
        assert!(
            !visible_point(&scene, document.mesh().vertices[5] + Vec3(0., 0., -0.2)),
            "front surface occludes the back target"
        );
    }

    #[test]
    fn precision_snap_normal_constraint_and_failed_target_clear_the_preview() {
        let document = document();
        let mut state = ModelingState::default();
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        let mut ui = crate::ui::UiState::default();
        ui.modeling.tool = ModelTool::Move;
        ui.modeling.interaction = ModelInteraction::Axis;
        ui.modeling.axis = ModelAxis::Normal;
        state.begin_snap(&document, ui.modeling.options()).unwrap();
        state.choose_snap_source(1);
        state
            .choose_snap_target(&document, Vec3(0.4, -0.4, 0.2))
            .unwrap();
        let mesh = state.preview().unwrap();
        for v in 0..4 {
            assert!(
                (mesh.vertices[v] - document.mesh().vertices[v] - Vec3(0., 0., 0.2)).len() < 1e-6
            );
        }
        assert_eq!(&mesh.vertices[4..], &document.mesh().vertices[4..]);
        assert!(
            state
                .choose_snap_target(&document, Vec3(f32::NAN, 0., 0.))
                .is_err()
        );
        assert!(!state.snap_ready());
        assert!(state.preview().is_none());
        state.selected = BTreeSet::from([Component::Vertex(1)]);
        state.begin_snap(&document, ui.modeling.options()).unwrap();
        state.choose_snap_source(1);
        assert!(
            state
                .choose_snap_target(&document, Vec3(0.4, -0.4, 0.2))
                .unwrap_err()
                .contains("Normal snapping needs")
        );
    }

    #[test]
    fn precision_snap_presets_default_and_validate_radius() {
        let mut ui = crate::ui::UiState::default();
        ui.modeling.tool = ModelTool::Move;
        ui.modeling.loop_count = 1;
        let options = ui.modeling.options();
        let mut encoded = serde_json::to_value(options).unwrap();
        encoded.as_object_mut().unwrap().remove("snap_target");
        encoded.as_object_mut().unwrap().remove("snap_radius");
        let restored: ModelingOptions = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored.snap_target, SnapTarget::Vertex);
        assert_eq!(restored.snap_radius, 12.);
        let mut invalid = restored;
        invalid.snap_radius = f32::NAN;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn precision_snap_transaction_stale_preview_and_protection() {
        let mut document = document();
        let before = document.geometry_snapshot();
        let history = document.history_counts();
        let mut state = ModelingState::default();
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        let mut ui = crate::ui::UiState::default();
        ui.modeling.tool = ModelTool::Move;
        let options = ui.modeling.options();
        state.begin_snap(&document, options).unwrap();
        state.choose_snap_source(0);
        state
            .choose_snap_target(&document, Vec3(0.4, -0.4, 0.))
            .unwrap();
        let expected = state.preview().unwrap().clone();
        assert_eq!(document.geometry_snapshot(), before);
        assert_eq!(document.history_counts(), history);
        assert_eq!(&expected.vertices[4..], &before.mesh.vertices[4..]);
        assert_eq!(expected.quads, before.mesh.quads);
        assert_eq!(expected.triangles, before.mesh.triangles);
        state.update_preview(&document, None, Some(options), false, false);
        assert!(state.snap_ready(), "modal suspension preserves draft");
        state.apply_snap(&mut document, options).unwrap();
        assert_eq!(document.mesh(), &expected);
        assert_eq!(document.history_counts().0, history.0 + 1);
        document.undo();
        assert_eq!(document.geometry_snapshot(), before);
        document.redo();
        assert_eq!(document.mesh(), &expected);
        document.undo();
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        state.begin_snap(&document, options).unwrap();
        state.choose_snap_source(0);
        state
            .choose_snap_target(&document, Vec3(0.4, -0.4, 0.))
            .unwrap();
        let mut changed = options;
        changed.axis = ModelAxis::Y;
        assert!(state.apply_snap(&mut document, changed).is_err());
        assert!(!state.snap_active());
        assert_eq!(document.geometry_snapshot(), before);
        let mut mesh = document.mesh().clone();
        mesh.mask[0] = 1.;
        document
            .replace_geometry(crate::sculpt::Geometry::new(mesh))
            .unwrap();
        state.sync(&document);
        state.selected.insert(Component::Face(0));
        changed.protect_mask = true;
        assert!(state.begin_snap(&document, changed).is_err());
        assert!(!state.snap_active());
    }
    #[test]
    fn precision_snap_picking_excludes_source_and_hidden_targets_in_both_projections() {
        let document = document();
        let ctx = egui::Context::default();
        let painter = ctx.layer_painter(egui::LayerId::background());
        for perspective in [false, true] {
            let camera = crate::camera::Camera {
                yaw: 0.,
                pitch: 0.,
                perspective,
                ..Default::default()
            };
            let rect = egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(900., 700.));
            let scene = Scene::paint(
                &painter,
                rect,
                &document,
                &camera,
                false,
                crate::viewport::WireMode::default(),
                false,
            );
            let mut state = ModelingState::default();
            state.sync(&document);
            state.selected.insert(Component::Face(0));
            let mut ui = crate::ui::UiState::default();
            ui.modeling.tool = ModelTool::Move;
            state.begin_snap(&document, ui.modeling.options()).unwrap();
            let source = scene.project(document.mesh().vertices[1]);
            assert_eq!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(&document, &scene, source)
                    .unwrap()
                    .0,
                Some(1)
            );
            state.choose_snap_source(1);
            assert!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(&document, &scene, source)
                    .is_none()
            );
            let target = scene.project(document.mesh().vertices[4]);
            assert_eq!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(&document, &scene, target)
                    .unwrap()
                    .0,
                Some(4)
            );
            state.snap_preview.as_mut().unwrap().options.snap_target = SnapTarget::Edge;
            let position = Vec3(0.7, -0.4, 0.);
            let p = scene.project(position);
            let (_, hit) = state
                .snap_preview
                .as_ref()
                .unwrap()
                .candidate(&document, &scene, p)
                .unwrap();
            assert!((hit - position).len() < 1e-5);
            state.snap_preview.as_mut().unwrap().options.snap_target = SnapTarget::Surface;
            let p = scene.project(Vec3(0.7, 0., 0.));
            assert!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(&document, &scene, p)
                    .is_some()
            );
            assert!(
                state
                    .snap_preview
                    .as_ref()
                    .unwrap()
                    .candidate(&document, &scene, scene.project(Vec3(-0.7, 0., 0.)))
                    .is_none()
            );
        }
    }
}
