//! Camera-independent bounds for consecutive blocks of source triangles.
use super::picking::GROUP;
use super::{DisplayMesh, Scene, Vec3, egui};

pub(super) struct WorldGroups(Vec<[Vec3; 2]>);

impl WorldGroups {
    pub fn build(surface: &DisplayMesh) -> Self {
        let bounds = surface
            .mesh
            .triangles
            .chunks(GROUP)
            .map(|faces| Self::bounds(surface, faces))
            .collect();
        Self(bounds)
    }

    fn bounds(surface: &DisplayMesh, faces: &[[usize; 3]]) -> [Vec3; 2] {
        let mut lo = Vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = Vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &v in faces.iter().flatten() {
            let p = surface.mesh.vertices[v];
            lo = Vec3(lo.0.min(p.0), lo.1.min(p.1), lo.2.min(p.2));
            hi = Vec3(hi.0.max(p.0), hi.1.max(p.1), hi.2.max(p.2));
        }
        [lo, hi]
    }

    pub fn project(&self, scene: &Scene) -> Vec<egui::Rect> {
        let raw: Vec<_> = self
            .0
            .iter()
            .map(|&[lo, hi]| {
                let mut rect = egui::Rect::NOTHING;
                for x in [lo.0, hi.0] {
                    for y in [lo.1, hi.1] {
                        for z in [lo.2, hi.2] {
                            let p = Vec3(x, y, z);
                            if !scene.visible_depth(p) {
                                return egui::Rect::EVERYTHING;
                            }
                            let q = scene.project(p);
                            if !q.is_finite() {
                                return egui::Rect::EVERYTHING;
                            }
                            rect.extend_with(q);
                        }
                    }
                }
                let magnitude = [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
                    .into_iter()
                    .map(f32::abs)
                    .fold(1_f32, f32::max);
                rect.expand(magnitude * (64. * f32::EPSILON))
            })
            .collect();
        // Unsorted GPU views keep source face order after backface filtering.
        // Union every source block spanned by this contiguous facing group.
        scene
            .all_faces
            .chunks(GROUP)
            .map(|faces| {
                raw[faces[0].0 / GROUP..=faces[faces.len() - 1].0 / GROUP]
                    .iter()
                    .copied()
                    .fold(egui::Rect::NOTHING, egui::Rect::union)
            })
            .collect()
    }
}

/// Compare rebuilding all world blocks with maintaining an extra reverse map.
/// This measurement includes constructing the reverse map and old bounds.
#[cfg(test)]
pub(in crate::viewport) fn profile_refit(
    before: &DisplayMesh,
    after: &DisplayMesh,
) -> serde_json::Value {
    use std::time::Instant;
    let positions = &before.mesh.vertices;
    assert_eq!(before.mesh.triangles, after.mesh.triangles);
    let start = Instant::now();
    let mut original = WorldGroups::build(before);
    let first_build = start.elapsed();
    let start = Instant::now();
    // Store one block ID per distinct source vertex/block pair. Iteration is in
    // block order, so a single last-seen value per vertex removes duplicates.
    let mut counts = vec![0u32; positions.len() + 1];
    let mut last = vec![u32::MAX; positions.len()];
    for (g, faces) in before.mesh.triangles.chunks(GROUP).enumerate() {
        for &i in faces.iter().flatten() {
            if last[i] != g as u32 {
                counts[i + 1] += 1;
                last[i] = g as u32;
            }
        }
    }
    for i in 0..positions.len() {
        counts[i + 1] += counts[i];
    }
    let mut cursor = counts[..positions.len()].to_vec();
    let mut groups = vec![0u32; counts[positions.len()] as usize];
    last.fill(u32::MAX);
    for (g, faces) in before.mesh.triangles.chunks(GROUP).enumerate() {
        for &i in faces.iter().flatten() {
            if last[i] != g as u32 {
                groups[cursor[i] as usize] = g as u32;
                cursor[i] += 1;
                last[i] = g as u32;
            }
        }
    }
    drop(last);
    drop(cursor);
    let reverse_build = start.elapsed();
    let scan = Instant::now();
    let changed: Vec<_> = positions
        .iter()
        .zip(after.mesh.vertices.iter())
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect();
    let scan_time = scan.elapsed();
    let refit = Instant::now();
    let mut dirty: Vec<_> = changed
        .iter()
        .flat_map(|&i| {
            groups[counts[i] as usize..counts[i + 1] as usize]
                .iter()
                .copied()
        })
        .collect();
    dirty.sort_unstable();
    dirty.dedup();
    for &g in &dirty {
        let i = g as usize * GROUP;
        original.0[g as usize] = WorldGroups::bounds(
            after,
            &after.mesh.triangles[i..(i + GROUP).min(after.mesh.triangles.len())],
        );
    }
    let refit_time = refit.elapsed();
    let rebuild = Instant::now();
    let expected = WorldGroups::build(after);
    let rebuild_time = rebuild.elapsed();
    assert_eq!(original.0, expected.0);
    serde_json::json!({"changed_vertices":changed.len(),"dirty_blocks":dirty.len(),"blocks":original.0.len(),"reference_bounds_bytes":original.0.capacity()*std::mem::size_of::<[Vec3;2]>(),"reverse_map_bytes":(counts.capacity()+groups.capacity())*4,"initial_bounds_ms":first_build.as_secs_f64()*1000.,"reverse_build_ms":reverse_build.as_secs_f64()*1000.,"whole_array_discovery_ms":scan_time.as_secs_f64()*1000.,"refit_ms":refit_time.as_secs_f64()*1000.,"rebuild_ms":rebuild_time.as_secs_f64()*1000.})
}
