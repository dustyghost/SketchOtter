# Demand-driven idle repaint validation

Issue #177 removes the unconditional 33 ms repaint timer. Active workers still
use bounded polling; preference saves and recovery use their actual deadlines.
Egui input, animations and the viewport's time-based interactions keep their
own repaint requests.

## Baseline and result

Measured in release mode on the same desktop before and after the scheduling
change. The window was left stationary, with no keyboard or pointer activity
after launch. Each run had a 5-second warm-up, a 12-second process-CPU sample,
then a clean window close. The trace includes startup and close frames.

| Scene | Geometry | Before: app updates over ~16 s | Before: CPU | After: app updates over ~16 s | After: CPU |
| --- | ---: | ---: | ---: | ---: | ---: |
| Empty project | 0 vertices, 0 triangles | 873 (~54/s) | 24.00% | 8 (~0.49/s) | 0.58% |
| Default startup scene | Starting sphere | 901 (~56/s) | 23.41% | 7 (~0.43/s) | 0.58% |
| Loaded fixture | 2,782 vertices, 5,429 triangles | 899 (~56/s) | 22.50% | 7 (~0.43/s) | 0.67% |

CPU is the process's `/proc/<pid>/stat` CPU time divided by the 12-second
sample, expressed as a percentage of one logical CPU. This is not a power
measurement. Post-change traces contained two input-event frames across the
whole run (launch and close); the baseline had one input-event frame at launch
and none in the steady intervals.

## Environment and method

- Arch Linux, kernel `7.3.5-arch1-1`, Wayland/Sway 1.12.
- Intel Core i7-7500U, four logical CPUs.
- Intel HD Graphics 620, Vulkan, Mesa 26.3.1.
- 1920×1080 display at 60 Hz; the app window opens at 1280×850.
- Release build: `cargo build --locked --release -j 1`.
- Repeat the post-change desktop sample with
  `python3 scripts/measure-idle-redraws.py`. It runs all three scenes, samples
  process CPU and prints the opt-in trace. It requires Linux/Sway, refuses to
  run if a SketchOtter process is already open, and uses temporary preferences
  and project files.
- Set `CLAY_IDLE_FRAME_TRACE=1` to log adapter details, `App::update` counts,
  input-event frames and egui frame numbers. For repeatable startup, use an
  isolated `XDG_CONFIG_HOME` containing a `sketchotter/preferences.json` with
  `{"dismissed_startup_version":"0.1.0-alpha.5"}`; this avoids changing the
  user's preferences.
- The loaded fixture is `tests/fixtures/smoothing-groove.json` wrapped in the
  project metadata from `tests/fixtures/v1-project.json`. The empty project
  uses that same metadata and an empty mesh.

## Interpretation and remaining validation

This trace counts app updates and egui frames, not completed GPU work or
compositor presentations. Input-event frames help identify launch/close input,
but do not classify every OS-generated redraw. The static post-change runs
showed no recurring 33 ms app timer and a large reduction in app updates and
process CPU; they do not establish battery savings.

Desktop follow-up should still cover pen pressure changes without pointer
movement, input after long idle, active progress and cancellation, focus and
minimise/restore, and additional platform/tablet backends. No GPU regression,
physical-tablet or cross-platform claim is made here.

## Follow-up: Linux pressure and preference deadline

The Linux evdev reader now requests a repaint when contact, pressure, or device
status changes, not on its steady 8 ms sample loop. A focused test simulates
publications from a worker thread without pointer events and checks that held
and idle samples do not request recurring repaints. A separate app-frame test
exercises the preference-save deadline through an idle frame and reads the
persisted file.

On 26 November 2026, a user ran the release build on Linux with a Huion
Kamvas 13 and reported that the manual tablet checklist looked good. That
checklist covered stationary pressure changes, input after idle, minimise and
restore, sculpting, and camera navigation; per-step results were not recorded.
This is a user-reported hardware smoke test, not a new instrumented trace.
Windows and macOS tablet backends and visible progress/cancellation still
require manual checks. The existing desktop measurements above were not
rerun or changed.
