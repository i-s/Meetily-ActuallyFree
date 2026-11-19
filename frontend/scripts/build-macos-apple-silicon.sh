#!/bin/bash

set -euo pipefail

TARGET="aarch64-apple-darwin"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRONTEND_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$FRONTEND_DIR/.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This build must run on macOS."
    exit 1
fi

if [[ "$(uname -m)" != "arm64" ]]; then
    echo "Run this build in a native Apple Silicon terminal (not Rosetta)."
    exit 1
fi

command -v cargo >/dev/null || { echo "Rust is required."; exit 1; }
command -v pnpm >/dev/null || { echo "pnpm is required."; exit 1; }
command -v brew >/dev/null || { echo "Homebrew with openssl@3 is required."; exit 1; }

# Match the CI sidecar's OpenSSL discovery and the app's minimum macOS version.
OPENSSL_PREFIX="$(brew --prefix openssl@3)"
test -f "$OPENSSL_PREFIX/include/openssl/err.h" || {
    echo "OpenSSL 3 headers missing; run: brew install openssl@3"
    exit 1
}
export OPENSSL_DIR="$OPENSSL_PREFIX"
export OPENSSL_ROOT_DIR="$OPENSSL_PREFIX"
export CMAKE_PREFIX_PATH="$OPENSSL_PREFIX${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}"
export CPPFLAGS="-I$OPENSSL_PREFIX/include ${CPPFLAGS:-}"
export CFLAGS="-I$OPENSSL_PREFIX/include ${CFLAGS:-}"
export CXXFLAGS="-I$OPENSSL_PREFIX/include ${CXXFLAGS:-}"
export LDFLAGS="-L$OPENSSL_PREFIX/lib ${LDFLAGS:-}"
export PKG_CONFIG_PATH="$OPENSSL_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export MACOSX_DEPLOYMENT_TARGET=14.2

rustup target add "$TARGET"

echo "Building the Apple Silicon llama-helper sidecar..."
(
    cd "$REPO_DIR"
    cargo build --locked --release --package llama-helper --target "$TARGET" --features metal
)

mkdir -p "$FRONTEND_DIR/src-tauri/binaries"
cp \
    "$REPO_DIR/target/$TARGET/release/llama-helper" \
    "$FRONTEND_DIR/src-tauri/binaries/llama-helper-$TARGET"
chmod +x "$FRONTEND_DIR/src-tauri/binaries/llama-helper-$TARGET"

echo "Building the Apple Silicon DMG..."
(
    cd "$FRONTEND_DIR"
    pnpm exec tauri build --target "$TARGET" --bundles dmg -- --locked
)

echo "Build complete."
find "$REPO_DIR/target/$TARGET/release/bundle/dmg" -maxdepth 1 -name "*.dmg" -print
