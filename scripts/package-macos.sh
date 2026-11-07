#!/usr/bin/env bash
# Build LocalReel.app and a .dmg for macOS. Run this on macOS.
#
#   scripts/package-macos.sh --build-web
#
# The build is ad-hoc signed (`codesign -s -`), which is enough to run locally
# without an Apple Developer account, but Gatekeeper still quarantines a
# downloaded copy: the user right-clicks the app and chooses Open once, or runs
#   xattr -dr com.apple.quarantine /Applications/LocalReel.app
# Notarisation needs an Apple account and is deliberately out of scope.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_DIR="$(cd "$ROOT/../localreel-core" 2>/dev/null && pwd || true)"
WEB_DIR="$(cd "$ROOT/../localreel-web" 2>/dev/null && pwd || true)"
VERSION=""
OUT_DIR="$ROOT/dist"
SKIP_BUILD=0
BUILD_WEB=0

usage() {
  cat <<'EOF'
Usage: scripts/package-macos.sh [options]

  --core <dir>     localreel-core checkout (default: ../localreel-core)
  --web <dir>      localreel-web checkout  (default: ../localreel-web)
  --version <v>    override version (default: core workspace Cargo.toml)
  --out <dir>      output directory (default: product dist/)
  --skip-build     package an existing <core>/target/release/localreel
  --build-web      run npm run build in the web checkout first
  -h, --help       show this help
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --core) CORE_DIR="${2:?missing core dir}"; shift 2 ;;
    --web) WEB_DIR="${2:?missing web dir}"; shift 2 ;;
    --version) VERSION="${2:?missing version}"; shift 2 ;;
    --out) OUT_DIR="${2:?missing out dir}"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --build-web) BUILD_WEB=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo "package-macos.sh must run on macOS" >&2
  exit 1
fi
if [ -z "$CORE_DIR" ] || [ ! -f "$CORE_DIR/Cargo.toml" ]; then
  echo "localreel-core not found; pass --core <dir>" >&2
  exit 1
fi
if [ -z "$WEB_DIR" ] || [ ! -d "$WEB_DIR/dist" ]; then
  echo "localreel-web dist not found; pass --web <dir>" >&2
  exit 1
fi

if [ -z "$VERSION" ]; then
  VERSION="$(awk '
    /^\[workspace.package\]/ { in_section=1; next }
    /^\[/ { in_section=0 }
    in_section && $1 == "version" { gsub(/"|\r/, "", $3); print $3; exit }
  ' "$CORE_DIR/Cargo.toml")"
fi
[ -n "$VERSION" ] || { echo "could not determine version" >&2; exit 1; }

MACHINE="$(uname -m)"
case "$MACHINE" in
  arm64) MAC_ARCH="arm64" ;;
  x86_64) MAC_ARCH="x86_64" ;;
  *) MAC_ARCH="$MACHINE" ;;
esac

if [ "$BUILD_WEB" -eq 1 ]; then
  (cd "$WEB_DIR" && npm run build)
fi

if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "building localreel-server (release)..."
  (cd "$CORE_DIR" && LOCALREEL_WEB_DIR="$WEB_DIR" cargo build --release -p localreel-server)
fi

BINARY="$CORE_DIR/target/release/localreel"
[ -f "$BINARY" ] || { echo "missing $BINARY (run without --skip-build)" >&2; exit 1; }

ICON_PNG="$CORE_DIR/crates/localreel-server/assets/localreel-256.png"
[ -f "$ICON_PNG" ] || { echo "missing icon $ICON_PNG" >&2; exit 1; }

APP_DIR="$OUT_DIR/LocalReel.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

install -m 0755 "$BINARY" "$APP_DIR/Contents/MacOS/localreel"
sed "s/__VERSION__/$VERSION/g" "$ROOT/packaging/macos/Info.plist" \
  > "$APP_DIR/Contents/Info.plist"
printf 'APPL????' > "$APP_DIR/Contents/PkgInfo"

# .icns from the single 256px PNG: iconutil needs the full size ladder.
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 64 128 256 512; do
  sips -z "$size" "$size" "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$ICON_PNG" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP_DIR/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$ICONSET")"

# Ad-hoc signature: without it macOS refuses to run the bundle on Apple silicon.
codesign --force --deep --sign - --timestamp=none "$APP_DIR" >/dev/null

DMG="$OUT_DIR/localreel-$VERSION-$MAC_ARCH.dmg"
rm -f "$DMG"
STAGE="$(mktemp -d)/dmg"
mkdir -p "$STAGE"
cp -R "$APP_DIR" "$STAGE/"
cp "$ROOT/packaging/macos/com.localreel.server.plist" "$STAGE/"
cp "$ROOT/docs/macos.md" "$STAGE/README-macos.md" 2>/dev/null || true
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "LocalReel $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$(dirname "$STAGE")"

echo "App: $APP_DIR"
echo "DMG: $DMG"
echo "Size: $(du -h "$DMG" | cut -f1)"
