//! Paired freehand Paint timings from pointer-down. The baseline replays the
//! former viewport call; both arms use the same document and Paint code.
use super::*;

fn paint(doc: &mut Document, prior: bool, state: &str) -> serde_json::Value {
    let brush = brush(Kind::Paint);
    let before = fingerprint(doc.mesh());
    let history = doc.history_counts().0;
    let mut samples = Vec::new();
    for step in 0..4 {
        let start = Instant::now();
        let (changed, stages) = crate::sculpt::texture::profile::capture(|| {
            if step == 0 {
                if prior {
                    doc.begin_stroke();
                } else {
                    doc.begin_freehand_stroke(Kind::Paint);
                }
                crate::sculpt::texture::profile::mark("pointer_down");
            }
            doc.apply_dab(&brush, dab(step))
        });
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        assert!(changed);
        samples.push(serde_json::json!({"total_ms":elapsed,"stages_ms":stages}));
    }
    doc.end_stroke();
    assert_eq!(doc.history_counts(), (history + 1, 0));
    let after = fingerprint(doc.mesh());
    assert_ne!(before, after);
    let bytes = doc
        .diagnostic_manifest(Default::default())
        .retained_estimated_bytes;
    doc.undo();
    assert_eq!(fingerprint(doc.mesh()), before);
    doc.redo();
    assert_eq!(fingerprint(doc.mesh()), after);
    doc.undo();
    serde_json::json!({"state":state,"samples":samples,"before_hash":before,
        "after_hash":after,"history_estimated_bytes":bytes})
}

fn sequence(level: usize, prior: bool) -> Vec<serde_json::Value> {
    let mut doc = document(fixture("hidden", level));
    let mut rows = Vec::new();
    for state in ["cold", "warm", "after_deformation", "after_visibility"] {
        match state {
            "after_deformation" => {
                assert!(doc.apply_dab(&brush(Kind::Standard), dab(0)));
                assert!(doc.end_stroke());
            }
            "after_visibility" => {
                assert!(doc.mesh().hidden_faces.iter().any(|&hidden| hidden));
                doc.show_all_faces().unwrap();
                assert!(doc.mesh().hidden_faces.iter().all(|&hidden| !hidden));
            }
            _ => {}
        }
        rows.push(paint(&mut doc, prior, state));
    }
    rows
}

#[test]
#[ignore = "manual Paint startup measurement"]
fn paint_pointer_startup() {
    let level = env_limit("CLAY_PAINT_LEVEL", 3);
    let repeats = env_limit("CLAY_PAINT_REPEATS", 3);
    assert!((1..=10).contains(&repeats));
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let output = std::env::var("CLAY_PAINT_PROFILE").expect("output path");
    let mut trials = Vec::new();
    for trial in 0..repeats {
        // Drop each document before allocating the next; alternate execution order.
        let (baseline, candidate) = if trial % 2 == 0 {
            (sequence(level, true), sequence(level, false))
        } else {
            let candidate = sequence(level, false);
            (sequence(level, true), candidate)
        };
        for (a, b) in baseline.iter().zip(&candidate) {
            for field in ["before_hash", "after_hash", "history_estimated_bytes"] {
                assert_eq!(a[field], b[field]);
            }
        }
        trials.push(serde_json::json!({"baseline":baseline,"candidate":candidate}));
        std::fs::write(
            &output,
            serde_json::to_vec_pretty(&serde_json::json!({
                "level":level,"initial_triangles":9408*4_usize.pow(level as u32),
                "fixture":"hidden","trials":trials,"memory":memory()
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
