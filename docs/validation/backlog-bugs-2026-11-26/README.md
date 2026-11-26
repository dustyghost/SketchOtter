# Ready bug batch, 26 November 2026

Base: `4f5f336`. Issues #199, #203, #205 and #206 were the four open
bug-labelled stories in SketchOtter Development (all in Ready, none literally
in Backlog). They moved to In progress together. Private diagnostic projects
remain local; no artwork or project payload is included here.

## Reproductions and changes

- **#199:** the drag handler used only vertical pointer travel, regardless of
  the surface rail. Horizontal travel therefore produced no slide; an
  unspecified numeric axis could also choose an unexpected side. Dragging now
  projects onto the actual grabbed edge's surface rail and latches that choice
  through reversal. Numeric axis-based sliding remains available. The interaction
  regression checks both horizontal directions, reversal, private previews,
  unchanged topology, preview/commit agreement and exact Undo/Redo.
- **#203:** independent source-only contact plans cannot see two mirrored caps
  meeting halfway, because neither midpoint cap exists yet. Reciprocal matching
  symmetry caps now get a shared midpoint plan. The existing extrusion/join
  implementation constructs and validates the result; arbitrary mismatched
  caps are not force-welded. The geometry regression covers both source sides,
  all three mirror axes, approach/snap/overshoot, a connected closed result,
  exact symmetry and mask retention.
- **#205:** soft-footprint seam joining applied to Standard only. Clay now
  blends direction separately from depth and uses its own sampled-plane depth
  in the existing soft-footprint join. Copies do not add their strengths.
  The profile regression checks two mesh densities and two pressure values.
  Drag stamps and non-Soft alphas do not get the soft-footprint extension.
- **#206:** Clay's triangle-centroid plane sampling was not invariant under
  reflection of logical quads with unmirrored rendering diagonals. Plane
  sampling now uses logical face centres and area vectors. The regression
  checks reflected planes and repeated mirrored strokes without requiring
  mirrored triangle indices or forcibly symmetrizing existing geometry.

## Retained export evidence (#206)

The six retained stages have identical triangle and quad indices. Pairing the
first stage's vertices with their X reflections gives:

| Stage | Maximum reflected-position mismatch | RMS |
| --- | ---: | ---: |
| 0 | 0.000000124 | 0.000000032 |
| 1 | 0.000939384 | 0.000091360 |
| 2 | 0.000998885 | 0.000121637 |
| 3 | 0.001817459 | 0.000257762 |
| 4 | 0.001817459 | 0.000261893 |
| 5 | 0.002203179 | 0.000286273 |

The first retained stroke already introduces drift. Maximum per-stroke
displacement mismatches are 0.000939384, 0.000598207, 0.001500337,
0.001213492 and 0.001678298. The recorded mirror plane is X=0; symmetry is on
and Dynamic Mesh/Adaptive are off.

A controlled plane-sampling comparison on stage 0 used centres at the normalized
vectors `(±0.2, 0.23, 1)` with radius 0.36, the same visibility/mask conditions
and area/taper weighting:

| Sampling | Reflected centre mismatch | Reflected normal mismatch |
| --- | ---: | ---: |
| Triangle centroids | 0.0000136824 | 0.0000157762 |
| Logical face centres | 0.00000000369 | 0.0000000133 |

These double-precision diagnostic calculations isolate the quadrature bias;
the Rust regression exercises the actual stamp implementation. Complete
pointer/pressure samples were not retained, so this is **not** an exact replay
of the five original pen strokes, nor a claim to repair existing drift.

## Validation

Run focused checks using the shared build lock, one compiler and one test
thread, with `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_PROFILE_TEST_INCREMENTAL=false`. The existing memory probe terminates
only its own process group on excessive RSS, low available memory or sustained
memory pressure. An initial 900 MiB production-check probe stopped at its
budget; the maintainer then freed RAM for the real test harness.

The new ignored `clay_symmetry_seam_offscreen_review` captures on/near/across-
seam cases with the real renderer and checks one-step Undo/Redo. Set
`CLAY_BUILDUP_PREVIEW` to a local output prefix; optional
`CLAY_SYMMETRY_SOURCE` selects a local source project instead of the synthetic
sphere. These are new deterministic dabs, not replayed original input.

Completed local checks:

- Final current-source combined focused run: **27 passed, zero failures**, four
  manual captures ignored. This includes approach-before-contact and
  incompatible mirrored-cap coverage added during final review.
- Clay-focused suite: **14 passed**, two manual captures ignored.
- Modeling drag/Fuse checks: **3 passed**.
- Neighboring Slide/Fuse, Flatten and soft-overlap checks: **14 passed**, two
  manual captures ignored. This includes tapered surface rails from oblique
  and reversed views with symmetry.
- Synthetic on/near/across-seam capture: **passed**, including closed geometry
  and exact Undo/Redo. Visually inspected all three 900 × 700 shaded images:
  continuous buildup, no obvious split central lobes or seam groove. The
  symmetry guide is visible, so the numerical profile regression remains
  important for the narrow centre line.
- Retained stage-0 project on/near/across-seam capture: **passed**, with the
  same geometry/history assertions. All three shaded images were visually
  inspected; the new strokes likewise show continuous buildup without an
  obvious split centre. This does not replay or repair the retained edits.

Rendering used software Vulkan, llvmpipe (LLVM 23.1.5), Mesa
26.3.1-arch1.3, with two worker threads. Local captures use the prefix
`/tmp/opencode/backlog-clay-synthetic`; no project artwork is committed.
The retained-project captures use `/tmp/opencode/backlog-clay-retained`.
Focused test compilation peaked at approximately 2.52 GiB process-tree RSS,
within the 3,600 MiB probe limit.

Full formatting/lint/default-suite/native-solver and configured GPU checks
belong to PR CI. Physical pen-tablet review is still required; synthetic
pressure checks do not establish device behaviour.

## Follow-up release testing: remaining drift

The maintainer supplied a second local diagnostic export from release
`b83c4c3994c7`. This shows that the initial fix is **incomplete**; the passing
Clay-only synthetic sequence did not cover subtractive Standard followed by
Smooth. All 15 stages retain identical topology, X symmetry at centre zero,
and disabled Dynamic Mesh/Adaptive.

| Stage | Outgoing/incoming edit | Maximum reflected-position mismatch |
| --- | --- | ---: |
| 0 | Starting geometry | 0.000000124 |
| 1 | Standard, edit 3 | 0.000000130 |
| 2 | Standard, edit 4 | 0.000000211 |
| 3 | Subtractive Standard, edit 5 | 0.007599561 |
| 5 | Subtractive Standard, edit 7 | 0.007595376 |
| 6 | Smooth, edit 8 | 0.053243777 |
| 14 | Smooth, edit 16 | 0.077707566 |

The first substantial drift is edit 5, before any retained Clay stroke.
Edit 8 adds a maximum reflected-displacement mismatch of 0.049794569.
Full pointer samples are again unavailable. The diagnostic files remain
local and unchanged; these measurements compare vertex pairs established
from the initial geometry, not nearest neighbours after deformation.

Two minimal regressions reproduced independent failures before the follow-up:
the triangle safety limiter gave reflected corners of a single symmetric
quad different reductions, and a single mirrored Smooth dab on the synthetic
sphere introduced 0.002666726 maximum reflected mismatch.

The follow-up checks both quad triangulations and reduces all four corners
of a failed logical face together. Smooth uses logical-face normals and
sheet classification, with mass and cotangent contributions averaged over
both diagonals. Triangle-only safety behaviour retains its frozen-reference
comparison. No mirror-pair snapping or repair of existing asymmetry is added.
Both minimal regressions pass with these changes.

The combined follow-up focused run passed **29 tests**, zero failures, with
three manual captures ignored. It covers the safety limiter, Smooth,
Clay, overlap blending and Clay application/history tests. The new 96-dab
synthetic sequence varies pressure and applies subtractive Standard, Smooth,
then Clay, checking unchanged topology and exact one-stroke Undo/Redo at each
step. Maximum reflected mismatches were respectively 0.0000001341,
0.0000001229 and 0.0000001966.

The ignored `mirrored_brush_sequence_offscreen_review` accepts
`SYMMETRY_REVIEW_SOURCE` and `SYMMETRY_REVIEW_PREVIEW`, runs that same
sequence on local starting geometry, and saves front-view before/after images
with the symmetry guide hidden. Use an initially symmetric source such as
the new bundle's stage 0: this check does not repair already-drifted stages.

That retained stage-0 check passed with the same three maximum errors as the
synthetic sequence, unchanged topology, closed geometry and exact Undo/Redo.
Both 900 × 700 front-view captures were visually inspected: no obvious
one-sided distortion; lighting itself is not reflection-symmetric.
Captures remain local at
`/tmp/opencode/symmetry-followup-retained-{before,after}.png`.
Rendering used software Vulkan/llvmpipe with two worker threads.
This is a controlled new sequence, **not** the unavailable original pen replay.

The initial `b83c4c3` PR CI run passed formatting, the default/focused suites,
lint, native solver checks and GPU regressions. That result does **not**
validate the follow-up changes, which require a fresh CI run.
