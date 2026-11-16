//! Region tools share the existing latched rectangle/lasso/line gesture.
use super::{Component, ComponentMode, SelectionChange, Viewport, scene::Scene};
use crate::{
    document::Document,
    sculpt::{ModelingSymmetry, Vec3},
};
use eframe::egui::Pos2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RegionShape {
    #[default]
    Rectangle,
    Lasso,
    Line,
}
impl RegionShape {
    pub const ALL: [Self; 3] = [Self::Rectangle, Self::Lasso, Self::Line];
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Lasso => "Lasso",
            Self::Line => "Line",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegionOperation {
    Components {
        mode: ComponentMode,
        change: SelectionChange,
        protect: bool,
    },
    Mask {
        erase: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RegionTool {
    pub shape: RegionShape,
    pub through: bool,
    pub flip: bool,
    pub operation: RegionOperation,
}

pub(super) fn point_in_region(
    scene: &Scene,
    p: Vec3,
    through: bool,
    contains: &impl Fn(Pos2) -> bool,
) -> bool {
    scene.visible_depth(p)
        && contains(scene.project(p))
        && (through
            || scene
                .pick(scene.project(p))
                .is_some_and(|(hit, _)| (hit - p).len() <= 1e-5 * (1. + p.len())))
}

impl Viewport {
    pub(super) fn select_component_region(
        &mut self,
        document: &Document,
        scene: &Scene,
        tool: RegionTool,
        contains: impl Fn(Pos2) -> bool,
    ) -> Result<(), String> {
        let RegionOperation::Components {
            mode,
            change,
            protect,
        } = tool.operation
        else {
            unreachable!()
        };
        self.modeling.sync(document);
        document.require_modeling()?;
        let mesh = document.mesh();
        let polygons = mesh.polygons();
        let editable = mesh.sculptable_vertices();
        let allowed_vertex = |v: usize| editable[v] && (!protect || mesh.mask[v] == 0.);
        let mut candidates = BTreeSet::new();
        let wires: BTreeSet<_> = mesh.visible_loose_edges().into_iter().collect();
        let wire_vertices: BTreeSet<_> = wires
            .iter()
            .flatten()
            .copied()
            .chain(mesh.loose_vertices.iter().copied())
            .collect();
        match mode {
            ComponentMode::Edge => candidates.extend(wires.iter().copied().map(Component::Edge)),
            ComponentMode::Vertex => {
                candidates.extend(wire_vertices.iter().copied().map(Component::Vertex))
            }
            ComponentMode::Face => (),
        }
        for (f, poly) in polygons.iter().enumerate().filter(|(_, p)| p.visible) {
            match mode {
                ComponentMode::Face => {
                    candidates.insert(Component::Face(f));
                }
                ComponentMode::Edge => {
                    candidates.extend(
                        poly.edges()
                            .map(|(a, b)| Component::Edge([a.min(b), a.max(b)])),
                    );
                }
                ComponentMode::Vertex => {
                    candidates.extend(poly.vertices.iter().copied().map(Component::Vertex));
                }
            }
        }
        let vertices = |c: Component| match c {
            Component::Face(f) => polygons[f].vertices.clone(),
            Component::Edge(e) => e.to_vec(),
            Component::Vertex(v) => vec![v],
        };
        let allowed = |c| vertices(c).iter().all(|&v| allowed_vertex(v));
        let symmetry = ModelingSymmetry::new(mesh, document.symmetry())?;
        let mut hits = BTreeSet::new();
        for c in candidates.into_iter().filter(|&c| allowed(c)) {
            let vs = vertices(c);
            let center = vs
                .iter()
                .fold(Vec3::default(), |sum, &v| sum + mesh.vertices[v])
                * (1. / vs.len() as f32);
            // A bent quad's average centre can lie off both render triangles.
            // For faces, verify the surface under its projected centre belongs
            // to this polygon instead of comparing the ray hit's 3D position.
            let hit = if let Component::Face(f) = c {
                scene.visible_depth(center)
                    && contains(scene.project(center))
                    && (tool.through
                        || scene.pick_face(scene.project(center)).is_some_and(|t| {
                            mesh.triangles[t]
                                .iter()
                                .all(|v| polygons[f].vertices.contains(v))
                        }))
            } else if match c {
                Component::Vertex(v) => wire_vertices.contains(&v),
                Component::Edge(e) => wires.contains(&e),
                _ => false,
            } {
                scene.visible_depth(center)
                    && contains(scene.project(center))
                    && (tool.through || scene.point_unoccluded(center))
            } else {
                point_in_region(scene, center, tool.through, &contains)
            };
            if !hit {
                continue;
            }
            let copies: Vec<_> = match c {
                Component::Face(f) => symmetry
                    .face_copies(f)
                    .into_iter()
                    .map(Component::Face)
                    .collect(),
                Component::Edge(e) => symmetry
                    .edge_copies(e)
                    .into_iter()
                    .map(|[a, b]| Component::Edge([a.min(b), a.max(b)]))
                    .collect(),
                Component::Vertex(v) => symmetry
                    .vertex_copies(v)
                    .into_iter()
                    .map(Component::Vertex)
                    .collect(),
            };
            // Never partly select a symmetry group across a protected boundary.
            if copies.iter().all(|&c| allowed(c)) {
                hits.extend(copies);
            }
        }
        let result = match change {
            SelectionChange::Replace => hits,
            SelectionChange::Add => self.modeling.selected.union(&hits).copied().collect(),
            SelectionChange::Remove => self.modeling.selected.difference(&hits).copied().collect(),
        };
        self.modeling.record_selection(document, result);
        Ok(())
    }
}
