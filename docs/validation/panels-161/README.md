# Polygon-group panels — issue #161

This review covers local perimeter reconstruction, closed shells and the complete preview/commit workflow. Input extraction alone is not treated as a completed panel.

## Geometry and interaction evidence

The focused panel regressions cover curved adjacent groups (including a closed sphere), holes, disconnected islands, concave corners, narrow strips, excessive widths, crossings, hidden/masked inputs, stale UVs, symmetry, combined mesh capacity, cancellation and folded normal offsets. They compare dimensions across loop counts and flat/rounded bevel profiles, preserve retained front UV corners and vertex attributes, and check manifold closure, winding and smooth subdivision. A 20,000-triangle source cancellation completed in under one second in the local debug run; this is a measured example, not a universal timing guarantee.

Controller regressions cover the initial region highlight, mouse and classified native-pen group selection, explicit Preview/Apply/Cancel, navigation without sculpting, stale settings and scene changes, source retention, one labelled Undo/Redo step, and save/reopen. A seam-only source regression checks that unselected source UV seam indices cannot leak into the applied panel.

The manual GPU review uses the real window and viewport with software Vulkan. It clicks the controls, compares the committed mesh with the private preview, and exercises Undo/Redo. The separate `panels_controls_offscreen_review` checks accessible Apply/Cancel at 800 × 560 and is included in GPU CI. The larger image gallery remains a manual review tool.

## Visual examples

| Example | Inspection |
| --- | --- |
| [Adjacent, shaded](panels-adjacent-shaded.png) / [wireframe](panels-adjacent-wire.png) | Curved surfaces, gap, welded surface strips and bevels. Gold is the original source perimeter. |
| [Jagged source](panels-jagged-initial.png) / [cleaned shading](panels-jagged-shaded.png) / [cleaned wireframe](panels-jagged-wire.png) | Mixed triangle/quad source; shallow zigzags are smoothed while right-angle turns remain pinned. Interior connectivity remains visible. |
| [Hole](panels-hole-wire.png) | Independent inner and outer cycles, with continuous quad strips around both. |
| [Concave corner](panels-concave-wire.png) | Border flow through a re-entrant corner. |
| [Narrow strip](panels-narrow-wire.png) | Small surviving interior with independent front/back bevel and surface support. Excessive width is a regression-tested error. |
| [Disconnected islands](panels-islands-wire.png) | Both islands belong to one editable panel submodel and remain independently closed. |
| [One loop, flat profile](panels-profile-flat-1.png) / [four loops, rounded profile](panels-profile-round-4.png) | Same source and dimensions, different border resolution and bevel shape. |
| [Smooth subdivision, shaded](panels-subdivided-shaded.png) / [wireframe](panels-subdivided-wire.png) | The applied panel under one Catmull–Clark subdivision step. |
| [Short viewport](panels-small-window.png) | Apply/Cancel stay above the scroll area. |

The reconstruction is deliberately local: the original perimeter vertices become an inset ring, their incident faces form the inner transition, and welded quad strips fill the border. It preserves triangles and any irregular valence already present farther inside the panel. A rectangular all-quad shell has eight valence-three poles at the inset cap corners; the support strips separate those corner fans from the bevel. It does not promise a new all-quad interior for a mixed source. Smooth subdivision rounds uncreased corners; use the border spacing and existing crease controls to suit the desired result.

Supported input and failure behavior are described in the [modelling guide](../../guide/modeling.md#generate-panels-from-polygon-groups). Branching/touching outlines, fully closed groups without a perimeter, invalid UV layouts and geometry that cannot accommodate the requested offsets are rejected. The source stays recoverable. Intersection checks cover panel self-intersections, panel-to-panel crossings and nearby **unselected faces of the same source**; they do not evaluate other independent scene submodels.

## Reproduce

Observe `AGENTS.md` memory checks and the shared build lock first. Use one compiler job when available RAM is below 2 GiB or memory pressure is present.

```sh
flock -n /home/mark/.cache/codex-build.lock cargo test -j 1 --lib panels -- --nocapture --test-threads=1
flock -n /home/mark/.cache/codex-build.lock cargo test -j 1 --lib sculpt::thickness::tests -- --test-threads=1
flock -n /home/mark/.cache/codex-build.lock env CLAY_GPU_SOFTWARE=1 WGPU_BACKEND=vulkan LP_NUM_THREADS=1 CLAY_GUIDE_PREVIEW=/tmp/panels-review cargo test -j 1 --lib panels_offscreen_review -- --ignored --nocapture --test-threads=1
```

The final focused run passed 22 checks selected by `panels`, all five Thickness tests and all seven existing projection/relaxation controller tests. Full formatting, lint, the full suite and other configured GPU regressions remain CI work. Local checks do not replace passing CI on the reviewed commit. Issue #161 remains **In progress** pending review and merge.

## Painted-mask testing follow-up

The direct mask workflow creates a new output vertex group and closed panel in a single Apply; it leaves the source mask and groups unchanged. Nine focused controller regressions cover it, including threshold selection, Cancel, stale settings, source retention, Undo/Redo and reopening. A geometry-only fixture retains the reported 174-vertex painted spherical boundary. The former fixed-width defaults exceed its first interior row; mesh-spacing defaults pass the existing winding, manifold and intersection checks. **Fit border to mesh** provides the same conservative reduction for explicitly chosen widths; oversized requests are still rejected.

The full saved diagnostic stage before grouping was also checked with the real Vulkan viewport. [Shaded output](panels-painted-mask-shaded.png), [wireframe](panels-painted-mask-wire.png) and [changed thickness](panels-painted-mask-stale.png) show the generated panel and the retained view while settings are stale. Parameter changes disable Apply and retain the previous view until Preview panels updates it; the old view is ignored after source changes. These screenshots are a visual review, not an automatic pixel comparison. The original diagnostic bundle is not included in this change.

To repeat the manual check, run `panels_mask_and_painted_boundary_offscreen_review` with the same GPU environment above. It uses the small regression fixture by default; optionally set `CLAY_PANEL_REPRO` to a saved project with a painted mask to review that complete source.
