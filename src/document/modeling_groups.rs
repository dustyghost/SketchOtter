//! Derive the surviving region of an edit without depending on vertex index stability.
use super::*;
use std::collections::BTreeSet;

pub(super) fn group_model_edit(before: &Mesh, after: &mut Mesh, label: &str) -> Result<(), String> {
    if before.vertices.len() == after.vertices.len()
        && before.triangles == after.triangles
        && before.quads == after.quads
        && before.loose_edges == after.loose_edges
        && before.loose_vertices == after.loose_vertices
    {
        let selected = before
            .vertices
            .iter()
            .zip(&after.vertices)
            .map(|(a, b)| a != b)
            .collect();
        return group_vertices(after, label, selected);
    }
    let point = |p: crate::sculpt::Vec3| [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()];
    let faces = |mesh: &Mesh| {
        mesh.polygons()
            .into_iter()
            .map(|p| {
                let points: Vec<_> = p
                    .vertices
                    .iter()
                    .map(|&v| point(mesh.vertices[v]))
                    .collect();
                (0..points.len())
                    .map(|start| {
                        points
                            .iter()
                            .cycle()
                            .skip(start)
                            .take(points.len())
                            .copied()
                            .collect::<Vec<_>>()
                    })
                    .min()
                    .unwrap()
            })
            .collect::<BTreeSet<_>>()
    };
    let edges = |mesh: &Mesh| {
        mesh.model_edges()
            .into_iter()
            .map(|e| {
                let mut e = e.map(|v| point(mesh.vertices[v]));
                e.sort_unstable();
                e
            })
            .collect::<BTreeSet<_>>()
    };
    let old_faces = faces(before);
    let new_faces = faces(after);
    let old_edges = edges(before);
    let new_edges = edges(after);
    let old_points: BTreeSet<_> = before.vertices.iter().copied().map(point).collect();
    let mut affected: BTreeSet<_> = old_faces
        .symmetric_difference(&new_faces)
        .flat_map(|f| f.iter().copied())
        .collect();
    affected.extend(
        old_edges
            .symmetric_difference(&new_edges)
            .flat_map(|e| e.iter().copied()),
    );
    affected.extend(
        after
            .vertices
            .iter()
            .copied()
            .map(point)
            .filter(|p| !old_points.contains(p)),
    );
    let selected: Vec<_> = after
        .vertices
        .iter()
        .map(|&p| affected.contains(&point(p)))
        .collect();
    group_vertices(after, label, selected)
}

fn group_vertices(after: &mut Mesh, label: &str, selected: Vec<bool>) -> Result<(), String> {
    if !selected.iter().any(|v| *v) {
        return Ok(());
    }
    let original_mask = std::mem::replace(
        &mut after.mask,
        selected.iter().map(|v| if *v { 1. } else { 0. }).collect(),
    );
    let name = after.unique_group_name(label);
    let result = after.create_vertex_group(name, 0.5, false);
    after.mask = original_mask;
    result.map(|_| ())
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    #[test]
    fn modeling_auto_groups_transform_only_the_moved_vertices() {
        let before = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Grid)
            .mesh()
            .unwrap();
        let mut after = before.clone();
        after.vertices[0].2 += 0.2;
        group_model_edit(&before, &mut after, "Move components").unwrap();
        assert_eq!(after.vertex_groups[0].weights[0], 1.);
        assert!(after.vertex_groups[0].weights[1..].iter().all(|&w| w == 0.));
    }
    #[test]
    fn modeling_auto_groups_isolate_new_points_preserve_masks_and_name_each_edit() {
        let mesh = Mesh {
            vertices: vec![crate::sculpt::Vec3(0., 0., 0.)],
            mask: vec![0.3],
            loose_vertices: vec![0],
            ..Default::default()
        };
        let mut out = mesh.clone();
        out.vertices.push(crate::sculpt::Vec3(1., 0., 0.));
        out.mask.push(0.4);
        out.loose_vertices.push(1);
        group_model_edit(&mesh, &mut out, "Duplicate vertices").unwrap();
        assert_eq!(out.mask, vec![0.3, 0.4]);
        assert_eq!(out.vertex_groups[0].weights, vec![0., 1.]);
        let before = out.clone();
        out.vertices[1].0 = 2.;
        group_model_edit(&before, &mut out, "Duplicate vertices").unwrap();
        assert_eq!(out.vertex_groups[1].name, "Duplicate vertices 2");
        assert_eq!(out.vertex_groups[1].weights, vec![0., 1.]);
    }
}
