#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
TARGET_DIR="$ROOT_DIR/runtime/python-home"
SOURCE_INPUT=${TENNIS_EMBEDDED_PYTHON_LINUX_ROOT:-${1:-}}
TEMP_EXTRACT_DIR=

cleanup() {
  if [[ -n "$TEMP_EXTRACT_DIR" && -d "$TEMP_EXTRACT_DIR" ]]; then
    rm -rf "$TEMP_EXTRACT_DIR"
  fi
}

trap cleanup EXIT

if [[ -z "$SOURCE_INPUT" ]]; then
  echo "Usage: TENNIS_EMBEDDED_PYTHON_LINUX_ROOT=/path/to/python-home npm run prepare:embedded-python:linux" >&2
  echo "   or: bash ./scripts/prepare-embedded-python-linux.sh /path/to/python-home-or-install_only.tar.gz" >&2
  exit 1
fi

SOURCE_DIR="$SOURCE_INPUT"

if [[ -f "$SOURCE_INPUT" ]]; then
  TEMP_EXTRACT_DIR=$(mktemp -d)
  tar -xzf "$SOURCE_INPUT" -C "$TEMP_EXTRACT_DIR"
  SOURCE_DIR="$TEMP_EXTRACT_DIR/python"
fi

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "[embedded-python:linux] source directory not found: $SOURCE_INPUT" >&2
  exit 1
fi

rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"

if command -v rsync >/dev/null 2>&1; then
  rsync -a "$SOURCE_DIR/" "$TARGET_DIR/"
else
  cp -a "$SOURCE_DIR"/. "$TARGET_DIR"/
fi

if [[ ! -x "$TARGET_DIR/bin/python3" && ! -x "$TARGET_DIR/bin/python" ]]; then
  echo "[embedded-python:linux] expected python binary under $TARGET_DIR/bin" >&2
  exit 1
fi

echo "[embedded-python:linux] staged embedded Python into $TARGET_DIR"
find "$TARGET_DIR" -maxdepth 2 | sort | sed -n '1,60p'
