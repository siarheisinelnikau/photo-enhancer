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

## Download and platform

The intended standalone Windows x64 asset is `PhotoEnhancer-v0.1.0.exe`, with `SHA256SUMS.txt` and license notices. It uses the existing Windows/OpenGL environment and does not bundle models or require a paid service. No architecture suffix is used because only one executable architecture is planned.

## Known limitations before publication

Interactive and packaged update validation, clean-machine compatibility, color/alpha inspection and real application/Settings screenshots remain pending. ICC, high-bit-depth, non-sRGB, HDR, animated inputs, RAW and other unimplemented formats are refused. Preview detail is approximate and capped at 1280 px; full-resolution exports are the output-quality reference. Extremely large files are rejected rather than streamed. No learned model/backend is installed. The repository/release is not yet published; update failures correctly remain unavailable. The packaged application has not been launched during this unattended implementation session.
