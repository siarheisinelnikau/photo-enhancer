# v0.2.0 localization and appearance validation

The v0.2.0 changes add six embedded UI catalogs and saved light/dark themes. The production renderer remains `egui_software_backend` CPU rasterization with `softbuffer` Win32 GDI presentation. Source review confirms worker-triggered wakeups, `ControlFlow::Wait` when no useful deadline exists, and no hidden/minimized window rendering. Appearance is applied only on initialization or a theme change; preferences are persisted only when values change.

The localization checks validate catalog parity, nonempty messages, placeholder preservation, and catalog coverage for explicit production lookups. Focused tests cover old JSON defaults, legacy RON migration without overwriting the old file, saved language/theme round trips without enabling startup checks, literal paths containing braces, and translated export/settings layout at 920 × 640 for all six languages in both themes. Existing headless scheduling tests remain in place, with added checks that translated Settings settles without periodic repaint requests.

Verified commands: `cargo fmt --all -- --check`, `python scripts/check_software_backend.py`, `python scripts/check_localization.py`, `cargo test --locked --offline --all-targets` (32 tests passed), and `cargo clippy --locked --offline --all-targets -- -D warnings`. Cargo invocations use two jobs and the repository-local target directory under the shared build gate.

This pass is code-only under the repository AGENTS.md prohibition on GUI launches and screenshots. No v0.2.0 screenshot, native visible/hidden/restore measurement, OS-native dialog interaction, updater installation or subjective responsiveness claim is made. Historical screenshots are retained and labeled as v0.1.1 in README. Updated workspace and Settings images for v0.2.0 are still required when GUI review is authorized.

The sections below are historical evidence for earlier versions; their binaries, hashes and runtime results do not certify v0.2.0.

---

# Validation record

Date: 2026-10-31 (Europe/Minsk). Follow-up GUI work was authorized after the initial code-only implementation. The final review followed the user's later instruction to avoid Computer Use: real application self-captures plus a bounded automatic native-window diagnostic. No real photo library, updater installer, global graphics settings, or unrelated application was operated.

## Build and regression checks

- Windows x64 MSVC, Rust/Cargo 1.96.1, repository-local `target/`, at most two Cargo jobs.
- `./scripts/check.ps1 -Offline`: **28 tests passed**, formatting passed, and Clippy with `-D warnings` passed. The compiler reports unavailable incremental-cache hard links and copies cache files instead.
- Engine tests cover byte-preserving identity, alpha, linear exposure/interpolation, denoise/sharpen effect, limits, cancellation, orientation, unsupported color/depth rejection, and no-clobber export including races.
- Worker tests cover failure recovery and selected-image cache switching after batch export.
- UI tests cover long settings errors, a visible primary export action at 920 × 640, divider drag without photo pan, preserving the old image/preview after a failed open, settled layouts at three window sizes, and stationary hover over controls/scrollbars/canvas.
- Preferences migration preserves startup-check opt-out, format and quality while keeping the original RON file unchanged.
- Offline release compilation/package completed. The capture parser rejects an unknown screen before startup, returns nonzero, and produces no stale PNG.

Tests use generated images and isolated directories inside this repository. Dependency resolution used the local cache; no models, inference runtimes, or paid APIs were downloaded or called.

## Photo-opening and icon regression checks

The v0.1.0 loader rejected every embedded ICC profile, including ordinary sRGB-tagged photos. The v0.1.1 loader instead transforms supported RGB/grayscale profiles to sRGB on the image worker. Five additional tests cover sRGB pixel fidelity and untouched source bytes, Display P3 gamut conversion and normalized export, grayscale gamma conversion, exact alpha preservation, Adobe RGB JPEG with EXIF orientation, and clear errors for damaged/mismatched profiles.

`cargo run --offline --locked --example color_fixtures` produced reproducible sRGB, Display P3 and Adobe RGB tagged fixtures. An independent Pillow/LittleCMS transform of the same profile bytes and test color `(130, 180, 80)` differed from PhotoEnhancer by 0, 0 and 1 RGB levels respectively, out of 255. This is a focused numerical cross-check, not monitor calibration or certification of every ICC profile. The GUI review below used an ICC-tagged 1600 × 1000 copy of the original landscape fixture.

`python scripts/verify_windows_icon.py dist/PhotoEnhancer-v0.1.1.exe` mapped the packaged EXE as resource data without executing it. All nine embedded RGBA icon payloads matched the source ICO byte-for-byte at 16, 20, 24, 32, 40, 48, 64, 128 and 256 pixels. These sizes were decoded, and small icons were visually reviewed on light/dark backgrounds. The product version and original filename resources report `0.1.1` and `PhotoEnhancer-v0.1.1.exe`. The native-window diagnostic also exercised initialization with the matching PNG window icon; desktop/taskbar appearance is outside the client-area captures.

## Real visual evidence

The actual packaged `PhotoEnhancer-v0.1.1.exe` produced six finite self-captures through its production UI and CPU rasterizer: workspace, settings, minimum-size workspace, settings at 125% scale, empty state, and a real image-decoding error. All six PNGs were inspected with a local image viewer. The selected workspace and Settings PNGs were copied byte-for-byte into `docs/screenshots/` for README.

The standard client area is 1280 × 900 logical/physical pixels at 100%. Compact review uses 920 × 640 logical pixels. Scaled Settings uses 920 × 640 logical and 1150 × 800 physical pixels. Each invocation renders 16 bounded frames and exits, with internal/external deadlines, no normal-profile persistence and no network requests. The original generated landscape fixture is enhanced by the real engine, rather than being a separate UI mockup.

Visual corrections include readable type/contrast, aligned controls, an always-visible export action and size/format summary, explicit scrolling when controls do not fit, collapsed batch details, labeled before/after images, a direct divider, long-error containment, and clearly separated settings disclosures. Unsupported arrow glyphs were replaced with readable dimensions. These PNGs establish client-area appearance; they do not include native window chrome, system dialogs, input latency, or monitor color calibration.

Capture evidence directory for this build: `.test-output/ui-review/run-23qtfxlq/` (ignored generated data). Capture manifests record the executable, renderer, scale, dimensions, state, bounded frame count and successful completion. Populated states opened `.test-output/color-fixtures/harbor-srgb.png`, with its embedded ICC profile, without an error.

## Native rendering/load gate

Code state: `f7aa27d` (v0.1.1). Diagnostic PID: `57320`. The sampled executable was the packaged Windows x64 release identified by the digest below, with the ICC-tagged 1600 × 1000 fixture; startup update checks and settings persistence were disabled in that isolated instance. It created one non-activating native window, measured actual visible/hidden/restored states, saved its report, and exited after 16.001 seconds under a 25-second external limit.

The running path is egui → `egui_software_backend` 0.0.2 CPU rasterization → retained bitmap → `softbuffer` 0.4.8 Win32 GDI. This is supported by the active dependency graph, the presenter's Win32 implementation, the executing host path, and loaded-module inspection. No OpenGL/Direct3D modules were loaded. Windows desktop composition is still an OS responsibility.

| State | Clean sample | CPU, one core | Layout frames | Raster frames | Presentations |
| --- | --- | --- | --- | --- | --- |
| visible_settled | 5.000 s | 0.000% | 0 | 0 | 0 |
| hidden | 4.998 s | 0.000% | 0 | 0 | 0 |
| restored_settled | 3.000 s | 0.000% | 0 | 0 | 0 |

All sample CPU counters were available, reported zero CPU-time growth at Windows accounting resolution, and reported 0.000% machine-normalized CPU on 20 logical CPUs. There were no input events, repaint events or repaint causes in those clean intervals. Outside the samples, the launch/finite transitions/restore produced 96 layouts/rasters and 96 presentations. Frame wall time including UI, rasterization and GDI presentation averaged 4.46 ms; maximum 29.02 ms includes startup. The unsampled warm-up intervals are not idle evidence. These are a small diagnostic sample, not a general benchmark or active-input latency measurement.

During the earlier v0.1.0 review, one interval produced recurring frames before input/cause instrumentation was available. Visual QA was suspended and the disposable process exited. Cause counters and stationary-hover regressions were added; subsequent instrumented clean runs and this final packaged run had zero recurring work. The earlier interval's cause was not established and is not attributed to a driver or parallel activity without evidence. Input-contaminated intervals are now explicitly rejected as idle evidence. Raw final report: `.test-output/ui-review/run-h6xxvvg1/runtime.json`.

The measured visible, hidden, and restored-idle scheduling checks passed. Physical pointer/keyboard input, OS menus/dialogs, minimize/restore appearance, GPU/compositor telemetry and subjective resolution of the user's original stutter remain separate manual checks. A screenshot or these idle samples alone cannot certify them.

## Release artifact

`dist/PhotoEnhancer-v0.1.1.exe`: **10,224,128 bytes**.

SHA-256: `46f0c91c350af4827e4f8fb72bdb56dee56c1d6a56e3e81f94882aea36dd33e6`.

A read-only PE inspection verified MZ/PE signatures, Windows x64 machine type and Windows GUI subsystem. Imports contain no OpenGL/Direct3D renderer or external `vcruntime`, `msvcp` or `ucrtbase` DLL; there are no delay imports. The actual digest matches `dist/SHA256SUMS.txt`; packaged notices match source notices byte-for-byte. The standalone EXE embeds the product and dependency license notices; generation covered 146 active Windows dependencies including build dependencies with no missing license texts.

No release, repository, tag, PR, or remote Git change was published. No live update download, replacement or restart was executed.

## Remaining manual checks

- Real OS file dialogs, keyboard/focus/accessibility traversal, pointer-active responsiveness and perception of desktop/cursor stutter; clean-machine Windows compatibility.
- Live DPI changes, minimize/restore image/font appearance, representative real-monitor color and transparent edges. The captured static layouts and engine tests have their own narrower evidence above.
- Larger-image peak memory, codec cancellation latency, disk-full/unwritable destination handling and closing during work, using disposable fixtures.
- After an authorized release exists: real startup opt-out across normal relaunch, manual update checking, network/rate-limit failures, asset verification, download cancellation, portable replacement/restart and preserved data. Never use a working installation as the updater fixture.


## Shared updater and icon/render audit — 2026-11-01

This pass used source inspection, isolated fixture tests, optimized compilation and native resource drawing in a memory DC. It did not launch an application window or use computer-use/input/screen-capture tools. Previous runtime measurements above belong to earlier binaries and are not a measurement of this change.

The Windows graph uses `egui_software_backend` for CPU rasterization and `softbuffer` for GDI presentation; the graph check rejects GPU/OpenGL backends. The native host retains pixels for unchanged OS exposures and now suppresses frames for `Occluded(true)` as well as hidden/minimized windows. The former unconditional 150-ms repaint while any worker was busy was removed. Image progress and shared updater progress now post throttled notifications only for actual changes. A new headless regression verifies an outstanding update does not generate periodic repaint deadlines; existing settled-layout and stationary-hover tests remain covered.

The executable icon has 16, 20, 24, 32, 40, 48, 64, 128 and 256-pixel variants. Alpha spans 0–255 at every size. Window and taskbar assignments now load the same native resource group rather than only the 256-pixel PNG. `native_icon_proof.py` loads the executable as resource data and calls Win32 `LoadImageW`/`DrawIconEx` into a memory bitmap on light/dark backgrounds; it neither executes the application nor captures the OS desktop. The image is an icon resource proof, not a tray screenshot. PhotoEnhancer has no tray interface.

All reusable update mechanics are in `../rust-auto-updator`. The app uses its cancellable `Service`, accepts previously published digest-only assets, keeps the saved startup opt-out and manual checks, and waits for explicit installation/restart. The shared installer is configured to acknowledge successful first-frame startup before deleting rollback files, with a 30-second health window. The sibling workspace is required for local/CI source builds; no updater package or repository was remotely published.

Remaining verification: actual new-binary visible/hidden/minimized idle frame/presentation counters, CPU sampling, restore/interaction/DPI behavior, OS taskbar appearance and full product update installation were not exercised under this no-GUI audit. Source inspection and headless tests do not certify that runtime gate.


Final checks for this follow-up: `cargo test --all-targets --offline --locked` passed 27 tests; `cargo clippy --all-targets --offline --locked -- -D warnings` passed; `cargo fmt --all -- --check`, dependency graph and regenerated license notices passed. `scripts/package.ps1 -Offline` built `PhotoEnhancer-v0.1.1.exe` (Windows x64 GUI) and verified GDI imports with no OpenGL/Direct3D/Vulkan renderer imports. `verify_windows_icon.py` verified all nine embedded icon entries against the source. The final [native icon resource proof](screenshots/icon-resources.png) was visually inspected at native sizes on light and dark backgrounds. These are local verification artifacts, not a new release or publication.
