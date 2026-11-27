#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${EXTRA_BIN_DIRS:-}" ]]; then
  export PATH="${EXTRA_BIN_DIRS}:${PATH}"
fi
if [[ -n "${TMPDIR:-}" && ! -d "${TMPDIR}" ]]; then
  export TMPDIR="/tmp"
fi

echo "Building lume (static x86_64-unknown-linux-musl, x86-64-v3)..."
cargo build --release --target x86_64-unknown-linux-musl -p lume-cli

BIN="target/x86_64-unknown-linux-musl/release/lume"
echo "Done: ${BIN}"
file "${BIN}" || true
ldd "${BIN}" 2>&1 || true
ls -lh "${BIN}"
