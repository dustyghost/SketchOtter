//! Revision-bound manual topology previews. Geometry is committed only by Apply.
use super::*;
use crate::sculpt::TopologyEdit;

pub(super) struct TopologyPreview {
    pub mesh: Mesh,
    pub selected: BTreeSet<Component>,
    pub symmetry: Symmetry,
    pub tool: ModelTool,
    pub mode: ComponentMode,
    pub protect: bool,
    pub bridge_segments: u32,
    pub overlay: Option<Vec<[Vec3; 2]>>,
    revision: u64,
    edit: TopologyEdit,
}
impl TopologyPreview {
    pub fn is_bridge(&self) -> bool {
        self.edit == TopologyEdit::BridgeChains
    }
}
impl ModelingState {
    pub fn topology_preview_label(&self) -> Option<&'static str> {
        self.topology_preview.as_ref().map(|p| p.edit.label())
    }
    pub fn cancel_topology(&mut self) {
        self.topology_preview = None;
    }
    pub fn preview_topology(
        &mut self,
        document: &Document,
        edit: TopologyEdit,
        options: ModelingOptions,
    ) -> Result<(), String> {
        self.sync(document);
        self.cancel_topology();
        document.require_modeling()?;
        let faces = self.selected_faces();
        let edges: Vec<_> = self
            .selected
            .iter()
            .filter_map(|c| {
                if let Component::Edge(e) = c {
                    Some(*e)
                } else {
                    None
                }
            })
            .collect();
        let vertices: Vec<_> = self
            .selected
            .iter()
            .filter_map(|c| {
                if let Component::Vertex(v) = c {
                    Some(*v)
                } else {
                    None
                }
            })
            .collect();
        let polygons = document.mesh().polygons();
        if faces.iter().any(|&f| f >= polygons.len())
            || vertices
                .iter()
                .chain(edges.iter().flatten())
                .any(|&v| v >= document.mesh().vertices.len())
        {
            return Err("The component selection is stale; select it again".into());
        }
        let symmetry = ModelingSymmetry::new(document.mesh(), document.symmetry())?;
        // Adding a cap does not remove a protected polygon, so check selected
        // boundary vertices explicitly as well as the common face-preservation guard.
        let touched: Vec<_> = edges
            .iter()
            .flatten()
            .copied()
            .chain(vertices.iter().copied())
            .chain(
                faces
                    .iter()
                    .flat_map(|&f| polygons[f].vertices.iter().copied()),
            )
            .flat_map(|v| symmetry.vertex_copies(v))
            .collect();
        document
            .mesh()
            .protect_model_vertices(&touched, options.protect_mask)?;
        let mesh = if edit == TopologyEdit::BridgeChains {
            document
                .mesh()
                .bridge_model_chains(&edges, options.bridge_segments, &symmetry)?
        } else {
            document
                .mesh()
                .edit_model_topology(edit, &faces, &edges, &vertices, &symmetry)?
        };
        document
            .mesh()
            .check_model_protection(&mesh, options.protect_mask)?;
        let overlay = if matches!(
            edit,
            TopologyEdit::BridgeChains | TopologyEdit::FlipFaces | TopologyEdit::DissolveFaces
        ) {
            let key = |p: Vec3| [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()];
            let selected: BTreeSet<_> = touched
                .iter()
                .map(|&v| key(document.mesh().vertices[v]))
                .collect();
            let old_edges: BTreeSet<_> = document
                .mesh()
                .polygon_edges()
                .into_iter()
                .map(|e| {
                    let mut points = e.map(|v| key(document.mesh().vertices[v]));
                    points.sort_unstable();
                    points
                })
                .collect();
            Some(
                mesh.polygon_edges()
                    .into_iter()
                    .filter_map(|e| {
                        let points = e.map(|v| mesh.vertices[v]);
                        let mut keys = points.map(key);
                        keys.sort_unstable();
                        (!old_edges.contains(&keys) || keys.iter().all(|p| selected.contains(p)))
                            .then_some(points)
                    })
                    .collect(),
            )
        } else {
            None
        };
        self.cancel();
        self.topology_preview = Some(TopologyPreview {
            mesh,
            overlay,
            selected: self.selected.clone(),
            symmetry: document.symmetry().clone(),
            tool: options.tool,
            mode: options.mode,
            protect: options.protect_mask,
            bridge_segments: options.bridge_segments,
            revision: document.display_revision(),
            edit,
        });
        Ok(())
    }
    pub fn apply_topology(
        &mut self,
        document: &mut Document,
        options: ModelingOptions,
    ) -> Result<(), String> {
        let p = self
            .topology_preview
            .as_ref()
            .ok_or("Preview a topology operation first")?;
        if p.revision != document.display_revision()
            || p.selected != self.selected
            || &p.symmetry != document.symmetry()
            || p.protect != options.protect_mask
            || p.tool != options.tool
            || p.mode != options.mode
            || (p.is_bridge() && p.bridge_segments != options.bridge_segments)
        {
            self.topology_preview = None;
            return Err(
                "The target or protection settings changed; preview the topology operation again"
                    .into(),
            );
        }
        document.commit_modeling(p.mesh.clone(), p.edit.label(), p.revision)?;
        self.topology_preview = None;
        self.sync(document);
        Ok(())
    }
}
