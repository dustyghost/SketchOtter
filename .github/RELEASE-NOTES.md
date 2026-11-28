**SketchOtter Alpha 6 (0.1.0-alpha.6)**

### Changes since Alpha 5

- Added panel generation and surface cuts, including EMB-style cuts. ([#215](https://github.com/dustyghost/SketchOtter/pull/215))
- Added quick-menu pins, modifier taps, Transform spaces and surface splits. ([#218](https://github.com/dustyghost/SketchOtter/pull/218))
- Added independent gizmo rotation and inline target controls. ([#219](https://github.com/dustyghost/SketchOtter/pull/219))
- Refined bevel corners and attribute handling. Fixed surface-rail Slide, mirrored Fuse and Clay symmetry. ([#212](https://github.com/dustyghost/SketchOtter/pull/212), [#213](https://github.com/dustyghost/SketchOtter/pull/213))
- Reduced idle redraws and fixed Linux tablet pressure updates while the UI is idle. ([#211](https://github.com/dustyghost/SketchOtter/pull/211), [#214](https://github.com/dustyghost/SketchOtter/pull/214))
- Fixed Size popup dismissal and added invert actions to the Z menu. ([#209](https://github.com/dustyghost/SketchOtter/pull/209))

[All changes since Alpha 5](https://github.com/dustyghost/SketchOtter/compare/v0.1.0-alpha.5...v0.1.0-alpha.6)

### Downloads

Choose the archive for your operating system and CPU. `aarch64` means ARM64; Apple Silicon Macs use `macos-aarch64`.

Launch `sketchotter` on Linux, `sketchotter.exe` on Windows or `SketchOtter.app` on macOS. Linux packages also accept the previous `clay-form` command.

Linux archives target glibc 2.35+ desktops with a working Vulkan driver. They are not Alpine/musl builds. Windows requires the Microsoft Visual C++ v14 runtime for the selected architecture; use [Microsoft's supported redistributable installer](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/) if needed. The runtime is not bundled.

Windows packages are unsigned. macOS packages have an ad-hoc signature and are not notarized. Operating systems may show download or launch warnings.

**Corresponding source:** `sketchotter-v0.1.0-alpha.6-source.tar.gz` contains the exact app source, build scripts, lockfile and vendored Cargo dependencies for these binaries. See its README and pinned Rust toolchain to rebuild. The compiler and operating-system SDKs are installed separately. Licence notices are included in each binary archive and in About. `SHA256SUMS` contains the archive hashes.

### Current limits

Configurable mesh limits extend to 16 million vertices and 32 million triangles per mesh. These are safety ceilings, not performance guarantees: large scenes can become slow or exceed available memory well below them. Live instances have separate project and expanded-geometry budgets; make an array editable before exporting its result. Preserve surfaces requires closed, consistently oriented solids, and local joins still need wider production-sculpt testing. Guided retopology can reject difficult inputs or reach its bounded solve limit; inspect its preview before applying.

Please include your OS, GPU and input device when reporting problems.
