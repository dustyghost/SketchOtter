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
retained as follows (function names below are unchanged):

| Owning module | Cases | Contract |
| --- | ---: | --- |
| `app::retopology_tests` | 5 | Private preview, Apply, source and attribute preservation, cancellation, failure/staleness, input limits, guide/path state, mirror inheritance, history and project round trip |
| `document::retopology::tests` | 4 | Density Undo/save/subdivision, mask/group independence across levels, group-all replacement, atomic replacement at the group limit |
| `sculpt::retopology::tests` | 6 | Exact solver coordinates, real quads, sculpt-mask independence, multiple/mirrored/stale guides, cancellation/limits, signed density, opt-in whole-sculpt adapter |
| `app::tests::guide_captures` (default harness) | 3 | `native_retopology_controls_offscreen_review`, `native_retopology_whole_sculpt_offscreen_review`, `guided_retopology_offscreen_review` |

The three group tests moved from `document::tests`; the five app tests moved from
`app::tests::projection`. There are no duplicate bodies or test-name selection
rules. The GPU control test remains in ordinary graphics CI; the two existing
manual captures and whole-sculpt adapter remain opt-in. Native solver crate
coverage is unchanged. CI runs both default and focused harnesses and checks
that the focused test inventory is a subset of the full inventory, includes all
three owning CPU modules and contains no offscreen captures.

See the [measurement record](validation/testing-batch-2026-11-22/README.md) for
compiler/profile, guarded memory measurements and comparison with staging.
