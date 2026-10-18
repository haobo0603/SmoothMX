#!/bin/bash
set -euo pipefail
SMOOTHMX_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
export MACOSX_DEPLOYMENT_TARGET=13.0
export CLANG_MODULE_CACHE_PATH="$SMOOTHMX_ROOT/apps/macos/.build/ModuleCache"
cd "$SMOOTHMX_ROOT"
cargo build --release --workspace
cd apps/macos
swift build -c release -debug-info-format none --disable-sandbox \
  --cache-path "$PWD/.build/cache" \
  --config-path "$PWD/.build/config" \
  --security-path "$PWD/.build/security"
python3 - <<'PY'
from pathlib import Path
import plistlib, shutil
build = Path('.build')
contents = build / 'SmoothMX.app/Contents'
(contents / 'MacOS').mkdir(parents=True, exist_ok=True)
shutil.copy2(build / 'release/SmoothMX', contents / 'MacOS/SmoothMX')
with (contents / 'Info.plist').open('wb') as f:
    plistlib.dump({
        'CFBundleExecutable': 'SmoothMX', 'CFBundleIdentifier': 'dev.smoothmx.preview',
        'CFBundleName': 'SmoothMX', 'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': '0.1.0', 'CFBundleVersion': '2',
        'LSMinimumSystemVersion': '13.0', 'NSHighResolutionCapable': True
    }, f)
print(f'Built {contents.parent.resolve()}')
PY
# Local ad-hoc signature only; this is not a notarized distribution build.
codesign --force --sign - .build/SmoothMX.app
