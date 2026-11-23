//! Logical adjacency and bounded, revision-local selection history.
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct SelectionHistory {
    symmetry: Option<Symmetry>,
    undo: Vec<(BTreeSet<Component>, BTreeSet<Component>)>,
    redo: Vec<(BTreeSet<Component>, BTreeSet<Component>)>,
}

fn copies(c: Component, symmetry: &ModelingSymmetry) -> Vec<Component> {
    match c {
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
    }
}

fn resized(
    mesh: &Mesh,
    selected: &BTreeSet<Component>,
    mode: ComponentMode,
    protect: bool,
    grow: bool,
    symmetry: &ModelingSymmetry,
) -> Result<BTreeSet<Component>, String> {
    let polygons = mesh.polygons();
    let mut incidence = BTreeMap::<[usize; 2], Vec<usize>>::new();
    let mut visible = BTreeSet::new();
    let mut blocked = BTreeSet::new();
    for (f, p) in polygons.iter().enumerate() {
        for &v in &p.vertices {
            if p.visible {
                visible.insert(v);
            } else {
                blocked.insert(v);
            }
            if protect && mesh.mask[v] > 0. {
                blocked.insert(v);
            }
        }
        for (a, b) in p.edges() {
            incidence.entry([a.min(b), a.max(b)]).or_default().push(f);
        }
    }
    for e in mesh.visible_loose_edges() {
        incidence.entry(e).or_default();
        for v in e {
            visible.insert(v);
            if protect && mesh.mask[v] > 0. {
                blocked.insert(v);
            }
        }
    }
    for &v in &mesh.loose_vertices {
        visible.insert(v);
        if protect && mesh.mask[v] > 0. {
            blocked.insert(v);
        }
    }
    let allowed_vertex = |v: usize| visible.contains(&v) && !blocked.contains(&v);
    let allowed = |c: Component| match c {
        Component::Vertex(v) => allowed_vertex(v),
        Component::Edge([a, b]) => allowed_vertex(a) && allowed_vertex(b),
        Component::Face(f) => {
            polygons[f].visible && polygons[f].vertices.iter().all(|&v| allowed_vertex(v))
        }
    };
    let mut adjacency = BTreeMap::<Component, BTreeSet<Component>>::new();
    let mut boundary = BTreeSet::new();
    let mut at_vertex = BTreeMap::<usize, Vec<[usize; 2]>>::new();
    for (&e, faces) in &incidence {
        match mode {
            ComponentMode::Face => {
                for &f in faces {
                    let c = Component::Face(f);
                    adjacency.entry(c).or_default().extend(
                        faces
                            .iter()
                            .filter(|&&n| n != f)
                            .map(|&n| Component::Face(n)),
                    );
                    if faces.len() != 2 {
                        boundary.insert(c);
                    }
                }
            }
            ComponentMode::Vertex => {
                adjacency
                    .entry(Component::Vertex(e[0]))
                    .or_default()
                    .insert(Component::Vertex(e[1]));
                adjacency
                    .entry(Component::Vertex(e[1]))
                    .or_default()
                    .insert(Component::Vertex(e[0]));
                if faces.len() != 2 {
                    boundary.extend(e.map(Component::Vertex));
                }
            }
            ComponentMode::Edge => {
                adjacency.entry(Component::Edge(e)).or_default();
                for v in e {
                    at_vertex.entry(v).or_default().push(e);
                }
                if faces.len() != 2 {
                    boundary.insert(Component::Edge(e));
                }
            }
        }
    }
    if mode == ComponentMode::Vertex {
        for &v in &mesh.loose_vertices {
            adjacency.entry(Component::Vertex(v)).or_default();
        }
    }
    if mode == ComponentMode::Edge {
        for edges in at_vertex.values() {
            for &e in edges {
                adjacency.get_mut(&Component::Edge(e)).unwrap().extend(
                    edges
                        .iter()
                        .filter(|&&n| n != e)
                        .copied()
                        .map(Component::Edge),
                );
            }
        }
    }
    if selected.is_empty() || selected.iter().any(|c| !adjacency.contains_key(c)) {
        return Err("Select components in the current mode first".into());
    }
    let source: BTreeSet<_> = selected.iter().flat_map(|&c| copies(c, symmetry)).collect();
    let mut result = if grow {
        source.clone()
    } else {
        BTreeSet::new()
    };
    for &c in &source {
        if !allowed(c) {
            continue;
        }
        if grow {
            for &n in &adjacency[&c] {
                let orbit = copies(n, symmetry);
                if orbit.iter().all(|&n| allowed(n)) {
                    result.extend(orbit);
                }
            }
        } else if !boundary.contains(&c)
            && adjacency[&c]
                .iter()
                .all(|n| source.contains(n) && allowed(*n))
        {
            result.insert(c);
        }
    }
    // Never leave half an exact symmetry orbit after erosion at a protected boundary.
    if !grow {
        let keep = result.clone();
        result.retain(|&c| copies(c, symmetry).iter().all(|n| keep.contains(n)));
    }
    Ok(result)
}

impl ModelingState {
    pub fn resize_selection(
        &mut self,
        document: &Document,
        mode: ComponentMode,
        protect: bool,
        grow: bool,
    ) -> Result<(), String> {
        self.sync(document);
        document.require_modeling()?;
        let symmetry = ModelingSymmetry::new(document.mesh(), document.symmetry())?;
        let result = resized(
            document.mesh(),
            &self.selected,
            mode,
            protect,
            grow,
            &symmetry,
        )?;
        self.record_selection(document, result);
        Ok(())
    }
    pub(in crate::viewport) fn record_selection(
        &mut self,
        document: &Document,
        result: BTreeSet<Component>,
    ) {
        if result == self.selected {
            self.notice = Some("Selection unchanged at its boundary");
            return;
        }
        if !self.can_restore_selection(document, false) {
            self.selection_history.undo.clear();
        }
        self.selection_history.redo.clear();
        self.selection_history.symmetry = Some(document.symmetry().clone());
        self.selection_history
            .undo
            .push((self.selected.clone(), result.clone()));
        while self.selection_history.undo.len() > 1
            && (self.selection_history.undo.len() > 32
                || self
                    .selection_history
                    .undo
                    .iter()
                    .map(|(a, b)| a.len() + b.len())
                    .sum::<usize>()
                    > 1_000_000)
        {
            self.selection_history.undo.remove(0);
        }
        self.selected = result;
    }
    pub fn clear_selection_history(&mut self) {
        self.selection_history = Default::default();
    }
    pub fn can_restore_selection(&self, document: &Document, redo: bool) -> bool {
        if self.revision != document.display_revision()
            || self.selection_history.symmetry.as_ref() != Some(document.symmetry())
        {
            return false;
        }
        if redo {
            self.selection_history
                .redo
                .last()
                .is_some_and(|(before, _)| *before == self.selected)
        } else {
            self.selection_history
                .undo
                .last()
                .is_some_and(|(_, after)| *after == self.selected)
        }
    }
    pub fn restore_selection(&mut self, document: &Document, redo: bool) -> Result<(), String> {
        self.sync(document);
        if !self.can_restore_selection(document, redo) {
            return Err("No selection change to restore on this mesh".into());
        }
        if redo {
            let entry = self.selection_history.redo.pop().unwrap();
            self.selected = entry.1.clone();
            self.selection_history.undo.push(entry);
        } else {
            let entry = self.selection_history.undo.pop().unwrap();
            self.selected = entry.0.clone();
            self.selection_history.redo.push(entry);
        }
        Ok(())
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    fn grid(n: usize) -> Mesh {
        let vertices = (0..=n)
            .flat_map(|y| {
                (0..=n).map(move |x| Vec3(x as f32 - n as f32 / 2., y as f32 - n as f32 / 2., 0.))
            })
            .collect::<Vec<_>>();
        let quads = (0..n)
            .flat_map(|y| {
                (0..n).map(move |x| {
                    let a = y * (n + 1) + x;
                    [a, a + 1, a + n + 2, a + n + 1]
                })
            })
            .collect::<Vec<_>>();
        Mesh {
            mask: vec![0.; vertices.len()],
            triangles: quads
                .iter()
                .flat_map(|&[a, b, c, d]| [[a, b, c], [a, c, d]])
                .collect(),
            vertices,
            quads,
            ..Default::default()
        }
    }
    #[test]
    fn precision_selection_logical_adjacency_and_open_boundary() {
        let mesh = grid(3);
        let sym = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let source = BTreeSet::from([Component::Face(4)]);
        let grown = resized(&mesh, &source, ComponentMode::Face, false, true, &sym).unwrap();
        assert_eq!(grown, [1, 3, 4, 5, 7].map(Component::Face).into());
        assert_eq!(
            resized(&mesh, &grown, ComponentMode::Face, false, false, &sym).unwrap(),
            source
        );
        let all = (0..9).map(Component::Face).collect();
        assert_eq!(
            resized(&mesh, &all, ComponentMode::Face, false, false, &sym).unwrap(),
            source
        );
        let v = BTreeSet::from([Component::Vertex(5)]);
        assert_eq!(
            resized(&mesh, &v, ComponentMode::Vertex, false, true, &sym).unwrap(),
            [1, 4, 5, 6, 9].map(Component::Vertex).into()
        );
        let edge = BTreeSet::from([Component::Edge([5, 6])]);
        let grown = resized(&mesh, &edge, ComponentMode::Edge, false, true, &sym).unwrap();
        assert_eq!(grown.len(), 7);
        assert!(
            !grown.contains(&Component::Edge([5, 10])),
            "No quad diagonal"
        );
        assert_eq!(
            resized(&mesh, &grown, ComponentMode::Edge, false, false, &sym).unwrap(),
            edge
        );
    }
    #[test]
    fn precision_selection_protection_islands_and_symmetry() {
        let mut mesh = grid(4);
        mesh.mask[2] = 1.;
        let sym = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let source = BTreeSet::from([Component::Vertex(7)]);
        let grown = resized(&mesh, &source, ComponentMode::Vertex, true, true, &sym).unwrap();
        assert!(!grown.contains(&Component::Vertex(2)));
        assert!(
            resized(&mesh, &source, ComponentMode::Vertex, false, true, &sym)
                .unwrap()
                .contains(&Component::Vertex(2))
        );
        mesh.hidden_faces = vec![false; mesh.triangles.len()];
        mesh.hidden_faces[0] = true;
        mesh.hidden_faces[1] = true;
        let grown = resized(&mesh, &source, ComponentMode::Vertex, false, true, &sym).unwrap();
        assert!(
            !grown.contains(&Component::Vertex(6)),
            "hidden adjoining face blocks vertex"
        );
        mesh.hidden_faces.clear();
        let symmetry = Symmetry {
            enabled: true,
            ..Default::default()
        };
        let sym = ModelingSymmetry::new(&mesh, &symmetry).unwrap();
        let source = BTreeSet::from([Component::Vertex(17)]);
        let grown = resized(&mesh, &source, ComponentMode::Vertex, true, true, &sym).unwrap();
        for c in &grown {
            assert!(copies(*c, &sym).iter().all(|n| grown.contains(n)));
        }
        let mesh = Mesh {
            vertices: vec![
                Vec3(0., 0., 0.),
                Vec3(1., 0., 0.),
                Vec3(0., 1., 0.),
                Vec3(3., 0., 0.),
                Vec3(4., 0., 0.),
                Vec3(3., 1., 0.),
            ],
            triangles: vec![[0, 1, 2], [3, 4, 5]],
            mask: vec![0.; 6],
            ..Default::default()
        };
        let sym = ModelingSymmetry::new(&mesh, &Symmetry::default()).unwrap();
        let selection = BTreeSet::from([Component::Face(0)]);
        assert_eq!(
            resized(&mesh, &selection, ComponentMode::Face, false, true, &sym).unwrap(),
            selection
        );
        assert!(
            resized(&mesh, &selection, ComponentMode::Face, false, false, &sym)
                .unwrap()
                .is_empty()
        );
    }
}
