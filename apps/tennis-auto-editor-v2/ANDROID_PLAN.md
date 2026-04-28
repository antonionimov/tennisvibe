# Android Plan — Tennis Auto Editor V2

## Current direction
Continue from the existing Tauri v2 codebase (方案 A), not a separate Android-only rewrite.

## What is already prepared
- Added runtime capability reporting from Rust to frontend.
- Added `suggest_export_path` so Android can save into app-managed `exports/` without depending on desktop-style save dialogs.
- Added `file://` URI normalization for selected input videos.
- Added Tauri `fs` plugin wiring plus frontend import flow that copies picker-returned URIs into `AppData/imports/...` before processing.
- Added explicit error messaging for non-file URIs so Android import failures are diagnosable instead of vague if the import layer is bypassed.
- Added Android-oriented runtime materialization: on app startup, if bundled `runtime/` exists, copy it into `AppData/mobile-runtime/` and mark `bin/*` executable before later ffmpeg/python lookup.
- Added runtime binary env wiring for both `ffmpeg` and `ffprobe`, so mobile/bundled runtime can cover both media execution and initial video probing.
- Added runtime diagnostics into `get_runtime_capabilities` + frontend UI, so the app now surfaces:
  - analyzer backend
  - runtime source (`bundled-mobile-runtime` / `bundled-runtime` / `system-path`)
  - ffmpeg / ffprobe resolved binary path
  - whether the media pipeline is actually ready before starting work
- Added npm Android entry scripts:
  - `npm run tauri:android:init`
  - `npm run tauri:android:dev`
  - `npm run tauri:android:build:debug`
  - `npm run tauri:android:build:release`
- Added **Rust MVP analyzer** and switched the main `run_analysis` path from Python process spawn to native Rust audio analysis.
  - Current MVP behavior: WAV read -> energy/peak transient detection -> hit clustering -> candidate segment generation.
  - Existing frontend/result JSON contract is preserved.
  - Visual bootstrap / ball-model parameters are currently accepted but skipped on this MVP path.
- Current verification after these changes: `npm run build` ✅, `cargo check` ✅.

## Main blockers still remaining
1. **Android input video import**
   - First-pass URI import layer is now wired on the frontend using Tauri `plugin-fs` copy into app storage.
   - Still needs real-device validation against Android picker behavior and permission edge cases.

2. **Bundled runtime on Android**
   - Current app no longer has to execute directly from APK resource paths first; startup now tries to materialize bundled runtime into writable app storage.
   - Analyzer side has now moved onto a Rust MVP main path, so Android no longer depends on shipping the Python analyzer first.
   - Still unresolved: whether the copied binaries/libs are actually Android-compatible and complete enough for ffmpeg/ffprobe/export usage.
   - Need an Android-compatible packaging story mainly for:
     - ffmpeg binary or mobile-native media pipeline
     - any remaining optional Python/vision assets if we choose to keep them as non-default extras

3. **Desktop assumptions in UX**
   - Export success currently means “saved inside app exports dir” on Android, not yet “visible in gallery / Downloads”.
   - Later step: share/export-to-public-location flow.

4. **Build environment**
   - Local Android build environment is now working on this machine.
   - Confirmed available:
     - `JAVA_HOME=/home/antonioni/.local/jdk-17`
     - `ANDROID_HOME=/home/antonioni/Android/Sdk`
     - Rust Android targets installed (`aarch64`, `armv7`, `i686`, `x86_64`)
   - Confirmed outputs:
     - Debug APK: `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`
     - Release unsigned APK: `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk`
     - Release AAB: `src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`
   - Note: the installable artifact right now is the **debug APK**; the release APK is still unsigned.

## Recommended execution order
1. Real-device test the URI import path (`content://` -> `AppData/imports/...`).
2. Improve/validate the Rust analyzer MVP on real match audio.
3. Validate whether ffmpeg can run from the materialized mobile runtime, otherwise switch to a different mobile media path.
4. Add user-visible share/save-to-gallery flow.
5. Produce a signed release APK for broader distribution.
