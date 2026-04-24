# Packaging Notes

## Current strategy

Use a **thin desktop bundle** first to keep installer size small.

What is already optimized:
- Rust release profile uses `opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`
- Tauri desktop bundle is enabled
- Desktop icon set is generated for macOS / Windows / Linux packaging
- Platform-specific bundle scripts are available in `package.json`
- Tauri bundle now includes `../runtime` as installer resources
- `npm run prepare:runtime` stages analyzer assets into `runtime/`
- default runtime staging profile is now `audio-only`, matching the current v2 product path

## Build commands

Run on the matching target OS:

```bash
# stage runtime resources first
npm run prepare:runtime

# then generate platform-matching numpy/scipy vendor wheels with the current host Python
npm run prepare:python-vendor

# for a true standalone Ubuntu build, also stage embedded Python
TENNIS_EMBEDDED_PYTHON_LINUX_ROOT=/path/to/python-home npm run prepare:embedded-python:linux

# the Linux helper also accepts a python-build-standalone install_only tarball path
bash ./scripts/prepare-embedded-python-linux.sh /path/to/cpython-3.10.x-x86_64-unknown-linux-gnu-install_only.tar.gz
```

```bash
# macOS
npm run tauri:bundle:mac
npm run tauri:bundle:mac:standalone

# Windows
npm run tauri:bundle:windows
npm run tauri:bundle:windows:standalone

# Ubuntu / Debian
npm run tauri:bundle:ubuntu

# Ubuntu standalone helper
npm run tauri:bundle:ubuntu:standalone
```

## Verified so far

On Linux, this successfully produced:

```text
src-tauri/target/release/bundle/deb/Tennis Auto Editor V2_0.1.0_amd64.deb
```

Observed sizes during validation:
- release binary: about `4.1MB`
- deb bundle: about `1.9MB`

Observed sizes after staging a standalone-style Linux runtime bundle:
- old `full` runtime experiment: staged `runtime/` about `1.4GB`, resulting Ubuntu `.deb` about `569MB`
- current `audio-only` runtime: staged `runtime/` about `190MB`, resulting Ubuntu `.deb` about `58MB`
- current `audio-only + embedded python-home` runtime: staged `runtime/` about `316MB`, resulting Ubuntu `.deb` about `140MB`

## Important runtime dependency blocker

The app is **not fully self-contained yet**.

Good news first: the runtime lookup code now supports a bundled layout under the app resources directory and will automatically prefer these paths when present:

- `runtime/python/`
- `runtime/bin/python3` or `runtime/bin/python.exe`
- `runtime/bin/ffmpeg` or `runtime/bin/ffmpeg.exe`

For local staging, use:

```bash
npm run prepare:runtime
```

Current runtime still depends on external tools/resources unless those bundled files are provided:
- `python3`
- `ffmpeg`
- analyzer code under `apps/tennis-auto-editor/python`

Important nuance from current testing:
- bundling analyzer resources + ffmpeg already works and produces a much larger installer
- the app still needs a proper per-platform embedded Python strategy before we can call the installers truly standalone across different machines
- current `audio-only` product path does **not** need the legacy visual model `tennis_vision_ball.pt`, and should avoid bundling visual ML weights/deps unless visual bootstrap is re-enabled as a shipped feature
- the app now also supports a bundled `runtime/python-home` directory for relocatable embedded Python distributions
- analyzer invocation is now done via `python -m analyzer.main`, which avoids the `analyzer/types.py` vs stdlib `types` import collision that showed up under embedded Python
- `numpy/scipy` vendor binaries must be generated per platform. Linux-built `.so` wheels cannot be reused on Windows or macOS, so standalone packaging now regenerates vendor packages with the current host Python via `npm run prepare:python-vendor`

## Latest Ubuntu standalone validation

Validated on Linux with `python-build-standalone` CPython `3.10.20`:

- staged `runtime/python-home` from `cpython-3.10.20+20260414-x86_64-unknown-linux-gnu-install_only.tar.gz`
- built via `npm run tauri:bundle:ubuntu:standalone`
- extracted the resulting `.deb`
- confirmed packaged resources include:
  - `runtime/python-home/bin/python3`
  - `runtime/python/analyzer/main.py`
  - `runtime/bin/ffmpeg`
- confirmed the extracted bundled interpreter can run:

```bash
PYTHONHOME=.../runtime/python-home \
PYTHONPATH=.../runtime/python/vendor:.../runtime/python \
./runtime/python-home/bin/python3 -m analyzer.main --help
```

## CI packaging

A GitHub Actions workflow is now included at:

```text
.github/workflows/tennis-auto-editor-v2-packaging.yml
```

It uses GitHub-hosted runners to build standalone test installers for:
- Ubuntu (`.deb`)
- macOS (`.dmg`)
- Windows (`NSIS .exe`)

This is the recommended path if you do not have Windows or macOS hardware locally.

That means the installer can be generated now, but a packaged app on another machine will not work correctly until we choose one of these directions:

1. **Thin installer**
   - require the user machine to have `python3` + `ffmpeg`
   - ship or separately install the analyzer directory
   - smallest package size

2. **Fat installer**
   - bundle ffmpeg and analyzer resources
   - much larger package size
   - needs cross-platform resource path handling and dependency bundling

## Recommended next step

Before building macOS and Windows release installers, decide whether this product should ship as:
- a **small installer with external runtime dependencies**, or
- a **bigger standalone installer**.
