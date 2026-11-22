# Experimental integer-grid parameterization

Experimental alternative for [#191](https://github.com/dustyghost/clay-form/issues/191).
The existing Surface field method remains the default. This is a separate native
Rust parameterization path, not another name for its lattice repair.

## Research and implementation boundary

- [Mixed-Integer Quadrangulation, 2009](https://cgg.unibe.ch/publications/miq/bommes_zimmer_2009_siggraph_011.pdf): field-aligned gradient energy, cut charts, quarter-turn seam transitions, integer translations and singularity coordinates, nearest-first rounding with continuous updates.
- [Integer-Grid Maps for Reliable Quad Meshing, 2013](https://www.graphics.rwth-aachen.de/publication/03197/): positive map orientation and conservative linear orientation constraints. This experiment uses the general interior-barycentric-point construction from §3.1, with a centroid instead of the paper's Fermat point.

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

## Current limits

The alternative handles closed surfaces. Layout symmetry and Protect sharp bends
are explicitly rejected for this method. Signed density and direction guides use
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
quadratic solves were the initial alternative's measured bottleneck. The final
implementation eliminates seam/fixed equations and updates the quadratic energy
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
