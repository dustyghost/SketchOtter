**SketchOtter Alpha 5 (0.1.0-alpha.5)**

Clay Form is now SketchOtter. This first SketchOtter release includes the teal clay otter welcome screen and desktop icon. Existing `.clay` projects, brushes, preferences and recovery files continue to use the same locations.

### Changes since Alpha 4

- **Guided retopology:** Integer grid is now the default, with **Protect sharp bends** enabled. Selected guides highlight immediately, and saved Surface paths follow the preview and transfer to the new model when applied. The previous Surface field choice is retired. This remains experimental, requires a closed manifold mesh and does not support layout symmetry. ([#192](https://github.com/dustyghost/SketchOtter/pull/192))
- **Editable paths:** restored **New ring** and **Auto / X / Y / Z** controls. ([#192](https://github.com/dustyghost/SketchOtter/pull/192))
- **Dense sculpting:** improvements to visible wireframe, picking, shading, local brush display updates and GPU uploads. Freehand Paint avoids a duplicate Undo snapshot. Performance gains depend on the mesh, brush and hardware. ([#184](https://github.com/dustyghost/SketchOtter/pull/184), [#185](https://github.com/dustyghost/SketchOtter/pull/185), [#186](https://github.com/dustyghost/SketchOtter/pull/186))
- **Mesh capacity:** new defaults of **1,500,000 vertices / 3,000,000 triangles** per mesh. Existing saved limits stay unchanged; use **Settings → Mesh capacity → Reset mesh limits**, then restart, to adopt the new defaults. ([#186](https://github.com/dustyghost/SketchOtter/pull/186))
- **Visibility and masks:** added **Split masked** and **Delete hidden**. **Group all** preserves hidden groups, and extracted pieces retain UV and shading seams. ([#187](https://github.com/dustyghost/SketchOtter/pull/187))
- **Mesh joins:** preserve UV seams, prepare mapped meshes faster and keep detailed previews cancellable. ([#181](https://github.com/dustyghost/SketchOtter/pull/181), [#183](https://github.com/dustyghost/SketchOtter/pull/183))
- **Reliability:** stronger mesh validation, mixed-edit/history regression coverage and a supported focused retopology test partition. ([#170](https://github.com/dustyghost/SketchOtter/pull/170), [#190](https://github.com/dustyghost/SketchOtter/pull/190))

[All changes since Alpha 4](https://github.com/dustyghost/SketchOtter/compare/v0.1.0-alpha.4...v0.1.0-alpha.5)

### Downloads

Choose the archive for your operating system and CPU. `aarch64` means ARM64; Apple Silicon Macs use `macos-aarch64`.

Launch `sketchotter` on Linux, `sketchotter.exe` on Windows or `SketchOtter.app` on macOS. Linux packages also accept the previous `clay-form` command.

Linux archives target glibc 2.35+ desktops with a working Vulkan driver. They are not Alpine/musl builds. Windows requires the Microsoft Visual C++ v14 runtime for the selected architecture; use [Microsoft's supported redistributable installer](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/) if needed. The runtime is not bundled.

Windows packages are unsigned. macOS packages have an ad-hoc signature and are not notarized. Operating systems may show download or launch warnings.

**Corresponding source:** `sketchotter-v0.1.0-alpha.5-source.tar.gz` contains the exact app source, build scripts, lockfile and vendored Cargo dependencies for these binaries. See its README and pinned Rust toolchain to rebuild. The compiler and operating-system SDKs are installed separately. Licence notices are included in each binary archive and in About. `SHA256SUMS` contains the archive hashes.

### Testing and current limits

Testing includes maintainer use on Arch Linux, automated CPU and headless Vulkan checks, and recorded visual/interaction reviews. Dense sculpting also has recorded software Vulkan and Apple Metal comparisons. Windows x86-64 and ARM64 PR packaging checks exercise pen routing and brush selection. Broader desktop, GPU and physical tablet testing is still needed, particularly on Windows and macOS; successful native builds do not establish runtime compatibility.

Configurable mesh limits extend to 16 million vertices and 32 million triangles per mesh. These are safety ceilings, not performance guarantees: large scenes can become slow or exceed available memory well below them. Live instances have separate project and expanded-geometry budgets; make an array editable before exporting its result. Preserve surfaces requires closed, consistently oriented solids, and local joins still need wider production-sculpt testing. Guided retopology can reject difficult inputs or reach its bounded solve limit; inspect its preview before applying.

Please include your OS, GPU and input device when reporting problems.

Built from [`c76401e`](https://github.com/dustyghost/SketchOtter/commit/c76401e8d16d3f0a9673961bb7593d98f76d891a). [Native release workflow](https://github.com/dustyghost/SketchOtter/actions/runs/45213486717) · [Main-branch CI](https://github.com/dustyghost/SketchOtter/actions/runs/45211943470).

All seven archive checksums, source/dependency correspondence, native executable architectures, bundled notices and the Linux command-line smoke test passed [archive verification](https://github.com/dustyghost/SketchOtter/actions/runs/45217423635). Windows text files were verified allowing only CRLF line-ending conversion.
