# Optional local inference: not installed

Version 0.1.0 bundles deterministic filters only. There are no model weights, inference runtime, GPU provider, automatic model downloads, or remote inference calls. The Settings panel reports learned restoration as unavailable. `LocalEnhancementBackend` is an extension contract, not a working ONNX integration.

## Upstream candidate for future review

[Real-ESRGAN](https://github.com/xinntao/Real-ESRGAN) publishes a photo-restoration implementation and model references under a [BSD 3-Clause project license](https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE), compatible with an MIT application when its notices and conditions are retained. Use the original upstream model references only. No third-party conversions, mirrors, weights, runtime packages or licenses have been imported or validated here. The project license alone is not a verification of every separately hosted or converted weight file.

Before enabling a particular model, pin the exact upstream release/asset, weight-specific redistribution permission, SHA-256, input/output tensor contract, color range, precision, scale, required runtime, and its redistribution terms. Do not advertise a downloadable source as supported until that complete asset has been reviewed. No model is approved for distribution by this initial version.

## Intended user-initiated setup

A future Models settings screen should present only reviewed manifests with source, version, license, size, checksum, capabilities and hardware requirements. The user chooses an explicit install/import action and accepts the applicable license; only then may the app download bounded files into its application-data model directory. Show progress, cancellation, disk-space and checksum errors. Do not download at application startup, run supplied scripts, install drivers, or silently fall back to cloud processing. Show unavailable/unsupported-provider status and let deterministic processing remain available.

The adapter must load local resources only, bound memory with overlap tiles, preserve alpha separately, validate output dimensions and numeric ranges, check cancellation per tile, and keep inference off the UI thread with bounded concurrency. Test seam handling and edge padding on generated fixtures before supporting real photographs. Hardware/provider checks and model-specific quality evaluation remain future work; no claims of recovering true lost detail should be made.
