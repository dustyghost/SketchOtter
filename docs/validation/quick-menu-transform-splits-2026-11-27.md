# Quick menus, modifier taps, Transform spaces and surface splits

Stories: #189, #208, #197, #200, #201. One implementation batch; stories remain
In progress until review and merge.

## Focused checks

`src/app/workflow_tests.rs` runs in both the default and focused CPU harnesses.
It exercises actual egui button/pointer input, not only command implementations.
The final local run passed all 16 checks in 46.84 seconds including compilation,
with peak process-tree RSS 1,693,496 KiB (about 1.62 GiB).

| Story | Checks |
| --- | --- |
| #189 | Pin/unpin mouse clicks and focused Space/Enter activation across all six shared menu contexts at 1000×800 and 640×480; labels, 32px hit targets, visible state, viewport navigation, Escape and unchanged geometry/history |
| #200 | Frontmost overlapping model activation across Sculpt, Modeling, Mask, Slice and Transform; ordinary and same-frame taps; empty, hidden and already-active targets; unchanged source vertices |
| #208 | Mask soft weights and face visibility, single Undo/Redo, click tolerance, same-frame taps/out-and-back drags, region drags, menu/text safeguards, Escape/pointer/focus cancellation and remapped selection holds |
| #197 | Actual World/Screen controls; view-aligned translation, rotation, depth and scaling after orbit/roll in both projections; custom pivot preserved; geometry Undo/Redo |
| #201 | Actual face selection through the viewport and command dispatch; selected-region and all-island splits; masks, materials, groups/weights, UVs, visibility, creases and quads; one Undo/Redo, JSON save/reopen, empty/single-part/model-limit rejection |

Ctrl navigation checks include same-frame press/move/release and remapped pan.
Remapped pan is exercised over empty space, preserving the existing surface
gesture priority. Pointer-button events are the shared mouse/pen-tip route;
physical pen hardware/pressure has not been validated in this headless run.
Existing context-menu lifecycle and shared face-subset shading/texture
regressions remain in the default suite for CI.

Bounded local command:

```sh
flock -n /home/mark/.cache/codex-build.lock \
  env CLAY_PROBE_LIMIT_MIB=2300 \
  python3 docs/validation/testing-batch-2026-11-22/memory-probe.py \
  /tmp/opencode/workflow-tests \
  env CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_INCREMENTAL=false \
  cargo test --locked -j 1 --lib --features focused-retopology \
  workflow_ -- --test-threads=1
```

## Optional visual review

Set `CLAY_WORKFLOW_CAPTURES` to an existing output directory to render the
compact Modeling menu, pinned Transform menus at normal/compact sizes and
screen-space handles after camera orbit/roll in both projections. On software
Vulkan, also set `CLAY_GPU_SOFTWARE=1 LP_NUM_THREADS=2`. Without the capture
variable these tests do no screenshot rendering.

The compact capture exposed viewport launchers covering the pin. The chooser
now stays above those launchers; actual compact pin clicks are regression-tested.
`docs/images/quick-menu-pin.png` illustrates the corrected pinned control.

The full suite, formatting/lint and configured GPU regressions are left to PR CI.
