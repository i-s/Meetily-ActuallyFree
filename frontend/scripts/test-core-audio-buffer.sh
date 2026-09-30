#!/usr/bin/env bash
# Device-free tests of the exact CoreAudio ring-buffer handoff used in production.
# This does not compile macOS device APIs or qualify physical audio capture.
set -euo pipefail

helper_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
helper_work_dir="$(mktemp -d "${TMPDIR:-/tmp}/meetily-core-audio-buffer.XXXXXX")"
trap 'rm -rf "$helper_work_dir"' EXIT

locked_version() {
  awk -v package="$1" '
    $0 == "name = \"" package "\"" {
      getline
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$helper_repo_root/Cargo.lock"
}

mkdir "$helper_work_dir/src"
cat > "$helper_work_dir/Cargo.toml" <<MANIFEST
[package]
name = "meetily-core-audio-buffer-tests"
version = "0.0.0"
edition = "2021"

[dependencies]
futures-util = "=$(locked_version futures-util)"
ringbuf = "=$(locked_version ringbuf)"
MANIFEST

# Reuse repository transitive versions. Cargo prunes unrelated packages and adds
# this temporary harness package; the repository lockfile is never modified.
cp "$helper_repo_root/Cargo.lock" "$helper_work_dir/Cargo.lock"
cat > "$helper_work_dir/src/lib.rs" <<SOURCE
#![allow(dead_code)]
#[path = r"$helper_repo_root/frontend/src-tauri/src/audio/capture/core_audio_buffer.rs"]
mod core_audio_buffer;
SOURCE

cargo test --manifest-path "$helper_work_dir/Cargo.toml" --lib
# Also compile the helper without its test-only interleaving hooks.
cargo check --locked --manifest-path "$helper_work_dir/Cargo.toml" --lib
