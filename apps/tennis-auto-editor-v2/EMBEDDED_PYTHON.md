# Embedded Python Plan

## Goal

Ship standalone installers for:
- macOS
- Windows
- Ubuntu/Linux

with the same app-side runtime lookup contract:

```text
runtime/
  bin/
    python3 | python.exe
    ffmpeg | ffmpeg.exe
  python/
    analyzer/
    vendor/
```

The Tauri app already prefers these bundled runtime paths automatically.

Important: embedded Python alone is not enough.
The vendored binary packages under `runtime/python/vendor/` must also match the target platform and ABI.
For this project today, `numpy` and `scipy` should be regenerated on each target OS (or matching CI runner) with `npm run prepare:python-vendor`.

## Current product reality

Current `tennis-auto-editor-v2` default flow is **audio-only**.
It does not need the old visual model stack in the shipped installer.

That means the shared minimum resource set should be:
- analyzer audio-only files
- `numpy`
- `scipy`
- `ffmpeg`
- embedded Python runtime per platform

## Windows strategy

Recommended source:
- official **Python embeddable package** from python.org

Packaging target:
- stage the extracted embeddable package into `runtime/python-home/`
- app runtime lookup already supports `runtime/python-home/python.exe`
- keep app-specific analyzer code in `runtime/python/`
- keep third-party packages in `runtime/python/vendor/`

Notes:
- Windows is the easiest desktop target for true embedded Python because python.org provides an embeddable distribution directly.
- `ffmpeg.exe` should also be placed in `runtime/bin/`.

## macOS strategy

Recommended source:
- a relocatable CPython build such as **python-build-standalone**

Packaging target:
- stage the relocatable Python install root into `runtime/python-home/`
- app runtime lookup already supports `runtime/python-home/bin/python3`
- keep analyzer code in `runtime/python/`
- keep third-party packages in `runtime/python/vendor/`

Notes:
- macOS usually needs the cleanest path handling and signing/notarization checks later.
- build the macOS installer on macOS.

## Ubuntu / Linux strategy

Recommended source:
- a relocatable CPython build, ideally the same family used for macOS if possible
- alternatively, a distro-compatible copied interpreter/runtime tree for Ubuntu releases you support

Packaging target:
- stage the relocatable Python install root into `runtime/python-home/`
- app runtime lookup already supports `runtime/python-home/bin/python3`
- keep analyzer code in `runtime/python/`
- keep third-party packages in `runtime/python/vendor/`

Notes:
- Linux standalone packaging is already partially validated on the app side.
- the remaining missing piece is a portable embedded Python runtime, not the Tauri wiring.

## Shared minimum resource set

For all three platforms, the default standalone package should avoid bundling:
- `tennis_vision_ball.pt`
- court keypoint models
- `torch`
- `torchvision`
- `opencv`
- `ultralytics`

unless visual bootstrap is promoted back into the shipped default product path.

## Execution order

1. Keep `audio-only` runtime as the default staging profile
2. Lock Ubuntu standalone package around that minimal runtime
3. Prepare Windows embedded Python runtime
4. Prepare macOS embedded Python runtime
5. Run per-platform packaging on the matching OS
6. Only after that, consider a separate optional `full-vision` build flavor

## Helper commands

```bash
# Linux
TENNIS_EMBEDDED_PYTHON_LINUX_ROOT=/path/to/python-home npm run prepare:embedded-python:linux

# Linux also accepts a python-build-standalone install_only tarball directly
bash ./scripts/prepare-embedded-python-linux.sh /path/to/cpython-3.10.x-x86_64-unknown-linux-gnu-install_only.tar.gz

# macOS
TENNIS_EMBEDDED_PYTHON_MACOS_ROOT=/path/to/python-home npm run prepare:embedded-python:mac

# macOS also accepts a python-build-standalone install_only tarball directly
bash ./scripts/prepare-embedded-python-macos.sh /path/to/cpython-3.10.x-x86_64-apple-darwin-install_only.tar.gz

# Windows embeddable package extracted dir
TENNIS_EMBEDDED_PYTHON_WINDOWS_ROOT=/path/to/python-embed npm run prepare:embedded-python:windows

# Windows helper also accepts the official embeddable zip directly
bash ./scripts/prepare-embedded-python-windows.sh /path/to/python-3.10.x-embed-amd64.zip
```

## Current Linux proof point

Validated on Ubuntu packaging with:
- `python-build-standalone` CPython `3.10.20`
- asset: `cpython-3.10.20+20260414-x86_64-unknown-linux-gnu-install_only.tar.gz`

Why `3.10` right now:
- the currently vendored `numpy/scipy` wheels inside `runtime/python/vendor/` are built for `cpython-310`
- matching the embedded interpreter ABI avoids binary extension mismatch
