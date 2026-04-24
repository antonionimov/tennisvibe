# Bundled Runtime Layout

This directory is reserved for files that should ship inside standalone installers.

Expected layout:

```text
runtime/
  bin/
    ffmpeg            # or ffmpeg.exe
    python3           # or python.exe, optional during staging
  python-home/        # optional embedded Python runtime root
  python/
    analyzer/
    models/
    vendor/
```

Notes:
- The app now prefers bundled runtime files from the installer resources directory when present.
- During local development you can stage this directory with:

```bash
npm run prepare:runtime
```

Optional environment variables for staging:
- `TENNIS_STANDALONE_SOURCE_PYTHON_ROOT`
- `TENNIS_STANDALONE_FFMPEG_BIN`
- `TENNIS_STANDALONE_PYTHON_BIN`
- `TENNIS_STANDALONE_RUNTIME_PROFILE=audio-only|full`

Embedded Python staging helpers:
- `npm run prepare:embedded-python:linux`
- `npm run prepare:embedded-python:mac`
- `npm run prepare:embedded-python:windows`

Vendor staging helper:
- `npm run prepare:python-vendor`
- this installs platform-matching `numpy` / `scipy` wheels into `runtime/python/vendor`
- do this on the same OS family that will produce the installer

Linux note:
- `prepare-embedded-python-linux.sh` can stage either an extracted relocatable Python directory or a `python-build-standalone` `install_only.tar.gz` archive directly.

macOS note:
- `prepare-embedded-python-macos.sh` can stage either an extracted relocatable Python directory or a `python-build-standalone` `install_only.tar.gz` archive directly.

Windows note:
- `prepare-embedded-python-windows.sh` can stage either an extracted embeddable Python directory or the official embeddable `.zip` directly.

Current status:
- analyzer resources can be staged here now
- ffmpeg can be copied here now
- default staging mode is now `audio-only`, which intentionally excludes visual models and heavyweight visual ML dependencies
- `full` mode is only for experiments where visual bootstrap assets must be bundled too
- latest measured `audio-only` staging size is about `190MB`
- a truly portable embedded Python runtime still needs a per-platform distribution strategy
