# Experimental integer-grid parameterization

Experimental alternative for [#191](https://github.com/dustyghost/clay-form/issues/191).
The existing Surface field method remains the default. This is a separate native
Rust parameterization path, not another name for its lattice repair.

## Research and implementation boundary

- [Mixed-Integer Quadrangulation, 2009](https://cgg.unibe.ch/publications/miq/bommes_zimmer_2009_siggraph_011.pdf): field-aligned gradient energy, cut charts, quarter-turn seam transitions, integer translations and singularity coordinates, nearest-first rounding with continuous updates.
- [Integer-Grid Maps for Reliable Quad Meshing, 2013](https://www.graphics.rwth-aachen.de/publication/03197/): positive map orientation and conservative linear orientation constraints. This experiment uses the general interior-barycentric-point construction from §3.1, with centroid sectors and normalized half-spaces; it does not implement the paper's Fermat-point/virtual-split optimization.

The initial cross field and sizing field reuse our existing native field code.
This is not a new implementation of the 2009 cross-field optimization. The
mapping operates on the original surface domain, subject to the shared bounded
long-edge refinement. It does not use the current solver's integer lattice,
network-flow regularity correction, coarse SAT repair or lattice extraction.

The experiment uses bounded greedy rounding, not the 2013 paper's global
branch-and-cut optimizer. Intrinsic complexity reduction and the singularity
separator constraints from that paper are not implemented. No global optimality or
unconditional success guarantee is claimed. Positive orientation, seam residuals,
integrality, extracted topology and surface fit must pass before returning a
layout; reaching a time or size limit is a failure.
Singularity detection also checks cone angles in the positive relaxed map, so a
full-turn pole cannot disappear through quarter-turn arithmetic modulo four.

The extractor joins fragments only across adjacent source triangles, carrying
quarter-turn seam rotations. UV overlap elsewhere does not weld nearby sheets.
It rejects incomplete cells, ambiguous corners and collapsed quads. Before
assembling output quads, it checks that every source singularity survives as a
distinct grid vertex. This provenance check does not compare global UV positions
of unrelated charts and does not implement the paper’s separator optimization.
The existing surface fitting and immutable-source validation remain the final checks.

Clarabel 0.11.1 supplies continuous sparse quadratic relaxations. Its upstream
[source and Apache-2.0 license](https://github.com/oxfordcontrol/Clarabel.rs/tree/v0.11.1)
were reviewed for this experiment. No optional BLAS, C++, commercial solver or
SDP backend is selected. The locked dependency inventory was regenerated and
`scripts/check-notices.py` verified all 292 distribution notices and input hashes.
The new AMD ordering dependency’s BSD-3-Clause attribution is included. Native
platform package builds remain part of PR CI.

Rounding starts with warm conjugate gradients. After 32 unsuccessful iterations,
it tries a sparse LDL factorization of the eliminated system using Clarabel's
existing QDLDL and AMD implementations. Later updates reuse that factor as an
SPD preconditioner, restricted to the remaining free coordinates. The current
system's independently recomputed residual, seam/fixed equations, inequalities
and map orientation still determine acceptance. A change that frees a coordinate
the cached factor cannot represent invalidates it. New orientation conditions
first update this warm system before falling back to the full quadratic solver.

Before factor allocation, an exact elimination-tree traversal limits strict-lower
entries to four million and numeric multiply-subtract work to 500 million. The
input remains limited to 100,000 free variables and four million matrix entries.
Ordering and numeric factorization have no cancellation callback; budget checks
surround them, and their input/work limits bound this uninterruptible section.
Failed or oversized factors retain the bounded iterative/full-relaxation paths.
Numerical factor regularization is disabled; a nonpositive factor is rejected.
The overall 60-second deadline and geometry tolerances are unchanged.

This follows the adaptive iterative/direct strategy in MIQ §2.1, without its
local Gauss–Seidel queue. The pinned upstream
[QuadriFlow optimizer](https://github.com/hjwdzh/QuadriFlow/blob/810b7a0967c35b0dc85b4464e3835e26a756c967/src/optimizer.cpp)
also uses sparse factorization for its continuous position systems; that is a
comparison of numerical methods, not algorithm parity for this parameterization.

## Current limits

The alternative handles closed surfaces. The app clears and disables Layout
symmetry and Protect sharp bends for this method; programmatic requests using
those unsupported options are explicitly rejected. Signed density and direction guides use
the shared sizing/field inputs. Nonuniform masks and materials use the existing
attribute transfer. No fallback to Surface field is hidden behind the selector.
The same private preview, explicit Apply and source-preserving attribute transfer
are used by the desktop adapter.

## Controlled measurements, 22 November 2026

Input: locally retained `retopo-slow`, source SHA-256
`c7e8ed7b66b2a4f7a2cef4aadfd24bd8686e6234d267a843d47769ebd00f664c`.
17,917 vertices, 35,830 triangles, target 3,583 quads, no symmetry, guides or
density paint, sharp protection off. The private geometry is not bundled here.

| Path | Result | Elapsed solver time | Peak process RSS |
| --- | --- | ---: | ---: |
| Existing regularized Surface field | 3,521 quads, passed validation | 102.832 s | 113,124 KiB |
| Initial direct MIQ/IGM prototype, full QP after each rounding | Rejected at 60-second internal budget; no output | 60.755 s | 146,096 KiB |
| Warm iterative rounding without carrying active orientation constraints | Rejected at the same budget; no output | 60.665 s | 149,980 KiB |
| Active orientation constraints, seam/singularity integers interleaved | Infeasible rounding with eight integer coordinates remaining; no output | 47.420 s | 145,440 KiB |
| Active orientation constraints, singularity coordinates first | 3,411 quads, passed validation | 58.203 s | 145,328 KiB |
| Source-singularity guard, same ordering | Identical 3,411-quad output, passed validation | 55.005 s | 145,412 KiB |
| Cone-angle guard including full-turn poles | Identical 3,411-quad output, passed validation | 55.344 s | 145,280 KiB |

The baseline spent 95.688 seconds on its first preparation/repair attempt; its
existing alternative preparation then succeeded in 7.144 seconds. Repeated full
quadratic solves were the initial alternative's measured bottleneck. The first
implementation eliminated seam/fixed equations and updated the quadratic energy
with preconditioned conjugate gradients, carrying active orientation constraints.
A failed update falls back to a full constrained relaxation. Carrying those
constraints is a bounded heuristic, not a globally optimal active-set method.

Fixing seam translations before singularity positions can force half-integer
poles through quarter-turn cycles. The final ordering fixes singularity
coordinates first. It completes this source: 3,413 vertices, 3,411 quads and a
maximum sampled surface error of 0.0284511 (baseline: 0.0426158). There are 28,646
field samples, 24 singularity coordinate variables and 1,220 seam translation
variables. Rounding finished at 57.419 seconds; extraction, fitting and original
validation then took about 0.8 seconds. The unchanged 60-second budget is close
to this measurement, so completion under different system load is not promised.
These are single runs on the same source, not a general performance claim.
The final guarded rerun produced a byte-identical OBJ, so the quality measurements
below apply to all successful experimental runs. The final cone-angle guard run's
total process-group peak was 149,400 KiB; the memory guard reported no pressure
or limit stop.

The existing independent `measure-layout.py` reports quad edge-length ratios
(longest edge divided by shortest edge): p95 1.35949 and maximum 3.78401 for the
integer-grid result, versus p95 1.26508 and maximum 1.67530 for Surface field.
Neither output has a strict closed opposite-edge cycle under that tool's metric.
These metrics show a quality trade-off, not a uniformly better replacement.
The front and rear wireframe comparisons retain the silhouette and show different pole
placement and edge flow. Local view artifacts are retained under
`target/retopo-slow-review/`; they are not published with the private geometry.
The diagnostic renderer uses OBJ fan diagonals for display, while each solver
independently chose valid render diagonals before returning its result.

## Timeout investigation, 23 November 2026

The native comparison reproduced the reported release-application solve-limit
failure at target 3,583. With the app still open, the original executable completed once
in 56.69 seconds (including process startup), while an instrumented rerun failed
at 60.062 solver seconds. This is the same immutable source/settings above, not a
larger target. The map has 57,288 triangles despite its 3,583-quad output target.
Individual rounding updates took 800–1,200 diagonally preconditioned CG iterations;
one newly inverted triangle also restarted several full constrained relaxations.

| Change, same source/settings | Solver time | Peak process RSS |
| --- | ---: | ---: |
| Sparse LDL after 32 CG iterations, without factor reuse | 45.330 s | 185,668 KiB |
| Reuse factor as preconditioner for successive updates | 29.609 s | 192,116 KiB |
| Also update new orientation conditions in the warm system | 17.344 s | 146,032 KiB |
| Final finite-value guards, repeat 1 | 16.112 s | 145,552 KiB |
| Final finite-value guards, repeat 2 | 16.173 s | 145,328 KiB |

The final path still produces 3,413 vertices and 3,411 quads. Quad connectivity is
identical to the previous successful result; maximum corresponding-vertex change
is 1.92e-6. Maximum sampled source error is 0.02845114; edge-ratio p95 is 1.3594844,
maximum 3.7840109, and strict closed opposite-edge cycles remain zero. Front/rear
wireframes show the same silhouette and edge flow. No tolerance, target, sample
count or overall time limit was relaxed. Timing depends on system load and this
single saved case does not establish general solver performance.
The two final repeats and the preceding 17.344-second run produced byte-identical
OBJs (`f518720f3013cda0825249c10833d1746d9cce1542ae6f5247e7ce17e68768d2`).
Their process-group peaks were 149,456–150,056 KiB, with no memory-guard stop.
Logs, outputs, and visual comparisons are retained locally under
`target/retopo-slow-review/`, outside the source bundle.

The numerical regression compares a 1,024-variable anisotropic grid against the
separate constrained solver with seams, fixed coordinates and an active bound.
It requires successful factor reuse after another rounding and invalidation when
a previously eliminated coordinate becomes free. A sparse-star case is rejected
by symbolic work counting before dense factor allocation. Nonpositive/nonfinite
factors and cancellation are also checked. Audit logs now separate elimination,
assembly, solve iterations and factor fill/work.

## Saved path failures, 23 November 2026

The local `retopo-with-paths` export contains the same source mesh plus two
closed surface rings. The current stage SHA-256 is
`dbf5e16a31474fb93f5ee5db3e4d35c41d850a03124fa6767f92c9db4e845d17`.
The app's actual surface-path sampler produces 51 and 53 guide samples, radius
0.0941548, strength 1. The user confirmed target 3,583 and otherwise unchanged
defaults. The original export stores path geometry but omits selected guide IDs
and current retopology controls; diagnostics now retain both current controls and
the latest attempted settings, including failed attempts without a history edit.

The following measurements record the first path fixes at `2484739`. Later
trimmed-source changes and their final comparisons are recorded below. Two
separate defects were isolated with the unchanged mesh and sampled guides:

- With both rings, a single-CPU run reproduced the reported generic layout error
  in 23.464 seconds. Clarabel stopped a relaxation with `AlmostSolved`; its primal
  and dual residuals were 2.32e-14 and 1.13e-13. The previous status check rejected
  the candidate without examining its independent residuals. That first fix
  required a bounded linear cleanup before continuing. The trimmed cases below
  replace that cleanup with an independent primal/dual check of the intermediate
  relaxation. Infeasible/numerical-error states
  remain failures, and an actual `MaxTime` reports the solve limit. The overall
  deadline, per-relaxation time limit and geometry acceptance gates are unchanged.
  This uses Clarabel's documented
  [reduced-accuracy solution status](https://github.com/oxfordcontrol/Clarabel.rs/blob/v0.11.1/src/solver/core/solver.rs)
  only as a starting point, not as proof of a valid map.
- Each ring alone reached a positive integral map, then failed with an incomplete
  cell. A dropped corner fragment had area about 1.4e-11; the remaining fragments
  covered 0.9999999999859 of the cell. The old 1e-10 area cutoff discarded the only
  owner of that corner. Clipping now measures a cell-local triangle fan and
  retains every positive fragment. Adjacency, corner agreement, singularity,
  manifold and surface-fit checks still apply. A five-vertex planar regression
  produces four complete unit quads, including after translating its chart.

| Guide selection | Previous result | Revised result | Revised solve time |
| --- | --- | --- | ---: |
| First ring only | Incomplete cell, 16.573 s | 3,447 validated quads | 15.696 s |
| Second ring only | Incomplete cell, 32.682 s | 3,433 validated quads | 30.345 s |
| Both rings | Load-dependent status rejection above; successful comparison without CPU affinity at 41.375 s | 3,404 validated quads | 36.433 s, one CPU |
| Both rings, forced near-converged stop | Original status gate rejects `AlmostSolved` | 3,419 validated quads after polishing | 43.360 s, one CPU |

The last row uses the audit-only `CLAY_MIQ_RELAXATION_ITERATIONS=11` reduction,
which reproduces the same status and residuals without depending on CPU load.
It does not raise any limit. This exercised actual polishing, integer rounding,
extraction and final immutable-source checks on the saved case. Peak process RSS
was 223,868 KiB, with maximum sampled source error 0.03495676. Polishing can take
a different valid greedy rounding route; it does not promise identical quads to
a fully converged relaxation. The current focused numerical regression
checks the analytical optimum and its dual certificate, and rejects violated
seams, bounds, fixed integers, nonfinite/malformed vectors, wrong dual signs,
stationarity errors, excessive complementarity gaps and cancellation.

The successful two-ring output is byte-identical before/after the clipping change
(`97589bfd6bd92cad001a733c24ac78a9c58aff8e127adba0de4b23a4e8b1546d`).
Maximum sampled source errors are 0.03113493, 0.02818252 and 0.03380500,
respectively. The corresponding edge-ratio p95 values are 1.37033, 1.43244 and
1.38734; maxima are 4.13147, 3.77838 and 4.26157. These are measurements of this
input, not a guarantee of regular loops or uniform quad quality. Private input,
sample arrays, build/run logs, output hashes and review artifacts are retained
under `target/retopo-paths-review/` rather than included in source control.
The unguided control still produces the identical 3,411-quad OBJ hash reported
above, completing in 15.049 seconds in this run.

## Trimmed sources and unsupported controls, 23 November 2026

The reported `retopo-with-paths-flat` export was never written. The user kept the
detailed side after Trim and reprojected both rings; the exact unsaved cut and
post-trim target are unavailable. Reproject does not change the target, but
reopening Guided retopology resets it to one tenth of the source triangle count.
Controlled copies of the original saved stage therefore exercise both 3,583 and
the reopened defaults. They use the production capped `Mesh::slice` and
`SurfacePath::reproject` (distance 0.2), then the same production path sampler as
the app. Both rings remain selected at strength 1. No private source was changed.

- Keep `x >= -0.15`: 13,808 vertices / 27,612 triangles; default target 2,761.
  Source SHA-256 `724732cd02dfa309018bdcc61d91a65b3fdccabd19e4c2a95ca58cccbebd5f82`.
- Keep `z <= 0.15`: 12,086 vertices / 24,168 triangles; default target 2,416.
  Source SHA-256 `174eaf0417a03895032edca77c789e62d6fca6c59740ef797fed4b70205b01c6`.

Each copy has 117 guide samples. The second cut reproduced the generic layout
error at `2484739`; the first failed its numerical residual check. Investigation
isolated several numerical issues, rather than a missing path selection:

- Forced cleanup of a feasible interior-point iterate can create inconsistent
  equalities from feasible inequalities (the minimal regression is `x >= 1`
  and `x >= 2`). Near-converged relaxations now pass the unchanged primal checks
  plus independent stationarity, dual-sign and complementarity checks using the
  original energy and constraints. Stationarity uses the full 1e-9 tolerance
  and Clarabel's unscaled normalization; the gap uses its 5e-5 reduced threshold.
  This is an intermediate relaxation, not an accepted quad mesh. Infeasible,
  numerical-error and timeout statuses remain failures.
- A cap triangle with positive doubled map area `7.4918377e-10` was rejected by
  the old absolute `1e-9` cutoff, causing repeated full solves. The orientation
  filter now requires a positive determinant above a conservative floating-point
  error bound, following the first stage of
  [Shewchuk's orientation predicate](https://www.cs.cmu.edu/~quake/robust.html).
  Zero, negative, nonfinite, underflow-scale and numerically ambiguous signs
  remain failures; this does not implement an exact-arithmetic fallback.
- The existing IGM §3.1 centroid half-spaces are normalized so tiny reference
  triangles retain their positive margin above absolute solver tolerances. This
  preserves each half-space exactly. Regressions cover regular, obtuse and very
  thin triangles, including reflection and collapse rejection. A controlled
  Fermat-sector/virtual-split trial regressed the first cut and was not retained.
- Initial orientation repairs reuse the bounded warm solver. Linear iterations
  recompute the true residual before stopping, and a direct solve has at most
  three refinement corrections. Partial pivoting avoids dividing by tiny
  coefficients in thin-cap constraints. Residual acceptance is unchanged.
- An inconsistent tentative active set can use a small nonnegative dual QP.
  It considers at most 1,024 inequalities, activates at most 512, and adds
  violated omitted bounds over at most eight rounds. Every original inequality
  is checked before returning. Needed inverse columns are streamed and only
  their Schur products are cached (at most 4 MiB of numeric data). Successful
  dual active sets seed later warm updates; a full relaxation discards that cache. Analytical/oracle regressions cover
  redundant bounds, added bounds, seam equations, fixed coordinates, reuse,
  pre-allocation limits and cancellation.

The overall 60-second deadline, sample/target limits and factor fill/work caps
are unchanged. Full-source topology, positive map orientation, integral seams,
singularity separation, extraction, render diagonals and sampled surface fit
still determine whether any preview can be returned. A smaller target is not a
guarantee of a faster solve because it can change field singularities.

| Controlled input | Target | Final result | Solver time | Peak process RSS |
| --- | ---: | --- | ---: | ---: |
| Keep `x >= -0.15`, both rings | 3,583 | 3,497 validated quads | 28.884 s | 204,996 KiB |
| Same cut, reopened default | 2,761 | 2,594 validated quads | 33.148 s | 231,056 KiB |
| Keep `z <= 0.15`, both rings | 3,583 | Rejected: independent stationarity check; no output | 40.664 s | 224,584 KiB |
| Same cut, reopened default | 2,416 | Rejected at the unchanged solve limit; no output | 61.044 s | 223,092 KiB |

These are single-CPU runs using the same saved cuts and actual sampled guides.
The second cut remains an unresolved case; this change does not claim that all
capped cuts, or the user's unavailable cut, now work. At 3,583 its near-converged
stationarity residual was 4.162e-9, above the unchanged 1e-9 check. At 2,416 it
exhausted the deadline while repairing orientation. The UI now suggests a
*different* target rather than implying that a smaller target must solve faster.

For the two successful runs, maximum sampled surface errors are 0.02295007 and
0.02591817; edge-ratio p95 values are 1.39984 and 1.55914, maxima 3.05449 and
4.00576. Strict closed opposite-edge cycle counts are 18 and zero, respectively.
Their OBJ SHA-256 values are
`106b47d0b1a9490d617b8b1b951e04536f248f3ab4fa7a5dd07b047fa708e743` and
`13d1893b2e8d84c0152150f8e5039df8f6b1413d0b111b4a8855635839df004c`.
The original untrimmed two-ring control also succeeds: 3,404 quads in 19.220 s,
peak process RSS 197,908 KiB and maximum sampled surface error 0.03357428.
Its current OBJ SHA-256 is
`034254fa848dc64a9c51b3a2cba4f7b5c154cc1f01b4ba5ba39f771ea104d95e`.
The greedy route changes, so identical output is not promised. Front and cap
wireframes of both successful trims and the original guided comparison were
inspected: the silhouette and flat cap are retained, with visible differences in
edge flow and stretching near the groove/poles. These diagnostic displays use
fixed OBJ fan diagonals, not the solver's independently validated triangulation.
Eleven focused numerical/extraction tests and five end-to-end Integer grid
geometry, density, guide and cancellation tests pass on the final source.

Private sources, guide samples, numerical snapshots, helper source, exact
settings, build/run logs, hashes and review renders are retained under
`target/retopo-trim-review/`. These controlled cuts cannot certify the user's
unsaved cut; the updated diagnostic export retains the actual target and guide
selection for a future exact replay.

Selecting Integer grid now clears and disables Protect sharp bends (including
its angle control) and Layout symmetry. Reopening also keeps those unsupported
settings off. Switching back to Surface field re-enables their controls without
silently restoring them. Headless real-click coverage checks the state changes,
disabled clicks, reopening and unchanged document/history; the existing GPU
control capture retains the disabled appearance for review.

## Focused coverage

Focused checks cover constrained quadratic minimization, closed-sphere mapping,
extraction and original-surface validation. A previously failing redistributed
sphere passes with singularity-first rounding and is retained as a regression.
An eight-triangle fan regression retains poles with a full-turn rotation.
Signed nonuniform density with a direction guide, cancellation during integer
rounding, and rejection of unsupported settings/open surfaces also pass locally.

The desktop regression covers rejected settings, cancellation, successful private
preview, source/attribute preservation, Apply as one history edit, Undo/Redo,
save/reopen and stale Apply rejection. The app library compile check passed;
the desktop regression and broad suites run in CI to avoid the known monolithic
test compilation memory cost on this laptop.
The existing interactive GPU regression clicks both method controls and verifies
that changing the method discards an existing private preview without editing the
document, then previews and applies again.

All builds/runs use the shared build lock and one compiler job. The comparison
executable is built separately; the user's release application and saved sources
are unchanged. Local memory guards stop only the diagnostic process group.
