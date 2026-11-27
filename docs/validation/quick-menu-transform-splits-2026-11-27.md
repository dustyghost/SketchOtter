# Quick menus, modifier taps, Transform spaces and surface splits

Stories: #189, #208, #197, #200, #201. One implementation batch; stories remain
In progress until review and merge.

## Focused checks

`src/app/workflow_tests.rs` runs in both the default and focused CPU harnesses.
It exercises actual egui button/pointer input, not only command implementations.
The masking follow-up local run passed all 19 checks in 48.69 seconds including
compilation, with peak process-tree RSS 1,676,756 KiB (about 1.60 GiB).

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

## Release-test follow-up

- New or missing selection preferences now start in Mask / Surface brush.
  Explicit saved selection modes and shapes are not migrated or overwritten.
- The new no-Z regression paints with Ctrl+Shift, including over a previously
  selected visibility tool, preserves camera/vertices/face visibility, and
  restores the persistent tool on hold release.
- Temporary Mask takes priority over persistent visibility selection. Legacy
  Rectangle/Lasso tools retain their actual shape rather than being replaced by
  the visibility preset; persistent configured visibility still uses its preset.
- Ctrl edge removal retains priority over Ctrl model taps. Ctrl navigation tests
  allow the intentional deferred press and still require navigation on movement.
- Transform operations remain before coordinate-space choices so opening E and
  pressing Enter still activates Move rather than merely changing axes.

An additional 42-check targeted run passed: 18 workflow checks before the focused
edge-removal case was added, 12 menu lifecycle checks, 11 masking checks, and the
previously failing Vulkan lasso preview/commit regression. Mask and hide captures
were visually reviewed. For this run only, the existing menu/masking/GPU-lasso
modules were temporarily included in the focused harness; their normal partition
gates were restored before the final 19-check run. Compiling the full default
harness exceeded the local memory-pressure guard; its full execution stays in CI.

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

## Selection-context split follow-up

All 22 focused workflow checks passed with optional GPU captures in 69.41 seconds
including compilation; peak process-tree RSS was 1,722,416 KiB. The Mask,
Visibility and component Z captures were visually reviewed: each displays its
correct split control inside the menu.

- Z offers exactly one region split: **Split masked**, **Split visible** or
  **Split selected**, according to its actual menu context. It no longer mixes
  in the disconnected-island operation.
- Submodels has a context-sensitive region split and a distinct **Split to
  parts** action. The existing disconnected-island action identity is retained
  for saved bindings and custom controls.
- `workflow_z_split_matches_selection_context` checks labels, action identity,
  enabled state, rendered controls, unchanged geometry/history on opening, and
  pointer/keyboard activation of the Mask/Visibility confirmation. Optional captures are `split-Mask.png`,
  `split-Visibility.png` and `split-Components.png`.
- `workflow_submodels_has_distinct_split_to_parts_control` checks the distinct
  rendered region/parts controls and clicks **Split to parts** in the actual
  Submodels panel, then verifies one Undo/Redo and JSON save/reopen.
- `workflow_split_visible_preserves_hidden_remainder_and_is_atomic` checks that
  explicitly visible faces move to the new submodel, hidden faces remain hidden
  in the original, masks/positions survive, and Undo/Redo plus JSON save/reopen
  restore the exact result. An all-visible mesh is rejected without history.

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
