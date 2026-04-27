#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
RUNTIME_DIR="$ROOT_DIR/runtime"
PLATFORM=${1:-${TENNIS_RUNTIME_PLATFORM:-auto}}

if [[ "$PLATFORM" == "auto" ]]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) PLATFORM=windows ;;
    Darwin*) PLATFORM=macos ;;
    *) PLATFORM=linux ;;
  esac
fi

failures=()

require_file() {
  local path=$1
  local label=${2:-$path}
  if [[ ! -f "$path" ]]; then
    failures+=("missing file: $label ($path)")
  fi
}

require_dir() {
  local path=$1
  local label=${2:-$path}
  if [[ ! -d "$path" ]]; then
    failures+=("missing dir: $label ($path)")
  fi
}

require_any_file() {
  local label=$1
  shift
  local path
  for path in "$@"; do
    if [[ -f "$path" ]]; then
      return 0
    fi
  done
  failures+=("missing file: $label (tried: $*)")
}

require_any_executable() {
  local label=$1
  shift
  local path
  for path in "$@"; do
    if [[ -x "$path" ]]; then
      return 0
    fi
  done
  failures+=("missing executable: $label (tried: $*)")
}

require_dir "$RUNTIME_DIR" "runtime root"
require_file "$ROOT_DIR/src-tauri/tauri.conf.json" "Tauri config"

if ! grep -q '"../runtime"' "$ROOT_DIR/src-tauri/tauri.conf.json"; then
  failures+=("Tauri config does not include ../runtime as bundled resource")
fi

require_file "$RUNTIME_DIR/python/analyzer/main.py" "Python analyzer main.py"
require_file "$RUNTIME_DIR/python/analyzer/audio_features.py" "Python audio analyzer features"
require_file "$RUNTIME_DIR/python/analyzer/scoring.py" "Python analyzer scoring"
require_file "$RUNTIME_DIR/python/analyzer/segment_logic.py" "Python analyzer segment logic"
require_dir "$RUNTIME_DIR/python/vendor/numpy" "runtime numpy vendor"
require_dir "$RUNTIME_DIR/python/vendor/scipy" "runtime scipy vendor"

case "$PLATFORM" in
  windows)
    require_file "$RUNTIME_DIR/bin/ffmpeg.exe" "Windows bundled ffmpeg.exe"
    require_file "$RUNTIME_DIR/bin/ffprobe.exe" "Windows bundled ffprobe.exe"
    require_any_file "Windows embedded python.exe" \
      "$RUNTIME_DIR/python-home/python.exe" \
      "$RUNTIME_DIR/python-home/bin/python.exe"
    ;;
  macos|linux)
    require_any_executable "$PLATFORM bundled ffmpeg" \
      "$RUNTIME_DIR/bin/ffmpeg"
    require_any_executable "$PLATFORM bundled ffprobe" \
      "$RUNTIME_DIR/bin/ffprobe"
    require_any_executable "$PLATFORM embedded python" \
      "$RUNTIME_DIR/python-home/bin/python3" \
      "$RUNTIME_DIR/python-home/bin/python"
    ;;
  *)
    failures+=("unknown runtime platform: $PLATFORM")
    ;;
esac

if (( ${#failures[@]} > 0 )); then
  echo "[check-runtime-bundle] runtime bundle validation failed for platform=$PLATFORM" >&2
  printf ' - %s\n' "${failures[@]}" >&2
  exit 1
fi

echo "[check-runtime-bundle] runtime bundle OK for platform=$PLATFORM"
du -sh "$RUNTIME_DIR" "$RUNTIME_DIR/bin" "$RUNTIME_DIR/python" "$RUNTIME_DIR/python-home" 2>/dev/null || true

