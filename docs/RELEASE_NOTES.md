# PhotoEnhancer v0.1.0 — initial release draft

This is the first PhotoEnhancer implementation. No preceding release exists and this draft has not been published.

## Added

- Local PNG/JPEG enhancement with exposure, saturation, warmth, mild edge-aware noise reduction, and unsharp masking.
- Original before/after workspace with split comparison, preview zoom and pan, explicit preview rendering, and full-resolution export.
- Linear-light 1×–4× interpolation with transparent-edge handling. This is ordinary interpolation, not AI restoration.
- PNG export preserving alpha and declaring sRGB; adjustable-quality JPEG export for opaque images. Originals and existing output names are protected, and exported copies omit private metadata.
- A sequential, cancellable 64-photo batch queue with per-file errors and a selected destination folder.
- Background stable-release checks, a persistent startup opt-out, installed-version display, SHA-256-verified user-initiated portable update installation, and restart controls.
- Bounds on input/output pixels, file sizes, scratch allocations, worker count, and queued jobs. Generated-data tests exercise orientation, color operations, transparency, cancellation, collisions, cache switching and release metadata selection.

## Improved during GUI review

- Replaced the OpenGL host with CPU/GDI rendering and a reactive, retained-bitmap event loop. Bounded diagnostics count actual layout, rasterization and presentation in visible/hidden/restored states.
- Increased text sizes and contrast, aligned adjustment rows, kept export visible at the minimum window size, and collapsed batch details until needed.
- Added labeled before/after views and a divider that drags directly on the image, with a regression test to prevent accidental panning.
- Bounded long update errors and settings scrolling; source preferences migrate from the initial host without changing the old settings file.
- Added real executable-generated workspace/Settings screenshots, plus an isolated, bounded capture mode for repeatable visual review.

## Download and platform

The intended standalone Windows x64 asset is `PhotoEnhancer-v0.1.0.exe`, with `SHA256SUMS.txt` and license notices. It uses CPU rasterization with Windows GDI presentation and does not bundle models or require a paid service. No architecture suffix is used because only one executable architecture is planned.

## Known limitations before publication

Packaged update installation, clean-machine compatibility, real-monitor color/alpha evaluation, and OS-native interaction/accessibility checks remain pending. Real client-area screenshots and bounded native lifecycle diagnostics are included; they do not certify subjective responsiveness. ICC, high-bit-depth, non-sRGB, HDR, animated inputs, RAW and other unimplemented formats are refused. Preview detail is approximate and capped at 1280 px; full-resolution exports are the output-quality reference. Extremely large files are rejected rather than streamed. No learned model/backend is installed. The repository/release is not yet published; update failures correctly remain unavailable.
