#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

bash ./scripts/prepare-runtime.sh
bash ./scripts/prepare-python-vendor.sh

if [[ ! -x "$ROOT_DIR/runtime/bin/ffmpeg" ]]; then
  echo "[standalone-macos] missing runtime/bin/ffmpeg" >&2
  exit 1
fi

if [[ ! -x "$ROOT_DIR/runtime/bin/ffprobe" ]]; then
  echo "[standalone-macos] missing runtime/bin/ffprobe" >&2
  exit 1
fi

if [[ ! -x "$ROOT_DIR/runtime/python-home/bin/python3" && ! -x "$ROOT_DIR/runtime/python-home/bin/python" ]]; then
  echo "[standalone-macos] embedded Python not staged yet." >&2
  echo "Run: TENNIS_EMBEDDED_PYTHON_MACOS_ROOT=/path/to/python-home npm run prepare:embedded-python:mac" >&2
  exit 1
fi

bash ./scripts/check-runtime-bundle.sh macos

npm run tauri:bundle:mac
