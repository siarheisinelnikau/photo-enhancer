# Validation record

Date: 2026-10-31 (Europe/Minsk). Follow-up GUI work was authorized after the initial code-only implementation. The final review followed the user's later instruction to avoid Computer Use: real application self-captures plus a bounded automatic native-window diagnostic. No real photo library, updater installer, global graphics settings, or unrelated application was operated.

## Build and regression checks

- Windows x64 MSVC, Rust/Cargo 1.96.1, repository-local `target/`, at most two Cargo jobs.
- `./scripts/check.ps1 -Offline`: **23 tests passed**, formatting passed, and Clippy with `-D warnings` passed. The compiler reports unavailable incremental-cache hard links and copies cache files instead.
- Engine tests cover byte-preserving identity, alpha, linear exposure/interpolation, denoise/sharpen effect, limits, cancellation, orientation, unsupported color/depth rejection, and no-clobber export including races.
- Worker tests cover failure recovery and selected-image cache switching after batch export.
- UI tests cover long settings errors, a visible primary export action at 920 × 640, divider drag without photo pan, preserving the old image/preview after a failed open, settled layouts at three window sizes, and stationary hover over controls/scrollbars/canvas.
- Preferences migration preserves startup-check opt-out, format and quality while keeping the original RON file unchanged.
- Offline release compilation/package completed. The capture parser rejects an unknown screen before startup, returns nonzero, and produces no stale PNG.

Tests use generated images and isolated directories inside this repository. Dependency resolution used the local cache; no models, inference runtimes, or paid APIs were downloaded or called.

## Real visual evidence

The actual packaged `PhotoEnhancer-v0.1.0.exe` produced six finite self-captures through its production UI and CPU rasterizer: workspace, settings, minimum-size workspace, settings at 125% scale, empty state, and a real image-decoding error. The selected workspace and Settings PNGs were inspected with a local image viewer and copied byte-for-byte into `docs/screenshots/` for README.

The standard client area is 1280 × 900 logical/physical pixels at 100%. Compact review uses 920 × 640 logical pixels. Scaled Settings uses 920 × 640 logical and 1150 × 800 physical pixels. Each invocation renders 16 bounded frames and exits, with internal/external deadlines, no normal-profile persistence and no network requests. The original generated landscape fixture is enhanced by the real engine, rather than being a separate UI mockup.

Visual corrections include readable type/contrast, aligned controls, an always-visible export action and size/format summary, explicit scrolling when controls do not fit, collapsed batch details, labeled before/after images, a direct divider, long-error containment, and clearly separated settings disclosures. Unsupported arrow glyphs were replaced with readable dimensions. These PNGs establish client-area appearance; they do not include native window chrome, system dialogs, input latency, or monitor color calibration.

Capture evidence directory for this build: `.test-output/ui-review/run-2fsifrwk/` (ignored generated data). Capture manifests record the executable, renderer, scale, dimensions, state, bounded frame count and successful completion.

## Native rendering/load gate

Code state: `060a02c`. Diagnostic PID: `27936`. The sampled executable was the packaged Windows x64 release with the generated 1600 × 1000 fixture; startup update checks and settings persistence were disabled in that isolated instance. It created one non-activating native window, measured actual visible/hidden/restored states, saved its report, and exited after 16.001 seconds under a 25-second external limit.

The running path is egui → `egui_software_backend` 0.0.2 CPU rasterization → retained bitmap → `softbuffer` 0.4.8 Win32 GDI. This is supported by the active dependency graph, the presenter's Win32 implementation, the executing host path, and loaded-module inspection. No OpenGL/Direct3D modules were loaded. Windows desktop composition is still an OS responsibility.

| State | Clean sample | CPU, one core | Layout frames | Raster frames | Presentations |
| --- | --- | --- | --- | --- | --- |
| visible_settled | 5.000 s | 0.000% | 0 | 0 | 0 |
| hidden | 4.998 s | 0.000% | 0 | 0 | 0 |
| restored_settled | 3.000 s | 0.000% | 0 | 0 | 0 |

All sample CPU counters were available, reported zero CPU-time growth at Windows accounting resolution, and reported 0.000% machine-normalized CPU on 20 logical CPUs. There were no input events, repaint events or repaint causes in those clean intervals. Outside the samples, the launch/finite transitions/restore produced 18 layouts/rasters and 18 presentations. Frame wall time including UI, rasterization and GDI presentation averaged 5.87 ms; maximum 26.37 ms includes startup. These are a small diagnostic sample, not a general benchmark or active-input latency measurement.

An earlier interval produced recurring frames before input/cause instrumentation was available. Visual QA was suspended and the disposable process exited. Cause counters and stationary-hover regressions were added; subsequent instrumented clean runs and this final packaged run had zero recurring work. The earlier interval's cause was not established and is not attributed to a driver or parallel activity without evidence. Input-contaminated intervals are now explicitly rejected as idle evidence. Raw final report: `.test-output/ui-review/run-ktgd_05l/runtime.json`.

The measured visible, hidden, and restored-idle scheduling checks passed. Physical pointer/keyboard input, OS menus/dialogs, minimize/restore appearance, GPU/compositor telemetry and subjective resolution of the user's original stutter remain separate manual checks. A screenshot or these idle samples alone cannot certify them.

## Release artifact

`dist/PhotoEnhancer-v0.1.0.exe`: **9,561,600 bytes**.

SHA-256: `efcd455abb731e9811ccb693754f766b20924a9f946356958f8ff85e99da5dd3`.

A read-only PE inspection verified MZ/PE signatures, Windows x64 machine type and Windows GUI subsystem. Imports contain no OpenGL/Direct3D renderer or external `vcruntime`, `msvcp` or `ucrtbase` DLL. The actual digest matches `dist/SHA256SUMS.txt`; packaged notices match source notices byte-for-byte. The standalone EXE embeds the product and dependency license notices; generation covered 145 active Windows dependencies including build dependencies with no missing license texts.

No release, repository, tag, PR, or remote Git change was published. No live update download, replacement or restart was executed.

## Remaining manual checks

- Real OS file dialogs, keyboard/focus/accessibility traversal, pointer-active responsiveness and perception of desktop/cursor stutter; clean-machine Windows compatibility.
- Live DPI changes, minimize/restore image/font appearance, representative real-monitor color and transparent edges. The captured static layouts and engine tests have their own narrower evidence above.
- Larger-image peak memory, codec cancellation latency, disk-full/unwritable destination handling and closing during work, using disposable fixtures.
- After an authorized release exists: real startup opt-out across normal relaunch, manual update checking, network/rate-limit failures, asset verification, download cancellation, portable replacement/restart and preserved data. Never use a working installation as the updater fixture.
