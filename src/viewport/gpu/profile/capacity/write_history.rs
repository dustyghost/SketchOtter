//! Paired CPU measurements. Both documents use the application's dab pipeline.
//! Only the candidate substitutes a position journal for the geometry snapshot.
use super::*;
use crate::document::stroke_capture::{Capture, Entry};
use crate::sculpt::Symmetry;

fn position_bits(p: Vec3) -> [u32; 3] {
    [p.0.to_bits(), p.1.to_bits(), p.2.to_bits()]
}

fn assert_same(reference: &Document, candidate: &Document) {
    assert!(reference.mesh() == candidate.mesh());
    assert_eq!(reference.mesh_settings(), candidate.mesh_settings());
    assert!(
        reference
            .mesh()
            .vertices
            .iter()
            .zip(&candidate.mesh().vertices)
            .all(|(&a, &b)| position_bits(a) == position_bits(b))
    );
}

fn record(metrics: &mut Measurements, name: &str, elapsed: std::time::Duration) {
    metrics
        .values
        .entry(name.into())
        .or_default()
        .push(elapsed.as_secs_f64() * 1000.);
}

fn stroke(
    doc: &mut Document,
    brush: &Brush,
    index: usize,
    tracked: bool,
    metrics: &mut Measurements,
) -> Option<Entry> {
    let kind = if index == 2 { "broad" } else { "local" };
    let mode = if tracked { "capture" } else { "document" };
    let prefix = format!("{mode}.{kind}");
    let mut dabs = Vec::with_capacity(DABS);
    let start = Instant::now();
    let mut capture = if tracked {
        Some(Capture::begin(doc))
    } else {
        doc.begin_stroke();
        None
    };
    let begin = start.elapsed();
    let mut first_end = begin;
    for step in 0..DABS {
        let mut d = dab(step);
        d.center.1 += index as f32 * 0.025;
        if index == 2 {
            d.radius = 0.85;
        }
        let now = Instant::now();
        doc.begin_sculpt_frame();
        let changed = match &mut capture {
            Some(c) => c.dab(doc, brush, d),
            None => doc.apply_dab(brush, d),
        };
        assert!(changed);
        dabs.push(now.elapsed());
        if step == 0 {
            first_end = start.elapsed();
        }
    }
    let end = Instant::now();
    let entry = if let Some(c) = capture {
        Some(c.finish(doc))
    } else {
        assert!(doc.end_stroke());
        None
    };
    let release = end.elapsed();
    let whole = start.elapsed();
    for (phase, time) in [
        ("begin", begin),
        ("begin_to_first_dab", first_end),
        ("release", release),
        ("stroke", whole),
    ] {
        record(metrics, &format!("{prefix}.{phase}"), time);
    }
    for time in &dabs[1..] {
        record(metrics, &format!("{prefix}.sustained_dab"), *time);
    }
    entry
}

pub(super) fn run(
    doc: &mut Document,
    baseline: &Geometry,
    repeats: usize,
    metrics: &mut Measurements,
) -> serde_json::Value {
    let mut candidate = document(baseline.clone());
    let symmetry = Symmetry {
        enabled: true,
        ..Default::default()
    };
    doc.set_symmetry(symmetry.clone()).unwrap();
    candidate.set_symmetry(symmetry).unwrap();
    let brush = brush(Kind::Standard);
    let mut trials = Vec::new();
    let mut output_hash = None;
    for trial in 0..repeats {
        let mut entries = Vec::new();
        let mut sizes = Vec::new();
        for index in 0..3 {
            metrics.checkpoint("paired_stroke", "running");
            // Alternate which implementation touches its data first.
            let entry = if (index + trial).is_multiple_of(2) {
                stroke(doc, &brush, index, false, metrics);
                stroke(&mut candidate, &brush, index, true, metrics)
            } else {
                let entry = stroke(&mut candidate, &brush, index, true, metrics);
                stroke(doc, &brush, index, false, metrics);
                entry
            }
            .unwrap();
            assert_same(doc, &candidate);
            assert!(entry.len() > 0);
            sizes.push(serde_json::json!({"changed_vertices":entry.len(), "position_bytes":entry.position_bytes()}));
            entries.push(entry);
        }
        let hash = fingerprint(doc.mesh());
        if let Some(previous) = output_hash.replace(hash.clone()) {
            assert_eq!(previous, hash);
        }
        let estimated = doc
            .diagnostic_manifest(Default::default())
            .retained_estimated_bytes;
        for e in entries.iter_mut().rev() {
            metrics.time("document.undo", || doc.undo());
            metrics.time("capture.undo", || e.swap(&mut candidate));
            assert_same(doc, &candidate);
        }
        assert_restored_three(doc, baseline);
        for e in &mut entries {
            metrics.time("document.redo", || doc.redo());
            metrics.time("capture.redo", || e.swap(&mut candidate));
            assert_same(doc, &candidate);
        }
        assert_eq!(doc.history_counts(), (3, 0));
        for e in entries.iter_mut().rev() {
            doc.undo();
            e.swap(&mut candidate);
        }
        assert_restored_three(doc, baseline);
        assert_same(doc, &candidate);
        trials.push(
            serde_json::json!({"entries":sizes,"document_history_estimated_bytes":estimated}),
        );
    }
    serde_json::json!({"trials":trials, "output_hash":output_hash.unwrap()})
}

fn assert_restored_three(doc: &Document, before: &Geometry) {
    assert!(doc.geometry_matches(before));
    assert_eq!(doc.history_counts(), (0, 3));
}

#[test]
fn write_history_handles_repeated_dabs_cancel_and_noop() {
    let mut reference = document(fixture("hidden", 0));
    let original = reference.geometry_snapshot();
    let mut candidate = document(original.clone());
    let brush = brush(Kind::Standard);
    let mut c = Capture::begin(&mut candidate);
    let miss = Dab {
        center: Vec3(9., 9., 9.),
        ..dab(0)
    };
    assert!(!c.dab(&mut candidate, &brush, miss));
    assert_eq!(c.finish(&mut candidate).len(), 0);
    let dirty_before = candidate.is_dirty();
    let mut cancelled = Capture::begin(&mut candidate);
    assert!(cancelled.dab(&mut candidate, &brush, dab(0)));
    cancelled.cancel(&mut candidate);
    assert_same(&reference, &candidate);
    assert_eq!(candidate.is_dirty(), dirty_before);
    assert_eq!(candidate.history_counts(), (0, 0));

    let mut c = Capture::begin(&mut candidate);
    for i in 0..DABS {
        assert_eq!(
            c.dab(&mut candidate, &brush, dab(i)),
            reference.apply_dab(&brush, dab(i))
        );
        assert_same(&reference, &candidate);
    }
    reference.end_stroke();
    let mut entry = c.finish(&mut candidate);
    assert_eq!(
        entry.len(),
        reference
            .mesh()
            .vertices
            .iter()
            .zip(&original.mesh.vertices)
            .filter(|(a, b)| position_bits(**a) != position_bits(**b))
            .count()
    );
    entry.swap(&mut candidate);
    reference.undo();
    assert_same(&reference, &candidate);
    entry.swap(&mut candidate);
    reference.redo();
    assert_same(&reference, &candidate);
}
