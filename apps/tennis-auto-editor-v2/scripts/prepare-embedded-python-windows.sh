#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
TARGET_DIR="$ROOT_DIR/runtime/python-home"
SOURCE_INPUT=${TENNIS_EMBEDDED_PYTHON_WINDOWS_ROOT:-${1:-}}
TEMP_EXTRACT_DIR=

cleanup() {
  if [[ -n "$TEMP_EXTRACT_DIR" && -d "$TEMP_EXTRACT_DIR" ]]; then
    rm -rf "$TEMP_EXTRACT_DIR"
  fi
}

trap cleanup EXIT

if [[ -z "$SOURCE_INPUT" ]]; then
  echo "Usage: TENNIS_EMBEDDED_PYTHON_WINDOWS_ROOT=/path/to/extracted-embedded-python npm run prepare:embedded-python:windows" >&2
  echo "   or: bash ./scripts/prepare-embedded-python-windows.sh /path/to/extracted-embedded-python-or-python-embed.zip" >&2
  echo "Expected source: official Python embeddable package extracted directory or zip" >&2
  exit 1
fi

SOURCE_DIR="$SOURCE_INPUT"

if [[ -f "$SOURCE_INPUT" ]]; then
  TEMP_EXTRACT_DIR=$(mktemp -d)
  if [[ "$SOURCE_INPUT" == *.zip ]]; then
    python3 - <<'PY' "$SOURCE_INPUT" "$TEMP_EXTRACT_DIR"
import sys, zipfile
archive, dest = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive) as zf:
    zf.extractall(dest)
PY
    SOURCE_DIR="$TEMP_EXTRACT_DIR"
  else
    python3 - <<'PY' "$SOURCE_INPUT" "$TEMP_EXTRACT_DIR"
import sys, tarfile
archive, dest = sys.argv[1], sys.argv[2]
with tarfile.open(archive, 'r:gz') as tf:
    tf.extractall(dest)
PY
    SOURCE_DIR="$TEMP_EXTRACT_DIR/python"
  fi
fi

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "[embedded-python:windows] source directory not found: $SOURCE_INPUT" >&2
  exit 1
fi

rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"

if command -v rsync >/dev/null 2>&1; then
  rsync -a "$SOURCE_DIR/" "$TARGET_DIR/"
else
  cp -a "$SOURCE_DIR"/. "$TARGET_DIR"/
fi

if [[ ! -f "$TARGET_DIR/python.exe" ]]; then
  echo "[embedded-python:windows] expected python.exe at $TARGET_DIR/python.exe" >&2
  exit 1
fi

echo "[embedded-python:windows] staged embedded Python into $TARGET_DIR"
find "$TARGET_DIR" -maxdepth 2 | sort | sed -n '1,60p'
