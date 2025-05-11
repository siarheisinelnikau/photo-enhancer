# PhotoEnhancer

Repository: `photo-enhancer`
Priority: 14 of 15 (user-requested launch order)
Product: PhotoEnhancer
Purpose: Local photo denoising, sharpening and upscaling
Inspiration: https://www.topazlabs.com/topaz-photo

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Implement a polished local photo-enhancement workflow: open image, before/after comparison, zoom/pan, exposure/color adjustments, denoise/sharpen/upscale controls, cancellable processing, output format and batch queue. Start with real deterministic image-processing operations; add a clean optional local inference adapter for suitable redistributable enhancement models, with explicit capability/status reporting.

Respect image orientation, color/alpha and originals. Process large images with memory bounds/tiling and keep the UI responsive. Provide only documented, license-compatible model sources and a future user-initiated model setup flow. Do not download huge models, install GPU runtimes, access the user's photo library, call paid AI services or launch the GUI now. Tests should use generated images and small licensed fixtures. Clearly distinguish ordinary interpolation/sharpening from learned restoration; no fabricated benchmarks or claims to recover true lost detail.

## Packaging and publication target

Windows standalone asset: `PhotoEnhancer-vVERSION.exe` (for the initial release, `PhotoEnhancer-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/photo-enhancer (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

