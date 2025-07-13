# Validation record

Development date: 2026-10-31 (Europe/Minsk). No desktop application, browser UI, updater installer, device or real photo library was operated.

## Completed noninteractive checks

- Rust/Cargo 1.96.1 on Windows x64 MSVC; repository-local target and at most two Cargo jobs.
- Offline locked debug compilation and tests: 16 passing tests (3 release-metadata tests, 12 image-engine tests, 1 worker integration test).
- Generated fixtures cover identity over every byte value, unchanged alpha, exposure, denoise/sharpen effect, premultiplied linear-light interpolation, parameter/pixel limits, cancellation before and during work, PNG round-trip, JPEG transparency/CMYK refusal, EXIF rotation, declared non-sRGB PNG/high-bit-depth refusal, no-overwrite publication races, worker failure recovery and selected-image cache switching.
- `cargo fmt --all -- --check` and `cargo clippy --offline --locked --all-targets -- -D warnings`.
- Release build/package status is recorded below after execution. The executable is never launched by package/check scripts.
- Dependency notices generated from the locked, active Windows dependency tree; includes font texts and build dependency notices. No models or other large artifacts were downloaded.

Fixture directories are created under `target/test-fixtures/`, not in a user's photo folders. Initial tests under the host temporary directory encountered sandbox-denied atomic publication; moving isolated fixtures into the authorized project workspace allowed the same no-clobber tests to pass. The export safeguard was retained.

Network dependency resolution failed in this environment (`getaddrinfo() thread failed to start`). Compatible dependencies resolved from the local cache; unavailable newer eframe dependencies were not installed. Compiler incremental-cache hard links are unavailable on this filesystem, so Cargo copies cache files. These host warnings do not imply a GUI performance measurement.

## Pending manual checks

- Launch the packaged EXE on Windows 10/11 x64 with existing graphics drivers; inspect DPI, resize, minimize/restore, modal file dialogs, keyboard focus, long paths and access-denied messages.
- Verify all controls, before/after divider, preview stale indication, zoom/pan, output format/quality, and queue status at typical display sizes. Capture real main-window and Settings screenshots for README.
- Inspect opaque and transparent generated images at image edges, all EXIF orientations, grayscale and palette PNG, and supported JPEGs on real monitors. Test ICC/high-bit-depth/HDR/non-sRGB rejection with representative files.
- Inspect full-resolution exports against preview approximations; evaluate actual denoising/sharpening visually. Measure large-image peak memory and cancellation latency without broad stress tests. Measure idle/active CPU and input responsiveness; compilation alone establishes none of these.
- Verify cancellation during decode/encode, cancellation near final publication, queue continuation after a bad file, existing output names, same stems, unwritable destinations, disk full, and closing the window during work.
- After authorized repository/release publication, verify startup enabled/disabled behavior across restart, manual checks, unavailable/rate-limited/offline checks, correct stable version and asset matching, missing/bad checksum/size rejection, download cancellation, writable/unwritable portable folders, self-replacement, restart, and preserved settings/user data. Use disposable app copies and fixtures.
- Verify a clean machine needs no separately installed Visual C++ runtime and that release licensing notices remain accessible. Audit/sign the actual release before any optional publication.

## Release artifact

Pending completion of the noninteractive release build and package command. No live updater check or replacement has been attempted.
