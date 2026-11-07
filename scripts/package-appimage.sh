#!/usr/bin/env bash
# Build a single-file AppImage for the Linux desktop.
#
#   scripts/package-appimage.sh --build-web
#
# The AppImage wraps the same binary and desktop entry as the .deb. It needs
# no installation: chmod +x and run. appimagetool is downloaded on first use
# and cached under dist/.tools.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_DIR="$(cd "$ROOT/../localreel-core" 2>/dev/null && pwd || true)"
WEB_DIR="$(cd "$ROOT/../localreel-web" 2>/dev/null && pwd || true)"
VERSION=""
OUT_DIR="$ROOT/dist"
SKIP_BUILD=0
BUILD_WEB=0
TOOLS_DIR="$OUT_DIR/.tools"

usage() {
  cat <<'EOF'
Usage: scripts/package-appimage.sh [options]

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
    --out) OUT_DIR="${2:?missing output dir}"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --build-web) BUILD_WEB=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ "$(uname -s)" != "Linux" ]; then
  echo "package-appimage.sh must run on Linux" >&2
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
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) ARCH="$MACHINE" ;;
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

ICON="$CORE_DIR/crates/localreel-server/assets/localreel-256.png"
[ -f "$ICON" ] || { echo "missing icon $ICON" >&2; exit 1; }

APPIMAGETOOL="$TOOLS_DIR/appimagetool-$ARCH.AppImage"
if [ ! -x "$APPIMAGETOOL" ]; then
  mkdir -p "$TOOLS_DIR"
  echo "downloading appimagetool..."
  curl -fsSL "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage" \
    -o "$APPIMAGETOOL"
  chmod +x "$APPIMAGETOOL"
fi

APPDIR="$OUT_DIR/LocalReel.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps"

install -m 0755 "$BINARY" "$APPDIR/usr/bin/localreel"
install -m 0644 "$ICON" "$APPDIR/usr/share/icons/hicolor/256x256/apps/localreel.png"
install -m 0644 "$ICON" "$APPDIR/localreel.png"
cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/localreel.png" "$APPDIR/.DirIcon"

cat > "$APPDIR/localreel.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=LocalReel
GenericName=Media Server
Comment=Local-first media server with a browser interface
Exec=localreel serve
Terminal=true
Icon=localreel
Categories=AudioVideo;Video;Player;
Keywords=media;video;server;stream;
X-AppImage-Name=LocalReel
EOF
install -m 0644 "$APPDIR/localreel.desktop" "$APPDIR/usr/share/applications/localreel.desktop"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
# AppImage entry point: hand every argument to the server binary.
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/localreel" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

APPIMAGE="$OUT_DIR/localreel-$VERSION-$ARCH.AppImage"
rm -f "$APPIMAGE"
# FUSE is not always available (containers, some WSL builds): the tool and the
# generated image both accept --appimage-extract-and-run.
ARCH="$ARCH" "$APPIMAGETOOL" --appimage-extract-and-run --comp zstd --mksquashfs-opt -noappend \
  "$APPDIR" "$APPIMAGE" >/dev/null
chmod +x "$APPIMAGE"
rm -rf "$APPDIR"

echo "AppImage: $APPIMAGE"
echo "Size: $(du -h "$APPIMAGE" | cut -f1)"
