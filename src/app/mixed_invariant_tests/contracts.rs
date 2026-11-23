//! Operation-specific expectations independent of production acceptance gates.
use super::*;

pub(super) fn invariant(mesh: &Mesh, closed: bool, all_quads: bool) -> Check {
    if let Some(error) = surface_error(mesh) {
        return Err(format!("surface: {error}"));
    }
    require!(!closed || closed_surface(mesh), "closed surface opened");
    require!(
        !all_quads || (!mesh.quads.is_empty() && mesh.quads.len() * 2 == mesh.triangles.len()),
        "lost real quad metadata"
    );
    require!(
        mesh.mask.len() == mesh.vertices.len()
            && mesh
                .mask
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
        "invalid mask"
    );
    require!(
        mesh.materials.len() == mesh.vertices.len(),
        "lost material samples"
    );
    require!(mesh.vertex_groups.len() == 3, "lost group channels");
    for g in &mesh.vertex_groups {
        require!(
            g.weights.len() == mesh.vertices.len()
                && g.weights
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid group samples"
        );
    }
    require!(
        mesh.polygroups.len() == mesh.triangles.len()
            && mesh.polygroups.iter().all(|&g| (1..=3).contains(&g)),
        "invalid face categories"
    );
    for m in &mesh.materials {
        require!(
            m.metallic.is_finite() && (0. ..=1.).contains(&m.metallic),
            "invalid material"
        );
        require!(
            (m.base_color[0] - m.metallic).abs() < 2e-5
                && (m.roughness - (0.2 + 0.5 * m.metallic)).abs() < 2e-5,
            "material channels remapped independently"
        );
    }
    let range = |v: &[f32]| {
        v.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - v.iter().copied().fold(f32::INFINITY, f32::min)
    };
    require!(
        range(&mesh.vertex_groups[0].weights) > 0.1,
        "group variation lost"
    );
    require!(
        range(
            &mesh
                .materials
                .iter()
                .map(|m| m.metallic)
                .collect::<Vec<_>>()
        ) > 0.1,
        "material variation lost"
    );
    Ok(())
}
fn signature(mesh: &Mesh, v: usize) -> Vec<u32> {
    std::iter::once(mesh.mask[v])
        .chain(mesh.vertex_groups.iter().map(|g| g.weights[v]))
        .chain([mesh.materials[v].metallic, mesh.materials[v].roughness])
        .map(f32::to_bits)
        .collect()
}
pub(super) fn protected(before: &Mesh, after: &Mesh) -> Check {
    let mut map = BTreeMap::<Vec<u32>, Vec<usize>>::new();
    for v in 0..after.vertices.len() {
        map.entry(signature(after, v)).or_default().push(v);
    }
    let mut remap = BTreeMap::new();
    let mut protect = |v: usize| -> Result<usize, String> {
        if let Some(&w) = remap.get(&v) {
            return Ok(w);
        }
        let candidates = map
            .get(&signature(before, v))
            .ok_or("protected attributes lost")?;
        require!(candidates.len() == 1, "ambiguous protected vertex identity");
        remap.insert(v, candidates[0]);
        Ok(candidates[0])
    };
    for (fi, t) in before.triangles.iter().enumerate() {
        if before.hidden_faces.get(fi) == Some(&true) || t.iter().any(|&v| before.mask[v] > 0.) {
            let mapped = [protect(t[0])?, protect(t[1])?, protect(t[2])?];
            let face = after
                .triangles
                .iter()
                .position(|q| {
                    *q == mapped
                        || *q == [mapped[1], mapped[2], mapped[0]]
                        || *q == [mapped[2], mapped[0], mapped[1]]
                })
                .ok_or("protected connectivity changed")?;
            require!(
                after.polygroups[face] == before.polygroups[fi],
                "protected face group changed"
            );
            require!(
                after.hidden_faces.get(face).copied().unwrap_or(false)
                    == before.hidden_faces.get(fi).copied().unwrap_or(false),
                "protected visibility changed"
            );
            if before.hidden_faces.get(fi) == Some(&true) {
                for (v, w) in t.iter().zip(mapped) {
                    require!(
                        before.vertices[*v] == after.vertices[w],
                        "hidden surface moved"
                    );
                }
            }
        }
    }
    for q in &before.quads {
        if q.iter().any(|&v| before.mask[v] > 0.) {
            let mapped = [
                protect(q[0])?,
                protect(q[1])?,
                protect(q[2])?,
                protect(q[3])?,
            ];
            require!(after.quads.contains(&mapped), "protected quad changed");
        }
    }
    for v in 0..after.vertices.len() {
        require!(
            (after.mask[v] - after.vertex_groups[2].weights[v]).abs() < 2e-6,
            "adaptive mask/group transfer disagrees"
        );
        require!(
            (after.materials[v].metallic - after.vertex_groups[0].weights[v]).abs() < 2e-6,
            "adaptive material/group transfer disagrees"
        );
    }
    Ok(())
}
pub(super) fn remesh_contract(before: &Mesh, after: &Mesh) -> Check {
    require!(
        after.mask.iter().all(|&v| v == 0.),
        "voxel remesh must clear sculpt mask"
    );
    require!(
        !after.has_hidden_faces(),
        "voxel remesh introduced hidden faces"
    );
    require!(
        before.triangles != after.triangles,
        "remesh did not reconstruct topology"
    );
    for (a, b) in before.vertex_groups.iter().zip(&after.vertex_groups) {
        require!(
            a.name == b.name && a.visible == b.visible && a.purpose == b.purpose,
            "remesh changed group identity"
        );
    }
    // Brute-force nearest source samples, independent of the production kd tree.
    // Ties admit every equally near sample, rather than relying on traversal order.
    for (v, &p) in after.vertices.iter().enumerate() {
        let distances: Vec<_> = before
            .vertices
            .iter()
            .map(|&q| (p - q).dot(p - q))
            .collect();
        let best = distances.iter().copied().fold(f32::INFINITY, f32::min);
        require!(
            distances
                .iter()
                .enumerate()
                .any(|(i, &d)| d <= best + best.abs() * 1e-5 + 1e-12
                    && before
                        .vertex_groups
                        .iter()
                        .zip(&after.vertex_groups)
                        .all(|(a, b)| a.weights[i] == b.weights[v])),
            "remesh group sample is not nearest source"
        );
    }
    Ok(())
}
pub(super) fn saved_roundtrip(document: &Document) -> Result<Project, String> {
    let path = std::env::temp_dir().join(format!(
        "clay-mixed-{}-{:?}.clay",
        std::process::id(),
        std::thread::current().id()
    ));
    let name = path.to_str().ok_or("temporary path is not UTF-8")?;
    let project = document.project();
    crate::persistence::save_project(name, &project)?;
    let loaded = crate::persistence::load_project(name);
    let _ = std::fs::remove_file(path);
    let loaded = loaded?;
    require!(loaded == project, "save/reopen changed project");
    Ok(loaded)
}
pub(super) fn history_roundtrip(app: &mut App, before: &Project, previous: usize) -> Check {
    let after = app.document.project();
    require!(
        app.document.history_counts() == (previous + 1, 0),
        "edit did not commit exactly once"
    );
    app.document.undo();
    require!(
        app.document.project() == *before,
        "Undo changed more than the edit"
    );
    app.document.redo();
    require!(
        app.document.project() == after,
        "Redo did not restore exact edit"
    );
    saved_roundtrip(&app.document)?;
    Ok(())
}
pub(super) fn atomic(
    app: &App,
    before: &Project,
    history: (usize, usize),
    dirty: bool,
    revision: u64,
) -> Check {
    require!(
        app.document.project() == *before,
        "rejection/no-op changed project"
    );
    require!(
        app.document.history_counts() == history,
        "rejection/no-op changed history or Redo"
    );
    require!(
        app.document.is_dirty() == dirty,
        "rejection/no-op changed dirty state"
    );
    require!(
        app.document.display_revision() == revision,
        "rejection/no-op changed revision"
    );
    Ok(())
}
pub(super) fn worker(app: &mut App) -> Check {
    app.start_mesh_job(Operation::Remesh)?;
    test_support::finish_mesh_job(app);
    Ok(())
}
pub(super) fn noop_probe(app: &mut App, brush: &Brush, dab: &SavedDab, remesh: bool) -> Check {
    let after = app.document.project();
    require!(
        app.document.can_undo(),
        "probe has no successful edit to Undo"
    );
    app.document.undo();
    let before = app.document.project();
    let history = app.document.history_counts();
    let dirty = app.document.is_dirty();
    let revision = app.document.display_revision();
    require!(history.1 > 0, "probe did not establish Redo");
    let mut zero = brush.clone();
    zero.strength = 0.;
    require!(
        !app.document.apply_dab(&zero, dab.dab()),
        "zero strength edited mesh"
    );
    app.document.end_stroke();
    atomic(app, &before, history, dirty, revision)?;
    let invalid = AdaptiveSettings {
        detail: 0.,
        ..app.document.mesh_settings().adaptive.clone()
    };
    require!(
        app.document.set_adaptive(invalid).is_err(),
        "invalid settings accepted"
    );
    atomic(app, &before, history, dirty, revision)?;
    if !closed_surface(&before.mesh) {
        for topology in [Topology::Triangles, Topology::Quads] {
            let unsupported = AdaptiveSettings {
                enabled: true,
                topology,
                ..Default::default()
            };
            let error = app
                .document
                .set_adaptive(unsupported)
                .expect_err("open adaptive mode must be rejected");
            require!(
                error.contains("closed"),
                "open adaptive rejection used the wrong guard"
            );
            atomic(app, &before, history, dirty, revision)?;
        }
    }
    if remesh {
        worker(app)?;
        atomic(app, &before, history, dirty, revision)?;
        require!(
            app.ui.status.contains("Show all faces") || app.ui.status.contains("closed"),
            "remesh failed at the wrong guard"
        );
    }
    app.document.redo();
    require!(
        app.document.project() == after,
        "Redo lost after rejected operation"
    );
    Ok(())
}
