//! Bounded compositions of real document edits, worker commits and persistence.
use super::*;
use crate::sculpt::{
    AdaptiveSettings, Alpha, Brush, Dab, Geometry, GroupPurpose, Kind, Material, Mesh, Primitive,
    PrimitiveKind, Project, Symmetry, Topology, Vec3, VertexGroup,
    test_support::{closed_surface, fan, surface_error},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Fixture {
    Sphere,
    Grid,
    Fan,
    Bicone,
    NearbySheets,
}
#[derive(Clone, Serialize, Deserialize)]
struct SavedDab {
    center: Vec3,
    normal: Vec3,
    right: Vec3,
    up: Vec3,
    radius: f32,
}
impl SavedDab {
    fn dab(&self) -> Dab {
        Dab {
            center: self.center,
            normal: self.normal,
            right: self.right,
            up: self.up,
            radius: self.radius,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        }
    }
}
#[derive(Clone, Copy, Serialize, Deserialize)]
enum Intent {
    Refine,
    Simplify,
    Sculpt,
}
#[derive(Clone, Serialize, Deserialize)]
enum Step {
    Adaptive(AdaptiveSettings),
    Stroke {
        brush: Brush,
        dab: SavedDab,
        intent: Intent,
    },
    NoopAndRejection {
        brush: Brush,
        dab: SavedDab,
    },
    Dynamic,
    RejectedRemesh {
        brush: Brush,
        dab: SavedDab,
    },
    ShowAll,
    Remesh {
        succeeds: bool,
    },
    SaveReopen,
}
#[derive(Clone, Serialize, Deserialize)]
struct Replay {
    version: u32,
    seed: u64,
    fixture: Fixture,
    scale: f32,
    initial: Project,
    steps: Vec<Step>,
    require_remesh: bool,
}
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    step: usize,
    contract: String,
}
type Check = Result<(), String>;
macro_rules! require {
    ($condition:expr, $message:expr) => {
        let satisfied: bool = $condition;
        if !satisfied {
            return Err($message.to_string());
        }
    };
}
#[derive(Debug, Default)]
struct Counts {
    refined: usize,
    simplified: usize,
    sculpted: usize,
    remeshed: usize,
    rejected: usize,
    reopened: usize,
}

mod fixtures;
use fixtures::generate;
mod contracts;
use contracts::*;

fn run_inner(replay: &Replay) -> Result<Counts, Failure> {
    let mut app = App::default();
    app.document
        .replace_project(replay.initial.clone(), "mixed-sequence.clay")
        .map_err(|contract| Failure { step: 0, contract })?;
    let closed = matches!(replay.fixture, Fixture::Sphere | Fixture::Bicone);
    invariant(app.document.mesh(), closed, false)
        .map_err(|contract| Failure { step: 0, contract })?;
    let mut counts = Counts::default();
    let mut remeshed = false;
    for (index, step) in replay.steps.iter().enumerate() {
        let result = (|| -> Check {
            let before = app.document.project();
            let history = app.document.history_counts();
            let dirty = app.document.is_dirty();
            let revision = app.document.display_revision();
            let mut undoable = true;
            match step {
                Step::Adaptive(settings) => {
                    // Same-mode numeric settings are preferences, not separate Undo entries.
                    app.document.set_adaptive(settings.clone())?;
                    undoable = app.document.history_counts().0 != history.0;
                }
                Step::Stroke { brush, dab, intent } => {
                    let changed = app.document.apply_dab(brush, dab.dab());
                    app.document.end_stroke();
                    let after = app.document.mesh();
                    require!(
                        changed == (after != &before.mesh),
                        "stroke return value disagrees with edit"
                    );
                    if !changed {
                        atomic(&app, &before, history, dirty, revision)?;
                        undoable = false;
                    }
                    match intent {
                        Intent::Refine => {
                            if after.vertices.len() > before.mesh.vertices.len() {
                                counts.refined += 1;
                            }
                        }
                        Intent::Simplify => {
                            if after.vertices.len() < before.mesh.vertices.len() {
                                counts.simplified += 1;
                            }
                        }
                        Intent::Sculpt => {
                            require!(changed, "generated sculpt missed the surface");
                            require!(
                                after.triangles == before.mesh.triangles
                                    && after.quads == before.mesh.quads,
                                "plain sculpt changed topology"
                            );
                            require!(
                                after.mask == before.mesh.mask
                                    && after.vertex_groups == before.mesh.vertex_groups
                                    && after.materials == before.mesh.materials,
                                "plain sculpt changed attributes"
                            );
                            for (v, &position) in before.mesh.vertices.iter().enumerate() {
                                let hidden =
                                    before.mesh.triangles.iter().enumerate().any(|(f, t)| {
                                        t.contains(&v)
                                            && before.mesh.hidden_faces.get(f) == Some(&true)
                                    });
                                if before.mesh.mask[v] == 1. || hidden {
                                    require!(
                                        after.vertices[v] == position,
                                        "plain sculpt moved protected vertex"
                                    );
                                }
                            }
                            counts.sculpted += 1;
                        }
                    }
                    if app.document.mesh_settings().adaptive.enabled && !remeshed {
                        protected(&before.mesh, after)?;
                    }
                }
                Step::NoopAndRejection { brush, dab } => {
                    noop_probe(&mut app, brush, dab, false)?;
                    counts.rejected += 1;
                    undoable = false;
                }
                Step::Dynamic => {
                    app.document.set_dynamic_mesh(true)?;
                }
                Step::RejectedRemesh { brush, dab } => {
                    // Make a real, undoable edit while Dynamic mode is already on,
                    // then test hidden/open guards with a live Redo branch.
                    require!(
                        app.document.apply_dab(brush, dab.dab()),
                        "rejection probe sculpt missed"
                    );
                    app.document.end_stroke();
                    history_roundtrip(&mut app, &before, history.0)?;
                    noop_probe(&mut app, brush, dab, true)?;
                    counts.rejected += 1;
                    undoable = false;
                }
                Step::ShowAll => {
                    app.document.show_all_faces()?;
                }
                Step::Remesh { succeeds } => {
                    worker(&mut app)?;
                    if *succeeds {
                        require!(
                            app.document.project() != before,
                            "successful remesh was not applied"
                        );
                        remesh_contract(&before.mesh, app.document.mesh())?;
                        remeshed = true;
                        counts.remeshed += 1;
                    } else {
                        atomic(&app, &before, history, dirty, revision)?;
                        counts.rejected += 1;
                        undoable = false;
                    }
                }
                Step::SaveReopen => {
                    let saved = saved_roundtrip(&app.document)?;
                    app.document.replace_project(saved, "mixed-reopened.clay")?;
                    require!(
                        app.document.history_counts() == (0, 0) && !app.document.is_dirty(),
                        "reopen did not reset transient history"
                    );
                    counts.reopened += 1;
                    undoable = false;
                }
            }
            if undoable && app.document.project() != before {
                history_roundtrip(&mut app, &before, history.0)?;
            }
            let all_quads = app.document.mesh_settings().adaptive.enabled
                && app.document.mesh_settings().adaptive.topology == Topology::Quads
                || remeshed && app.document.mesh_settings().remesh_topology == Topology::Quads;
            invariant(app.document.mesh(), closed, all_quads)
        })();
        result.map_err(|contract| Failure {
            step: index,
            contract,
        })?;
    }
    if (closed && (counts.refined == 0 || counts.simplified == 0))
        || counts.sculpted == 0
        || counts.rejected == 0
        || counts.reopened == 0
        || (replay.require_remesh && counts.remeshed == 0)
    {
        return Err(Failure {
            step: replay.steps.len(),
            contract: format!(
                "coverage: refine={}, simplify={}, sculpt={}, remesh={}, reject={}, reopen={}",
                counts.refined,
                counts.simplified,
                counts.sculpted,
                counts.remeshed,
                counts.rejected,
                counts.reopened
            ),
        });
    }
    Ok(counts)
}
fn run(replay: &Replay) -> Result<Counts, Failure> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(replay))).unwrap_or_else(
        |panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            Err(Failure {
                step: usize::MAX,
                contract: format!("panic: {message}"),
            })
        },
    )
}
fn artifact_dir() -> PathBuf {
    std::env::var_os("CLAY_SEQUENCE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/invariant-failures"))
}
fn retain(replay: &Replay, failure: &Failure, suffix: &str) -> PathBuf {
    let dir = artifact_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("mixed-{}-{suffix}.json", replay.seed));
    std::fs::write(&path, serde_json::to_vec_pretty(replay).unwrap()).unwrap();
    std::fs::write(path.with_extension("failure.txt"), format!("{failure:?}\n")).unwrap();
    path
}
fn check(replay: &Replay) {
    let result = run(replay);
    if let Ok(counts) = &result {
        println!("mixed seed={} {counts:?}", replay.seed);
    }
    if let Err(failure) = result {
        let path = retain(replay, &failure, "original");
        panic!(
            "{failure:?}; seed={}; replay={}",
            replay.seed,
            path.display()
        );
    }
}

#[test]
fn mixed_adaptive_sculpt_remesh_sequences() {
    for (seed, fixture, scale, quads, mirror) in [
        (17, Fixture::Sphere, 0.01, false, false),
        (28, Fixture::Sphere, 1., true, true),
        (39, Fixture::Sphere, 100., false, true),
        (50, Fixture::Sphere, 0.01, true, false),
        (61, Fixture::Grid, 1., false, true),
        (72, Fixture::Bicone, 100., false, false),
        (105, Fixture::Fan, 1., false, false),
        (83, Fixture::NearbySheets, 0.01, false, true),
        (94, Fixture::Grid, 100., false, false),
    ] {
        check(&generate(seed, fixture, scale, quads, mirror));
    }
}

// Greedy single-step deletion, capped by attempts and a campaign deadline. Only
// the original failure contract is interesting; rejected prerequisites and lost
// success coverage never count as a reduction of a topology/history failure.
fn reduce(
    mut replay: Replay,
    mut interesting: impl FnMut(&Replay) -> bool,
    attempts: usize,
) -> Replay {
    let mut index = 0;
    for _ in 0..attempts {
        if index >= replay.steps.len() {
            break;
        }
        let mut candidate = replay.clone();
        candidate.steps.remove(index);
        if interesting(&candidate) {
            replay = candidate;
        } else {
            index += 1;
        }
    }
    replay
}
#[test]
fn mixed_reducer_retains_only_reproducing_steps_and_respects_budget() {
    let mut replay = generate(1, Fixture::Grid, 1., false, false);
    replay.steps = vec![
        Step::ShowAll,
        Step::Adaptive(AdaptiveSettings {
            enabled: true,
            ..Default::default()
        }),
        Step::Dynamic,
        Step::SaveReopen,
        Step::ShowAll,
    ];
    // An intentionally unsupported open-surface request is a real, stable
    // failure predicate for testing reduction without injecting a mesh defect.
    let expected = run(&replay).unwrap_err().contract;
    assert!(expected.contains("closed"));
    let mut calls = 0;
    let reduced = reduce(
        replay.clone(),
        |candidate| {
            calls += 1;
            run(candidate).err().is_some_and(|e| e.contract == expected)
        },
        3,
    );
    assert_eq!(calls, 3);
    assert_eq!(reduced.steps.len(), 3);
    let decoded: Replay = serde_json::from_slice(&serde_json::to_vec(&reduced).unwrap()).unwrap();
    assert_eq!(run(&decoded).unwrap_err().contract, expected);
    assert_eq!(reduce(replay, |_| true, 0).steps.len(), 5);
}
#[test]
#[ignore = "opt-in bounded campaign/replay/reduction; see docs/testing.md"]
fn mixed_sequence_campaign() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    if let Ok(path) = std::env::var("CLAY_SEQUENCE_REPLAY") {
        let replay: Replay = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(replay.version, 1);
        assert!(replay.steps.len() <= 64);
        if std::env::var_os("CLAY_SEQUENCE_REDUCE").is_some() {
            let failure = run(&replay).expect_err("input must reproduce a failure");
            let reduced = reduce(
                replay,
                |candidate| {
                    std::time::Instant::now() < deadline
                        && run(candidate)
                            .err()
                            .is_some_and(|e| e.contract == failure.contract)
                },
                16,
            );
            assert_eq!(run(&reduced).unwrap_err().contract, failure.contract);
            eprintln!(
                "reduced replay: {}",
                retain(&reduced, &failure, "reduced").display()
            );
        } else {
            check(&replay);
        }
    } else {
        let first = std::env::var("CLAY_SEQUENCE_SEED")
            .unwrap_or_else(|_| "1000".into())
            .parse::<u64>()
            .unwrap();
        let cases = std::env::var("CLAY_SEQUENCE_CASES")
            .unwrap_or_else(|_| "8".into())
            .parse::<usize>()
            .unwrap();
        assert!((1..=32).contains(&cases));
        for i in 0..cases {
            assert!(
                std::time::Instant::now() < deadline,
                "campaign safety deadline"
            );
            let fixture = [
                Fixture::Sphere,
                Fixture::Grid,
                Fixture::Bicone,
                Fixture::NearbySheets,
            ][i % 4];
            check(&generate(
                first.wrapping_add(i as u64),
                fixture,
                [0.01, 1., 100.][i % 3],
                matches!(fixture, Fixture::Sphere) && i % 2 == 0,
                i % 2 != 0,
            ));
        }
    }
}
