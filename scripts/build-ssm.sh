#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$ROOT_DIR/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
PKG_NAME="lume"
PLATFORM="all"
MAINTAINER="${SSM_MAINTAINER:-lume}"
PKG_DIR="${PKG_NAME}_${VERSION}_${PLATFORM}"
BIN_PATH="$ROOT_DIR/target/x86_64-unknown-linux-musl/release/${PKG_NAME}"
DIST_DIR="$ROOT_DIR/dist"
STAGE_DIR="$DIST_DIR/$PKG_DIR"

if [[ ! -x "$BIN_PATH" ]]; then
    echo "ERROR: release binary not found at $BIN_PATH" >&2
    exit 1
fi

if ldd "$BIN_PATH" 2>&1 | grep -q "=>"; then
    echo "ERROR: $BIN_PATH is dynamically linked!" >&2
    ldd "$BIN_PATH"
    exit 1
fi

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/.ssm.d" "$STAGE_DIR/bin" "$STAGE_DIR/etc/profile.d"

cp "$BIN_PATH" "$STAGE_DIR/bin/${PKG_NAME}"
chmod 0755 "$STAGE_DIR/bin/${PKG_NAME}"

cat > "$STAGE_DIR/.ssm.d/control.json" <<EOF
{
  "name": "${PKG_NAME}",
  "version": "${VERSION}",
  "platform": "${PLATFORM}",
  "maintainer": "${MAINTAINER}",
  "summary": "Pixel-rendered & ANSI terminal Markdown viewer and live two-pane editor",
  "x-target": "x86_64-unknown-linux-musl",
  "x-cpu": "x86-64-v3"
}
EOF

cat > "$STAGE_DIR/etc/profile.d/${PKG_NAME}.sh" <<'EOF'
if [ -n "${SSM_PACKAGE_ROOT:-}" ] && [ -d "${SSM_PACKAGE_ROOT}/bin" ]; then
    case ":${PATH}:" in
        *":${SSM_PACKAGE_ROOT}/bin:"*) ;;
        *) export PATH="${SSM_PACKAGE_ROOT}/bin:${PATH}" ;;
    esac
fi
EOF
chmod 0644 "$STAGE_DIR/etc/profile.d/${PKG_NAME}.sh"

TARBALL="$DIST_DIR/${PKG_DIR}.ssm"
tar -czf "$TARBALL" -C "$DIST_DIR" "$PKG_DIR"
echo "Built SSM package: $TARBALL ($(du -h "$TARBALL" | cut -f1))"
