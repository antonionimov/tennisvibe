#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
TARGET_DIR=${TENNIS_VENDOR_TARGET_DIR:-$ROOT_DIR/runtime/python/vendor}
PYTHON_BIN=${TENNIS_VENDOR_PYTHON_BIN:-}
NUMPY_VERSION=${TENNIS_VENDOR_NUMPY_VERSION:-2.2.6}
SCIPY_VERSION=${TENNIS_VENDOR_SCIPY_VERSION:-1.15.3}

if [[ -z "$PYTHON_BIN" ]]; then
  if command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN=$(command -v python3)
  elif command -v python >/dev/null 2>&1; then
    PYTHON_BIN=$(command -v python)
  else
    echo "[prepare-python-vendor] python3/python not found" >&2
    exit 1
  fi
fi

rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"

"$PYTHON_BIN" -m pip install --upgrade pip
"$PYTHON_BIN" -m pip install \
  --only-binary=:all: \
  --no-cache-dir \
  --target "$TARGET_DIR" \
  "numpy==$NUMPY_VERSION" \
  "scipy==$SCIPY_VERSION"

find "$TARGET_DIR" -type d -name '__pycache__' -prune -exec rm -rf {} +
find "$TARGET_DIR" -type f \( -name '*.pyc' -o -name '*.pyo' -o -name '.DS_Store' -o -name '._*' \) -delete

echo "[prepare-python-vendor] staged numpy==$NUMPY_VERSION scipy==$SCIPY_VERSION into $TARGET_DIR"
"$PYTHON_BIN" - <<'PY' "$TARGET_DIR"
import sys
from pathlib import Path
root = Path(sys.argv[1])
sys.path.insert(0, str(root))
import numpy, scipy
print('numpy', numpy.__version__)
print('scipy', scipy.__version__)
for pattern in ['numpy/**/*.so', 'numpy/**/*.pyd', 'scipy/**/*.so', 'scipy/**/*.pyd']:
    hits = list(root.glob(pattern))
    if hits:
        print(pattern, hits[0].name)
PY

du -sh "$TARGET_DIR" 2>/dev/null || true
