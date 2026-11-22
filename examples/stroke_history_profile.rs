//! Measure a position-only history representation against the real document's Undo/Redo.
//! Run the release example once per subdivision count (0..=3).
use clay_form::{
    document::Document,
    sculpt::{Brush, Dab, Geometry, Kind, Material, Mesh, Symmetry, Vec3},
};
use serde_json::json;
use std::time::Instant;

const STROKES: usize = 3;
const DABS: usize = 12;

fn bits(p: Vec3) -> [u32; 3] {
    [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()]
}

// Store the previous value once per changed vertex. Swapping makes the same
// entry usable in both directions. Only valid for a fixed vertex identity/order.
struct PositionPatch(Vec<(u32, Vec3)>);

impl PositionPatch {
    fn between(before: &[Vec3], after: &[Vec3]) -> Self {
        assert_eq!(before.len(), after.len());
        let mut entries = Vec::new();
        for (index, (&old, &new)) in before.iter().zip(after).enumerate() {
            if bits(old) != bits(new) {
                entries.push((u32::try_from(index).unwrap(), old));
            }
        }
        Self(entries)
    }

    fn swap(&mut self, positions: &mut [Vec3]) {
        for (index, saved) in &mut self.0 {
            std::mem::swap(&mut positions[*index as usize], saved);
        }
    }
}

fn fixture(subdivisions: usize) -> Document {
    let mut geometry = Geometry::new(Mesh::starting_sphere());
    for _ in 0..subdivisions {
        geometry.subdivide().unwrap();
    }
    let mesh = &mut geometry.mesh;
    // A ridged surface with a protected centreline. Each mirrored stroke
    // overlaps its predecessor while editing both sides of the protected band.
    for (p, mask) in mesh.vertices.iter_mut().zip(&mut mesh.mask) {
        p.2 *= 1.0 + 0.025 * (18.0 * p.0).cos() * (11.0 * p.1).sin();
        *mask = if p.0.abs() < 0.025 { 1.0 } else { 0.0 };
    }
    mesh.materials = vec![Material::from_srgb([0.25, 0.45, 0.6], 0.65, 0.1); mesh.vertices.len()];
    let mut doc = Document::default();
    let mut project = doc.project();
    project.mesh = geometry.mesh;
    project.mesh_settings = geometry.settings;
    doc.replace_project(project, "position-history.clay")
        .unwrap();
    doc.set_symmetry(Symmetry {
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    doc
}

fn timed<T>(action: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let result = action();
    (result, start.elapsed().as_secs_f64() * 1000.)
}

fn assert_same_mesh(replay: &Mesh, expected: &Mesh) {
    assert!(
        replay == expected,
        "active mesh or attributes differ from document history"
    );
    assert!(
        replay
            .vertices
            .iter()
            .zip(&expected.vertices)
            .all(|(&a, &b)| bits(a) == bits(b)),
        "position bits differ from document history"
    );
}

fn main() {
    let subdivisions: usize = std::env::args()
        .nth(1)
        .expect("supply subdivision count 0..=3")
        .parse()
        .expect("integer subdivision count");
    assert!(subdivisions <= 3, "fixture exceeds the default mesh budget");
    let mut doc = fixture(subdivisions);
    let mut brush = Brush::new(Kind::Standard);
    brush.strength = 0.25;
    let mut patches = Vec::new();
    let mut samples = Vec::new();
    for stroke in 0..STROKES {
        // Keep the before-image cost visible. The prototype does a full diff at release.
        let (before, copy_ms) = timed(|| doc.mesh().vertices.clone());
        let (_, begin_ms) = timed(|| doc.begin_stroke());
        for step in 0..DABS {
            let dab = Dab {
                center: Vec3(
                    0.035 + step as f32 * 0.008,
                    0.02 + stroke as f32 * 0.035,
                    1.,
                ),
                normal: Vec3(0., 0., 1.),
                right: Vec3(1., 0., 0.),
                up: Vec3(0., 1., 0.),
                radius: 0.18,
                pressure: 0.65 + (step % 3) as f32 * 0.1,
                delta: Vec3::default(),
                invert: false,
            };
            assert!(doc.apply_dab(&brush, dab));
        }
        assert!(doc.end_stroke());
        assert_eq!(doc.history_counts(), (stroke + 1, 0));
        let (patch, diff_ms) = timed(|| PositionPatch::between(&before, &doc.mesh().vertices));
        assert!(patch.0.iter().any(|&(_, p)| p.0 < 0.0));
        assert!(patch.0.iter().any(|&(_, p)| p.0 > 0.0));
        assert!(
            patch
                .0
                .iter()
                .all(|&(i, _)| doc.mesh().mask[i as usize] < 1.)
        );
        samples.push(json!({
            "stroke": stroke + 1, "changed_vertices": patch.0.len(),
            "dense_position_bytes": before.capacity() * size_of::<Vec3>(),
            "patch_bytes": patch.0.capacity() * size_of::<(u32, Vec3)>(),
            "copy_positions_ms": copy_ms, "document_begin_ms": begin_ms, "diff_ms": diff_ms,
        }));
        patches.push(patch);
    }

    let mut replay = doc.mesh().clone();
    for (patch, sample) in patches.iter_mut().zip(&mut samples).rev() {
        let (_, document_ms) = timed(|| doc.undo());
        sample["document_undo_ms"] = json!(document_ms);
        let (_, ms) = timed(|| patch.swap(&mut replay.vertices));
        sample["undo_patch_ms"] = json!(ms);
        assert_same_mesh(&replay, doc.mesh());
    }
    assert_eq!(doc.history_counts(), (0, STROKES));
    for (patch, sample) in patches.iter_mut().zip(&mut samples) {
        let (_, document_ms) = timed(|| doc.redo());
        sample["document_redo_ms"] = json!(document_ms);
        let (_, ms) = timed(|| patch.swap(&mut replay.vertices));
        sample["redo_patch_ms"] = json!(ms);
        assert_same_mesh(&replay, doc.mesh());
    }
    assert_eq!(doc.history_counts(), (STROKES, 0));
    println!(
        "{}",
        json!({
            "subdivisions": subdivisions, "dabs_per_stroke": DABS, "samples": samples,
            "vertices": replay.vertices.len(), "quads": replay.quads.len(), "triangles": replay.triangles.len(),
            "stored_vertices": replay.vertices.len() + doc.mesh_settings().lower.iter().map(|m| m.vertices.len()).sum::<usize>(),
            "document_history_estimated_bytes": doc.diagnostic_manifest(Default::default()).retained_estimated_bytes,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_round_trips_signed_zero_and_leaves_neighbours_alone() {
        let old = [Vec3(1., 2., 3.), Vec3(-0.0, 1., 0.)];
        let mut positions = [old[0], Vec3(0.0, 1., 0.)];
        let new = positions;
        let mut patch = PositionPatch::between(&old, &new);
        assert_eq!(patch.0.len(), 1);
        patch.swap(&mut positions);
        assert_eq!(positions.map(bits), old.map(bits));
        patch.swap(&mut positions);
        assert_eq!(positions.map(bits), new.map(bits));
    }
}
