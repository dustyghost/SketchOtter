# Partial-chain bridges and face-region edits — 15 November 2026

Tickets: [#156](https://github.com/dustyghost/clay-form/issues/156) and [#157](https://github.com/dustyghost/clay-form/issues/157). This batch is based on the manual topology previews in open PR #158. No existing PR was merged.

Eleven focused `topology_regions_` geometry/controller tests passed. They cover bridge segment counts 1/2/16, source preservation and interpolation, exact mirror correspondence, unequal counts, closed loops, stale edges, hidden geometry, obstructions and folded strips; complete-shell reversal including triangle-corner UV identity and a reversible quad flip; planar interior removal, unrelated materials/UVs/creases/hard edges, concave tessellation, disconnected regions and hole rejection; protection of internal quad UV seams, mapped/unmapped boundaries, face groups, curvature and painted detail; private preview, Cancel, one-step Undo/Redo, save/reopen, changed selection/settings, failed-preview cleanup and legacy preset defaults.

Sixteen existing focused regressions also passed: ten manual-construction checks, two topology-operation checks and four modeling/construction/transform preset checks.

The new operations reuse the existing revision-bound manual construction preview and brush-change decision. Bridge segment changes invalidate an earlier preview. Face-region overlays draw the proposed edges instead of source diagonals that dissolution removes; these edges are cached independently of the camera.

Local builds used one compiler job with debug information and incremental compilation disabled, reusing the shared test target under the global build lock. Full formatting, lint, test suites and platform builds run in PR CI.

Supported limits are deliberate: equal-count partial chains with fixed opposite-directed correspondence and 1–16 segments; complete shells for orientation; flat, single-outline dissolution with uniform vertex attributes and affine UV/UV-mask interpolation. There are no stored n-gons, arbitrary face-region bridges, unequal-chain resampling or solid fusion. The [guide](../../guide/modeling.md#bridge-two-partial-boundary-chains) includes the workflow and rejection guidance. Both stories remain In progress pending review and merge.

`topology_regions_offscreen_review` passed on Vulkan llvmpipe (LLVM 23.1.4, Mesa 26.2.6) with one software-rendering thread. At 1100×1000 it selects the two open borders and all four fan faces using real pointer events, invokes the sidebar actions, cancels/applies a three-segment bridge, uses Undo/Redo, dissolves the fan into one quad, then flips the complete sheet and inspects it from the opposite side. The walkthrough is included in GPU CI and screenshot regeneration.

The captured bridge, selected face fan, dissolved preview and flipped back view were inspected. The final dissolve preview shows only its outer quad outline; the source selection's deleted diagonals no longer obscure the proposed result. Apply/Cancel and the viewport hint identify the pending operation. Selected captures are in the user guide. Mouse and pen share the exercised primary-pointer path; physical tablet feel was not tested.
