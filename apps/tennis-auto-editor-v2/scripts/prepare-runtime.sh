#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
RUNTIME_DIR="$ROOT_DIR/runtime"
RUNTIME_BIN_DIR="$RUNTIME_DIR/bin"
RUNTIME_PYTHON_DIR="$RUNTIME_DIR/python"
RUNTIME_VENDOR_DIR="$RUNTIME_PYTHON_DIR/vendor"
DEFAULT_SOURCE_PYTHON_ROOT=$(cd "$ROOT_DIR/../tennis-auto-editor/python" 2>/dev/null && pwd || true)
SOURCE_PYTHON_ROOT=${TENNIS_STANDALONE_SOURCE_PYTHON_ROOT:-$DEFAULT_SOURCE_PYTHON_ROOT}
FFMPEG_BIN=${TENNIS_STANDALONE_FFMPEG_BIN:-$(command -v ffmpeg || true)}
FFPROBE_BIN=${TENNIS_STANDALONE_FFPROBE_BIN:-$(command -v ffprobe || true)}
PYTHON_BIN=${TENNIS_STANDALONE_PYTHON_BIN:-}
RUNTIME_PROFILE=${TENNIS_STANDALONE_RUNTIME_PROFILE:-audio-only}

is_windows_host() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) return 0 ;;
    *) return 1 ;;
  esac
}

resolve_windows_real_binary() {
  local tool_name=$1
  local candidate=$2

  if [[ -z "$candidate" ]]; then
    return 0
  fi

  local normalized=${candidate//\\//}
  if [[ "$normalized" == */Chocolatey/bin/* ]]; then
    local choco_lib="$(cd "$(dirname "$normalized")/../lib" 2>/dev/null && pwd || true)"
    if [[ -n "$choco_lib" ]]; then
      local real_candidate
      real_candidate=$(find "$choco_lib" -type f \( -iname "$tool_name.exe" -o -iname "$tool_name" \) | grep -vi '/bin/' | head -n 1 || true)
      if [[ -n "$real_candidate" ]]; then
        echo "$real_candidate"
        return 0
      fi
    fi
  fi

  echo "$candidate"
}

copy_runtime_binary() {
  local label=$1
  local src=$2
  local default_dest_name=$3

  if [[ -z "$src" || ! -f "$src" ]]; then
    echo "[prepare-runtime] $label not copied. Set TENNIS_STANDALONE_${label^^}_BIN if needed."
    return 0
  fi

  local dest_name=$default_dest_name
  if is_windows_host; then
    src=$(resolve_windows_real_binary "$label" "$src")
    if [[ ! -f "$src" ]]; then
      echo "[prepare-runtime] resolved $label binary missing after shim resolution: $src" >&2
      exit 1
    fi
    dest_name="$label.exe"
  fi

  cp "$src" "$RUNTIME_BIN_DIR/$dest_name"
  echo "[prepare-runtime] copied $label -> $RUNTIME_BIN_DIR/$dest_name (from $src)"
}

mkdir -p "$RUNTIME_BIN_DIR"
rm -rf "$RUNTIME_PYTHON_DIR"
mkdir -p "$RUNTIME_VENDOR_DIR"

if [[ ! -f "$SOURCE_PYTHON_ROOT/analyzer/main.py" ]]; then
  echo "[prepare-runtime] analyzer source not found: $SOURCE_PYTHON_ROOT" >&2
  exit 1
fi

copy_tree() {
  local src=$1
  local dest=$2
  if command -v rsync >/dev/null 2>&1; then
    rsync -a \
      --exclude '.venv' \
      --exclude '.DS_Store' \
      --exclude '._*' \
      --exclude '__pycache__' \
      --exclude '*.pyc' \
      --exclude '*.pyo' \
      "$src/" "$dest/"
  else
    cp -a "$src"/. "$dest"/
    find "$dest" -type d -name '__pycache__' -prune -exec rm -rf {} +
    find "$dest" -type f \( -name '*.pyc' -o -name '*.pyo' \) -delete
    find "$dest" -type f \( -name '.DS_Store' -o -name '._*' \) -delete
    rm -rf "$dest/.venv"
  fi
}

mkdir -p "$RUNTIME_PYTHON_DIR/analyzer"

copy_file() {
  local src=$1
  local dest=$2
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
}

copy_file "$SOURCE_PYTHON_ROOT/analyzer/__init__.py" "$RUNTIME_PYTHON_DIR/analyzer/__init__.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/main.py" "$RUNTIME_PYTHON_DIR/analyzer/main.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/audio_features.py" "$RUNTIME_PYTHON_DIR/analyzer/audio_features.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/io_utils.py" "$RUNTIME_PYTHON_DIR/analyzer/io_utils.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/scoring.py" "$RUNTIME_PYTHON_DIR/analyzer/scoring.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/segment_logic.py" "$RUNTIME_PYTHON_DIR/analyzer/segment_logic.py"
copy_file "$SOURCE_PYTHON_ROOT/analyzer/types.py" "$RUNTIME_PYTHON_DIR/analyzer/types.py"

if [[ "$RUNTIME_PROFILE" == "full" ]]; then
  if [[ -d "$SOURCE_PYTHON_ROOT/models" ]]; then
    mkdir -p "$RUNTIME_PYTHON_DIR/models"
    copy_tree "$SOURCE_PYTHON_ROOT/models" "$RUNTIME_PYTHON_DIR/models"
  fi
  if [[ -d "$SOURCE_PYTHON_ROOT/vendor" ]]; then
    copy_tree "$SOURCE_PYTHON_ROOT/vendor" "$RUNTIME_VENDOR_DIR"
  fi
  echo "[prepare-runtime] using full runtime profile"
else
  echo "[prepare-runtime] using audio-only runtime profile"
  keep_vendor_items=(
    numpy
    numpy.libs
    scipy
    scipy.libs
  )

  for item in "${keep_vendor_items[@]}"; do
    if [[ -e "$SOURCE_PYTHON_ROOT/vendor/$item" ]]; then
      if [[ -d "$SOURCE_PYTHON_ROOT/vendor/$item" ]]; then
        mkdir -p "$RUNTIME_VENDOR_DIR/$item"
        copy_tree "$SOURCE_PYTHON_ROOT/vendor/$item" "$RUNTIME_VENDOR_DIR/$item"
      else
        cp "$SOURCE_PYTHON_ROOT/vendor/$item" "$RUNTIME_VENDOR_DIR/$item"
      fi
    fi
  done

  rm -rf "$RUNTIME_PYTHON_DIR/models"
fi

copy_runtime_binary "ffmpeg" "$FFMPEG_BIN" "$(basename "$FFMPEG_BIN")"
copy_runtime_binary "ffprobe" "$FFPROBE_BIN" "$(basename "$FFPROBE_BIN")"

if [[ -n "$PYTHON_BIN" && -f "$PYTHON_BIN" ]]; then
  cp "$PYTHON_BIN" "$RUNTIME_BIN_DIR/$(basename "$PYTHON_BIN")"
  echo "[prepare-runtime] copied python -> $RUNTIME_BIN_DIR/$(basename "$PYTHON_BIN")"
else
  echo "[prepare-runtime] python binary not copied by default. Set TENNIS_STANDALONE_PYTHON_BIN if you want to stage one for local testing."
fi

echo "[prepare-runtime] runtime staged at $RUNTIME_DIR"
find "$RUNTIME_DIR" -type f \( -name '.DS_Store' -o -name '._*' \) -delete
du -sh "$RUNTIME_DIR" "$RUNTIME_BIN_DIR" "$RUNTIME_PYTHON_DIR" "$RUNTIME_VENDOR_DIR" 2>/dev/null || true
find "$RUNTIME_DIR" -maxdepth 2 -mindepth 1 | sort
