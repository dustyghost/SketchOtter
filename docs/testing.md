# Testing on a memory-limited development machine

The default `cargo test --locked` suite includes all application tests. CI also
runs the native solver crate and the existing GPU/control checks. Do not replace
the default invocation with `--all-features`: the `focused-retopology` feature
selects a smaller **test compilation** partition. It changes no production code.

## Focused retopology partition

```sh
free -h
cat /proc/pressure/memory
# Inspect host compiler/build processes too if the sandbox hides other sessions.
# Use an existing development CARGO_TARGET_DIR if one is available; never clean it.
flock -n /home/mark/.cache/codex-build.lock env \
  CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
  cargo test --locked -j 1 --lib --features focused-retopology -- --test-threads=1
```

If the lock is held or memory is tight, defer compilation. The application test
crate uses 256 LLVM code-generation units even with incremental compilation off.
This does not raise compiler concurrency. Local builds still use at most two
Cargo jobs, one under memory pressure, and heavy tests run on one thread.

The partition compiles the working production modules directly. Tests are owned
once by ordinary Rust modules; explicit `cfg(all(test, not(feature =
"focused-retopology")))` gates omit unrelated test modules. The default harness
includes every module, including the focused modules. Private editor internals
stay private. Shared test instrumentation remains available in both harnesses;
unused instrumentation/import warnings are allowed only in the focused test
crate. The complete default Clippy run remains mandatory.

The former staging script selected 18 cases on main at `67b1352`. Coverage is
retained and extended as follows:

| Owning module | Cases | Contract |
| --- | ---: | --- |
| `app::retopology_tests` | 7 | Private preview, Apply, source and attribute preservation, cancellation, failure/staleness, input limits, default Integer grid, open-surface rejection, selected/unselected path reprojection, unsupported symmetry rejection, history and project round trip |
| `document::retopology::tests` | 4 | Density Undo/save/subdivision, mask/group independence across levels, group-all replacement, atomic replacement at the group limit |
| `sculpt::retopology::tests` | 6 | Exact solver coordinates, real quads, sculpt-mask independence, multiple/mirrored/stale guides, cancellation/limits, signed density, opt-in whole-sculpt adapter |
| `app::tests::guide_captures` (default harness) | 3 | `native_retopology_controls_offscreen_review`, `native_retopology_whole_sculpt_offscreen_review`, `guided_retopology_offscreen_review` |

The three group tests moved from `document::tests`; the original five app tests
and two Integer grid additions moved from `app::tests::projection`. There are no
duplicate bodies or test-name selection rules. The GPU control test remains in ordinary graphics CI; the two existing
manual captures and whole-sculpt adapter remain opt-in. Native solver crate
coverage is unchanged. CI runs both default and focused harnesses and checks
that the focused test inventory is a subset of the full inventory, includes all
three owning CPU modules and contains no offscreen captures.

PR #190 establishes this partition and the mixed invariant runner against main.
PR #192 is stacked on #190 and updates the same shared retopology tests for the
new default solver and path attachments. Merge #190 first, then refresh #192
against the latest main and retarget it to main. Neither PR owns a second copy of these tests.

See the [measurement record](validation/testing-batch-2026-11-22/README.md) for
compiler/profile, guarded memory measurements and comparison with staging.

## Mixed edit sequences

`app::mixed_invariant_tests` is included in both harnesses. The ordinary budget
uses nine saved-seed recipes with small cube spheres, a closed irregular
high-valence bicone derived from #170's fan, an open fan, open grids and two
nearby open sheets. Scales are 0.01, 1 and 100, with mirror symmetry on/off,
partial/full masks, hidden faces, and nonuniform group and material channels.
The seed determines numeric brush/adaptive settings and the repeated-stroke
count; each failure saves the complete initial Project and **explicit steps**,
so replay does not depend on a future version of the fixture generator.

Existing coverage was inventoried before adding sequences:

| Existing tests | Retained responsibility |
| --- | --- |
| `sculpt/adaptive.rs` | Local split/collapse, group transport, candidate selection, protection and differential broad-phase/relaxation checks |
| `sculpt/adaptive/quads.rs` | True quad refine/coarsen, repeated adaptation, partial masks, hidden regions, symmetry and stencil protection |
| `sculpt/remesh.rs`, `sculpt/group_tests.rs` | Closed reconstruction, resolution, unions/disconnected volumes, cancellation/open-input rejection, material/group transfer and repeated surface drift |
| `document/tests.rs`, `document/stroke.rs` | Adaptive mode/history, invalid projects/settings, numeric setting updates and stroke boundaries |
| `sculpt/modeling/invariant_tests.rs` | #170's 36 Split/Weld sequences and pinched-vertex regressions; fixtures/oracle now shared in `sculpt/test_support.rs` |

The new runner uses `Document::set_adaptive`, `apply_dab`/`end_stroke`, Dynamic
Mesh's real `App::start_mesh_job`/poll/commit path, Undo/Redo, and the real project
save/load/reopen functions. Every undoable edit is checked for one history entry,
exact Undo/Redo and persistence. Rejection/no-op probes keep a live Redo branch
and compare the complete project, history counts, dirty flag and revision.

Expectations follow the operation:

- Adaptive closed cases must actually refine **and** simplify. Protected face
  connectivity, quad metadata, visibility and attributes are checked using
  nonuniform channel signatures across vertex renumbering. Mask/group/material
  relationships independently check interpolation, rather than constant fields.
- Plain sculpting must move geometry while retaining topology/attributes and
  keeping fully masked and hidden vertices fixed.
- Dynamic remesh must actually reconstruct a closed surface. It intentionally
  clears the sculpt mask and may change vertex IDs and component counts. Named
  group samples are checked against a brute-force nearest-source oracle; material
  variation and relationships between independently stored channels must survive.
- Open surfaces use successful sculpt/history/save transitions and explicit
  adaptive/remesh rejection checks. They are never forced through closed-surface
  guards. Successful closed remesh results are reopened and adapted again.
- The independent triangle/link flood oracle checks indices, finite coordinates,
  nonzero areas, duplicate faces, winding, connected fans and the actual triangles
  owned by each quad. Known-invalid fixtures include the #170 pinched vertex,
  reversed winding, invalid indices, nonfinite coordinates and invalid quad data.

Focused local command (same lock/memory precautions as above):

```sh
flock -n /home/mark/.cache/codex-build.lock env \
  CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
  cargo test --locked -j 1 --lib --features focused-retopology mixed_ -- --test-threads=1
```

Failures write `target/invariant-failures/mixed-SEED-original.json` and an adjacent
failure description; `CLAY_SEQUENCE_ARTIFACTS` overrides the directory. CI uploads
these files on failure. Replay or reduce a failure without regenerating it:

```sh
# Prefix with the guarded memory-probe invocation from the measurement record
# on the laptop. timeout also bounds an unexpectedly slow replay externally.
flock -n /home/mark/.cache/codex-build.lock timeout 180s env \
  CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
  CLAY_SEQUENCE_REPLAY=target/invariant-failures/mixed-SEED-original.json \
  cargo test --locked -j 1 --lib --features focused-retopology \
  mixed_sequence_campaign -- --ignored --nocapture --test-threads=1
# Add CLAY_SEQUENCE_REDUCE=1 to try at most 16 single-step deletions, keeping only
# candidates that reproduce the same failure contract. The reduced JSON remains
# replayable and keeps the initial fixture, seed and all retained settings.
```

The reducer has a 120-second campaign deadline and is exercised against a real
unsupported open-adaptive request; it cannot accept missing success coverage or
a different rejected prerequisite as the original topology/history failure.
A larger generated campaign is opt-in using the same command with
`CLAY_SEQUENCE_SEED=1000 CLAY_SEQUENCE_CASES=8` in place of `CLAY_SEQUENCE_REPLAY`.
The allowed count is 1–32; the campaign deadline and external timeout remain in
force. No performance threshold, capacity increase or full local suite is added.

These are sampled, bounded correctness checks. Manifold incidence and connected
fans do **not** prove that a surface has no self-intersections, and these tests
provide no continuous surface-error bound or general remeshing quality proof.
