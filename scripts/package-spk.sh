#!/usr/bin/env bash
# Build a Synology package (.spk) for DSM 7.
#
#   scripts/package-spk.sh --arch x86_64
#
# Experimental: the archive follows the documented SPK layout (INFO,
# package.tgz, scripts/, conf/privilege) and is checked structurally, but it has
# not been installed on a real DSM system yet. Install it through Package Center
# once third-party packages are allowed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_DIR="$(cd "$ROOT/../localreel-core" 2>/dev/null && pwd || true)"
WEB_DIR="$(cd "$ROOT/../localreel-web" 2>/dev/null && pwd || true)"
VERSION=""
OUT_DIR="$ROOT/dist"
SKIP_BUILD=0
BUILD_WEB=0
TARGET_ARCH="$(uname -m)"

usage() {
  cat <<'EOF'
Usage: scripts/package-spk.sh [options]

  --core <dir>     localreel-core checkout (default: ../localreel-core)
  --web <dir>      localreel-web checkout  (default: ../localreel-web)
  --version <v>    override version (default: core workspace Cargo.toml)
  --out <dir>      output directory (default: product dist/)
  --arch <arch>    x86_64 or aarch64 (default: this machine)
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
    --arch) TARGET_ARCH="${2:?missing arch}"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --build-web) BUILD_WEB=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[ "$(uname -s)" = "Linux" ] || { echo "package-spk.sh must run on Linux" >&2; exit 1; }
[ -n "$CORE_DIR" ] && [ -f "$CORE_DIR/Cargo.toml" ] || { echo "localreel-core not found" >&2; exit 1; }
[ -n "$WEB_DIR" ] && [ -d "$WEB_DIR" ] || { echo "localreel-web not found; pass --web <dir>" >&2; exit 1; }

if [ -z "$VERSION" ]; then
  VERSION="$(awk '
    /^\[workspace.package\]/ { in_section=1; next }
    /^\[/ { in_section=0 }
    in_section && $1 == "version" { gsub(/"|\r/, "", $3); print $3; exit }
  ' "$CORE_DIR/Cargo.toml")"
fi
[ -n "$VERSION" ] || { echo "could not determine version" >&2; exit 1; }

# Synology platform codenames per binary architecture; Package Center matches on
# these, not on uname -m. The list grows as new models ship.
case "$TARGET_ARCH" in
  x86_64|amd64)
    PKG_ARCH="x86_64"
    DSM_ARCH="apollolake avoton braswell broadwell broadwellnk bromolow cedarview denverton geminilake germantown grantley purley v1000 x86_64"
    ;;
  aarch64|arm64)
    PKG_ARCH="aarch64"
    DSM_ARCH="alpine alpine4k armada37xx armada38x rtd1296 armv8"
    ;;
  *) echo "unsupported arch: $TARGET_ARCH" >&2; exit 1 ;;
esac

if [ "$BUILD_WEB" -eq 1 ]; then
  (cd "$WEB_DIR" && npm run build)
fi

if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "building localreel-server (release, $TARGET_ARCH)..."
  (cd "$CORE_DIR" && LOCALREEL_WEB_DIR="$WEB_DIR" cargo build --release -p localreel-server)
fi

if [ ! -f "$WEB_DIR/dist/index.html" ]; then
  echo "missing $WEB_DIR/dist/index.html; build the web UI first or pass --build-web" >&2
  exit 1
fi

BINARY="$CORE_DIR/target/release/localreel"
[ -f "$BINARY" ] || { echo "missing $BINARY (run without --skip-build)" >&2; exit 1; }

BUILD="$OUT_DIR/.spk-$PKG_ARCH"
rm -rf "$BUILD"
mkdir -p "$BUILD/scripts" "$BUILD/conf" "$BUILD/target/bin"

install -m 0755 "$BINARY" "$BUILD/target/bin/localreel"

cat > "$BUILD/scripts/start-stop-status" <<'SCRIPT'
#!/bin/sh
# DSMPKG start/stop/status. Data lives in the package's var directory so an
# upgrade keeps the library and accounts.
PACKAGE="localreel"
TARGET="/var/packages/${PACKAGE}/target"
DATA="/var/packages/${PACKAGE}/var"

case "$1" in
  start)
    mkdir -p "$DATA/logs"
    LOCALREEL_MODE=server LOCALREEL_DATA_DIR="$DATA" LOCALREEL_LOG_DIR="$DATA/logs" \
      "$TARGET/bin/localreel" serve --mode server --no-open >>"$DATA/logs/start.log" 2>&1 &
    echo $! > "$DATA/localreel.pid"
    sleep 1
    if ! kill -0 "$(cat "$DATA/localreel.pid")" 2>/dev/null; then
      echo "localreel did not stay up; see $DATA/logs/localreel.log" >&2
      exit 1
    fi
    ;;
  stop)
    if [ -f "$DATA/localreel.pid" ]; then
      kill "$(cat "$DATA/localreel.pid")" 2>/dev/null || true
      rm -f "$DATA/localreel.pid"
    fi
    ;;
  status)
    [ -f "$DATA/localreel.pid" ] && kill -0 "$(cat "$DATA/localreel.pid")" 2>/dev/null
    ;;
  *)
    exit 1
    ;;
esac
exit 0
SCRIPT
chmod 0755 "$BUILD/scripts/start-stop-status"

cat > "$BUILD/scripts/postinst" <<'SCRIPT'
#!/bin/sh
mkdir -p /var/packages/localreel/var/logs
exit 0
SCRIPT
chmod 0755 "$BUILD/scripts/postinst"

cat > "$BUILD/scripts/preuninst" <<'SCRIPT'
#!/bin/sh
if [ -f /var/packages/localreel/var/localreel.pid ]; then
  kill "$(cat /var/packages/localreel/var/localreel.pid)" 2>/dev/null || true
fi
exit 0
SCRIPT
chmod 0755 "$BUILD/scripts/preuninst"

# DSM 7 needs to know which user the package runs as; the server writes into
# its own var directory and binds an unprivileged port, so a package user is
# enough.
cat > "$BUILD/conf/privilege" <<'JSON'
{
  "defaults": {
    "run-as": "package"
  },
  "username": "localreel",
  "groupname": "localreel"
}
JSON

# Package Center shows the icon in the package list.
if [ -f "$CORE_DIR/crates/localreel-server/assets/localreel-256.png" ]; then
  cp "$CORE_DIR/crates/localreel-server/assets/localreel-256.png" "$BUILD/PACKAGE_ICON.PNG"
fi

cat > "$BUILD/INFO" <<INFO
package="localreel"
version="$VERSION"
os_min_ver="7.0-40000"
displayname="LocalReel"
description="Local-first media server. Open the built-in web UI on port 3001, create the owner account, then add your media folders."
maintainer="LocalReel contributors"
maintainer_url="https://github.com/LocalReel/LocalReel"
distributor="LocalReel"
distributor_url="https://github.com/LocalReel/LocalReel"
arch="$DSM_ARCH"
adminprotocol="http"
adminport="3001"
dsmappname="SYNO.SDS.LocalReel"
silent_install="yes"
silent_upgrade="yes"
silent_uninstall="yes"
startstop_restart="yes"
INFO

# package.tgz carries the app tree that DSM extracts into the target directory.
tar -C "$BUILD/target" -czf "$BUILD/package.tgz" .

SPK="$OUT_DIR/localreel-$VERSION-spk-$PKG_ARCH.spk"
rm -f "$SPK"
( cd "$BUILD" && tar -cf "$SPK" INFO package.tgz scripts conf PACKAGE_ICON.PNG 2>/dev/null \
  || tar -cf "$SPK" INFO package.tgz scripts conf )
rm -rf "$BUILD"

echo "SPK: $SPK"
echo "Size: $(du -h "$SPK" | cut -f1)"
