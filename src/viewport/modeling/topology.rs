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
    revision: u64,
    edit: TopologyEdit,
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
        let symmetry = ModelingSymmetry::new(document.mesh(), document.symmetry())?;
        // Adding a cap does not remove a protected polygon, so check selected
        // boundary vertices explicitly as well as the common face-preservation guard.
        let touched: Vec<_> = edges
            .iter()
            .flatten()
            .copied()
            .chain(vertices.iter().copied())
            .flat_map(|v| symmetry.vertex_copies(v))
            .collect();
        document
            .mesh()
            .protect_model_vertices(&touched, options.protect_mask)?;
        let mesh = document
            .mesh()
            .edit_model_topology(edit, &faces, &edges, &vertices, &symmetry)?;
        document
            .mesh()
            .check_model_protection(&mesh, options.protect_mask)?;
        self.cancel();
        self.topology_preview = Some(TopologyPreview {
            mesh,
            selected: self.selected.clone(),
            symmetry: document.symmetry().clone(),
            tool: options.tool,
            mode: options.mode,
            protect: options.protect_mask,
            revision: document.display_revision(),
            edit,
        });
        Ok(())
    }
    pub fn apply_topology(&mut self, document: &mut Document, protect: bool) -> Result<(), String> {
        let p = self
            .topology_preview
            .as_ref()
            .ok_or("Preview Connect vertices, Create face or Fill boundary first")?;
        if p.revision != document.display_revision()
            || p.selected != self.selected
            || &p.symmetry != document.symmetry()
            || p.protect != protect
        {
            self.topology_preview = None;
            return Err(
                "The target or protection settings changed; preview the construction again".into(),
            );
        }
        document.commit_modeling(p.mesh.clone(), p.edit.label(), p.revision)?;
        self.topology_preview = None;
        self.sync(document);
        Ok(())
    }
}
