# PhotoEnhancer v0.2.0

This is the first published release of PhotoEnhancer.

## Initial capabilities

- Open PNG/JPEG photos, apply orientation and local ICC-to-sRGB conversion, and compare the original with edited previews.
- Adjust light and color, apply deterministic denoising/sharpening and enlarge up to four times with interpolation.
- Export new PNG/JPEG files or process a bounded batch without overwriting originals. Processing is local and does not use learned detail restoration or cloud inference.

## Localization and appearance

PhotoEnhancer now offers a translated interface and a choice of light or dark appearance. These changes are compared with the v0.1.1 source baseline; previous validation notes remain archived with that version.

## Added

- Complete English, Russian, French, German, Spanish and Italian UI catalogs covering the photo workspace, adjustments, comparison controls, batch export, Settings, update actions, file-dialog titles, status messages and common processing errors.
- A discoverable language selector using native language names in Settings → Appearance. Changes apply immediately, including messages already displayed in the status bar and batch queue.
- Light and Dark theme controls in the same settings section. Both palettes cover panels, controls, text, selections, warnings, comparison labels and the transparency checkerboard.
- Persistent appearance preferences with migration defaults, preserving existing export options, JPEG quality and the startup update-check opt-out.

## Improved

- Adjustment labels use their own line, and empty-workspace guidance wraps within the canvas, accommodating longer translations at the minimum window size.
- Parameterized messages preserve paths, version numbers and other supplied values without translating or reinterpreting them.
- Catalog coverage and placeholder checks, old-settings migration tests, and headless layout/repaint checks cover the supported languages and themes.

## Download and platform

The standalone asset is `PhotoEnhancer-v0.2.0.exe` for Windows x64, accompanied by `SHA256SUMS.txt` and license notices. It uses the existing CPU/GDI renderer. No model weights, cloud processing or paid API are required. Settings and source photos remain outside the update replacement transaction.

## Validation and remaining checks

Automated formatting, dependency-backend checks, generated-fixture tests and Clippy are run before publication. This source-only change does not claim a new native runtime/performance pass. Current repository instructions prohibit GUI launches and screenshots; refreshed v0.2.0 workspace/Settings screenshots, visual review of both palettes, OS-native interactions and installed-update validation remain pending. The README retains explicitly labeled real v0.1.1 captures. Historical evidence is in [VALIDATION.md](VALIDATION.md), and the previous release notes are in [releases/v0.1.1.md](releases/v0.1.1.md).
