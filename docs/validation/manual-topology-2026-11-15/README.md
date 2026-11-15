# Manual topology construction — 15 November 2026

Tickets: [#126](https://github.com/dustyghost/clay-form/issues/126) and the audit/first delivery of [#127](https://github.com/dustyghost/clay-form/issues/127). See the [capability matrix](../../research/modeling-capabilities-2026-11-15.md) for existing workflows, limits and separately tracked follow-ups.

The nine focused `manual_topology` geometry/controller tests passed. They cover triangle/quad/concave/mildly nonplanar caps; unchanged vertices, materials, masks, groups, creases, hard shading edges and existing UV corners; explicit unmapped new caps; either quad diagonal; ambiguous UV rejection; partial, branched, crossed, obstructed and stale boundaries; symmetry; protected/hidden cap vertices; navigation; private preview and cancellation; one-step Undo/Redo; brush-change Apply/Keep editing; changed-target rejection; and serialization.

The existing topology operation regression, expanded-symmetry Connect regression and coalesced pen-selection regression also passed. Local builds used one compiler job under the shared build lock. Full tests, formatting, lint and native packaging are left to PR CI after rebasing onto the merged array work.

`manual_topology_offscreen_review` passed on Vulkan llvmpipe (LLVM 23.1.4, Mesa 26.2.6), with one software rendering thread. It clicks the actual controls, deletes a cube face, selects its four boundary vertices through pointer events while orbiting to expose their adjoining surfaces, previews/cancels/applies a cap, uses Undo/Redo, previews/applies a diagonal, and round-trips the project. See [interaction.txt](interaction.txt). The walkthrough runs in GPU CI.

Visual inspection confirmed selected boundary points, a filled quad preview, the explicit diagonal, and reachable Apply/Cancel controls. The source-document face count remains unchanged until Apply. Captures appear in the [modelling guide](../../guide/modeling.md#connect-vertices-and-create-faces).

Mouse and pen share the tested primary-pointer selection path; no physical tablet was available for a hardware feel check. This delivery supports complete existing boundaries and quad diagonals, not loose topology, stored n-gons or advanced quad/adaptive cap generation (#125). New cap UVs require unwrapping.
