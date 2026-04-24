#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

bash ./scripts/prepare-runtime.sh
bash ./scripts/prepare-python-vendor.sh

if [[ ! -f "$ROOT_DIR/runtime/bin/ffmpeg.exe" ]]; then
  echo "[standalone-windows] missing runtime/bin/ffmpeg.exe" >&2
  exit 1
fi

if [[ ! -f "$ROOT_DIR/runtime/python-home/python.exe" && ! -f "$ROOT_DIR/runtime/python-home/bin/python.exe" ]]; then
  echo "[standalone-windows] embedded Python not staged yet." >&2
  echo "Run: TENNIS_EMBEDDED_PYTHON_WINDOWS_ROOT=/path/to/python-embed npm run prepare:embedded-python:windows" >&2
  exit 1
fi

npm run tauri:bundle:windows
