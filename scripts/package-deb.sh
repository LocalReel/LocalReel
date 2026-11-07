#!/usr/bin/env bash
# Build a Debian package for LocalReel: the server with its systemd unit plus a
# desktop menu entry. Run on Linux with dpkg-deb available, or on a machine that
# only needs to produce the archive from an existing binary (--skip-build).
#
#   scripts/package-deb.sh --core ../localreel-core --web ../localreel-web
#
# Layout:
#   /usr/bin/localreel
#   /lib/systemd/system/localreel.service
#   /usr/share/applications/localreel.desktop
#   /usr/share/icons/hicolor/256x256/apps/localreel.png
#   /usr/share/doc/localreel/{README.md,LICENSE,docs/…}
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
Usage: scripts/package-deb.sh [options]

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
  echo "package-deb.sh must run on Linux" >&2
  exit 1
fi
if ! command -v dpkg-deb >/dev/null 2>&1; then
  echo "dpkg-deb not found; install dpkg-dev" >&2
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
if [ -z "$VERSION" ]; then
  echo "could not determine version" >&2
  exit 1
fi

MACHINE="$(uname -m)"
case "$MACHINE" in
  x86_64|amd64) DEB_ARCH="amd64" ;;
  aarch64|arm64) DEB_ARCH="arm64" ;;
  *) DEB_ARCH="$MACHINE" ;;
esac

if [ "$BUILD_WEB" -eq 1 ]; then
  (cd "$WEB_DIR" && npm run build)
fi

if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "building localreel-server (release)..."
  (cd "$CORE_DIR" && LOCALREEL_WEB_DIR="$WEB_DIR" cargo build --release -p localreel-server)
fi

BINARY="$CORE_DIR/target/release/localreel"
if [ ! -f "$BINARY" ]; then
  echo "missing $BINARY (run without --skip-build)" >&2
  exit 1
fi

PKG="localreel_${VERSION}_${DEB_ARCH}"
BUILD_DIR="$OUT_DIR/$PKG"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/bin"
mkdir -p "$BUILD_DIR/lib/systemd/system"
mkdir -p "$BUILD_DIR/usr/share/applications"
mkdir -p "$BUILD_DIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$BUILD_DIR/usr/share/doc/localreel/docs"

install -m 0755 "$BINARY" "$BUILD_DIR/usr/bin/localreel"

# The service unit ships with /usr/local/bin because the tarball installs
# there; the package rewrite points at the distro location.
sed 's#/usr/local/bin/localreel#/usr/bin/localreel#' \
  "$CORE_DIR/packaging/localreel.service" > "$BUILD_DIR/lib/systemd/system/localreel.service"
chmod 0644 "$BUILD_DIR/lib/systemd/system/localreel.service"

install -m 0644 "$ROOT/README.md" "$BUILD_DIR/usr/share/doc/localreel/README.md"
install -m 0644 "$ROOT/LICENSE" "$BUILD_DIR/usr/share/doc/localreel/LICENSE"

# Desktop entry: a menu launch keeps the console visible (Terminal=true) so the
# log stays in sight and Ctrl+C is the way out; the systemd service is the
# headless path.
cat > "$BUILD_DIR/usr/share/applications/localreel.desktop" <<'EOF'
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
StartupNotify=true
EOF
chmod 0644 "$BUILD_DIR/usr/share/applications/localreel.desktop"

if [ -f "$CORE_DIR/crates/localreel-server/assets/localreel-256.png" ]; then
  install -m 0644 "$CORE_DIR/crates/localreel-server/assets/localreel-256.png" \
    "$BUILD_DIR/usr/share/icons/hicolor/256x256/apps/localreel.png"
fi

for doc in install.md operations.md architecture.md building.md linux.md docker.md service.md signing.md reverse-proxy.md; do
  if [ -f "$ROOT/docs/$doc" ]; then
    install -m 0644 "$ROOT/docs/$doc" "$BUILD_DIR/usr/share/doc/localreel/docs/$doc"
  fi
done

cat > "$BUILD_DIR/DEBIAN/control" <<EOF
Package: localreel
Version: $VERSION
Section: video
Priority: optional
Architecture: $DEB_ARCH
Maintainer: LocalReel contributors <noreply@localhost>
Installed-Size: $(( $(stat -c %s "$BINARY") / 1024 ))
Depends: libc6, ca-certificates
Recommends: ffmpeg
Description: Local-first media server with a built-in web UI
 LocalReel indexes local media, generates metadata and thumbnails with
 ffmpeg, and serves a private web interface on the loopback address.
 .
 This package installs the server, a systemd service and a desktop menu
 entry. Accounts are managed in the web UI; the first run asks for the
 owner account.
EOF

cat > "$BUILD_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if ! id -u localreel >/dev/null 2>&1; then
    adduser --system --group --home /var/lib/localreel --no-create-home localreel
fi
install -d -o localreel -g localreel /var/lib/localreel /var/log/localreel
systemctl daemon-reload >/dev/null 2>&1 || true
exit 0
EOF

cat > "$BUILD_DIR/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ -d /run/systemd/system ]; then
    systemctl stop localreel.service >/dev/null 2>&1 || true
    systemctl disable localreel.service >/dev/null 2>&1 || true
fi
exit 0
EOF

cat > "$BUILD_DIR/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi
exit 0
EOF

chmod 0755 "$BUILD_DIR/DEBIAN/postinst" "$BUILD_DIR/DEBIAN/prerm" "$BUILD_DIR/DEBIAN/postrm"

DEB="$OUT_DIR/$PKG.deb"
rm -f "$DEB"
dpkg-deb --root-owner-group --build "$BUILD_DIR" "$DEB" >/dev/null
echo "Package: $DEB"
dpkg-deb --info "$DEB" | sed -n '1,12p'
echo "Size: $(du -h "$DEB" | cut -f1)"
