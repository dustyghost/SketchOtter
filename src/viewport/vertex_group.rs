//! Click the visible, active model's face group without invoking global inversion.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VertexGroupTool {
    Mask {
        erase: bool,
    },
    Visibility {
        hide: bool,
    },
    Components {
        mode: ComponentMode,
        change: SelectionChange,
        protect: bool,
    },
}

impl VertexGroupTool {
    pub(super) fn reversed(self, reverse: bool) -> Self {
        if !reverse {
            return self;
        }
        match self {
            Self::Mask { erase } => Self::Mask { erase: !erase },
            Self::Visibility { hide } => Self::Visibility { hide: !hide },
            Self::Components {
                mode,
                change,
                protect,
            } => Self::Components {
                mode,
                protect,
                change: if change == SelectionChange::Remove {
                    SelectionChange::Add
                } else {
                    SelectionChange::Remove
                },
            },
        }
    }
}

impl Viewport {
    pub(super) fn click_vertex_group(
        &mut self,
        document: &mut Document,
        tool: VertexGroupTool,
        group: Option<usize>,
    ) -> Result<(), String> {
        let Some(group) = group.filter(|&g| g != 0) else {
            return Ok(()); // Empty/unassigned/occluded/inactive hits never invert.
        };
        document.mesh().require_selection_group(group - 1)?;
        match tool {
            VertexGroupTool::Mask { erase } => {
                document.mask_polygroup(group, erase)?;
                self.modeling.accept_attribute_revision(document);
            }
            VertexGroupTool::Visibility { hide } => {
                document.hide_polygroup(group, hide)?;
                self.modeling.sync(document);
            }
            VertexGroupTool::Components {
                mode,
                change,
                protect,
            } => {
                document.require_modeling()?;
                self.modeling.sync(document);
                let mesh = document.mesh();
                let editable = mesh.sculptable_vertices();
                let allowed = |v: usize| editable[v] && (!protect || mesh.mask[v] == 0.);
                let mut hits = BTreeSet::new();
                let polygon_faces = crate::sculpt::uv::polygon_triangles(mesh);
                for (i, polygon) in mesh.polygons().iter().enumerate() {
                    // Polygon groups are resolved from the same triangle labels
                    // used for the face colors and the surface pick.
                    if !polygon.visible {
                        continue;
                    }
                    let belongs = polygon_faces[i]
                        .iter()
                        .any(|&f| mesh.polygroups.get(f).copied().unwrap_or(0) == group);
                    if !belongs {
                        continue;
                    }
                    match mode {
                        ComponentMode::Face => {
                            if polygon.vertices.iter().all(|&v| allowed(v)) {
                                hits.insert(Component::Face(i));
                            }
                        }
                        ComponentMode::Edge => hits.extend(
                            polygon
                                .edges()
                                .map(|(a, b)| [a.min(b), a.max(b)])
                                .filter(|e| e.iter().all(|&v| allowed(v)))
                                .map(Component::Edge),
                        ),
                        ComponentMode::Vertex => hits.extend(
                            polygon
                                .vertices
                                .iter()
                                .copied()
                                .filter(|&v| allowed(v))
                                .map(Component::Vertex),
                        ),
                    }
                }
                let selected = match change {
                    SelectionChange::Replace => hits,
                    SelectionChange::Add => self.modeling.selected.union(&hits).copied().collect(),
                    SelectionChange::Remove => {
                        self.modeling.selected.difference(&hits).copied().collect()
                    }
                };
                self.modeling.record_selection(document, selected);
            }
        }
        Ok(())
    }
}
