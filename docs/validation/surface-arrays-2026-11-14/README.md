# Surface arrays and progressive rotation

Validated on 14 November 2026 on the branch based on saved-camera PR #147.

- 63 focused `instances::` tests passed. Coverage includes captured-asset sharing, face orientation/contact/fitting, logical quad placement, stable slots, hidden faces, Keep/Replace conversion, save/reopen, Undo/Redo, preview rollback, expanded-geometry limits, ownership, and existing array/path/brush regressions.
- The signed per-copy rotation regression covers X/Y/Z, negative and zero steps, a displaced source pivot and whole-layer rotation. Existing regressions cover layered and radial composition and persistence.
- `surface_array_controls_offscreen_review` passed using software Vulkan with one Mesa thread. It clicks creation from selected faces, Replace faces, Apply, Edit guide, region reassignment, Cancel, Make editable, Bake and Undo. It also drags the visible Z rotation-per-copy field and verifies the live recipe changes.
- Local compilation used one compiler job and the shared build lock. Full formatting/lint, test suites and native packaging are left to PR CI.

Actual app captures, visually inspected after the final run:

- [Keep faces](../../images/surface-keep.png): four captured panels over four logical quads; five unselected quads remain visible.
- [Replace faces](../../images/surface-replace.png): matched view, with only the four assigned quads suppressed.
- [Progressive rotation](../../images/progressive-rotation.png): nine fins, Z spacing 0.15 and Z rotation 20° per copy; the controls are visible beside the result.

Reproduce with `CLAY_GPU_SOFTWARE=1 LP_NUM_THREADS=1 CLAY_GUIDE_PREVIEW=/tmp/clay-surface-review cargo test --locked --lib surface_array_controls_offscreen_review -- --ignored --test-threads=1`, using the repository's local build lock and compiler-job limits. The capture is also included in the documentation screenshot script and headless CI.

Topology policy: vertex positions remain live; connectivity or subdivision changes suspend the region and reveal the original guide. Explicit reselection updates the fingerprint and clears per-copy overrides. No automatic reassignment or welding occurs. Native project version 7 stores the new recipe and continues reading versions 1–6.
