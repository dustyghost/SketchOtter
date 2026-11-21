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
            .map(|faces| {
                let mut lo = Vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
                let mut hi = Vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
                for &v in faces.iter().flatten() {
                    let p = surface.mesh.vertices[v];
                    lo = Vec3(lo.0.min(p.0), lo.1.min(p.1), lo.2.min(p.2));
                    hi = Vec3(hi.0.max(p.0), hi.1.max(p.1), hi.2.max(p.2));
                }
                [lo, hi]
            })
            .collect();
        Self(bounds)
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
