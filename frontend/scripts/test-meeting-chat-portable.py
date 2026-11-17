#!/usr/bin/env python3
"""Run the production chat SQLite/context module and Codex subprocess fixtures.

Requires Python 3 and Cargo/Rust. Uses the repository lockfile and migrations;
does not link Tauri/audio or invoke a real model/account. The detached-task test
establishes native task ownership, not an installed WebView integration test.
Set CARGO_TARGET_DIR to reuse build artifacts across invocations.
"""
from pathlib import Path
import json
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
NATIVE = REPO / "frontend/src-tauri"


def main():
    with tempfile.TemporaryDirectory(prefix="meetily-chat-test-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        shutil.copy2(REPO / "Cargo.lock", work / "Cargo.lock")
        shutil.copytree(NATIVE / "migrations", work / "migrations")
        (work / "Cargo.toml").write_text('''[package]
name = "meetily-chat-tests"
version = "0.1.0"
edition = "2021"
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate"] }
tokio = { version = "1", features = ["full"] }
tokio-util = "0.7"
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
tempfile = "3"
''')
        (work / "src/lib.rs").write_text(
            f'#[path = {json.dumps(str(NATIVE / "src/meeting_chat.rs"))}]\npub mod meeting_chat;\n'
            f'#[path = {json.dumps(str(NATIVE / "src/codex_cli/codex_cli.rs"))}]\npub mod codex_cli;\n'
        )
        subprocess.run(["cargo", "test", "--manifest-path", str(work / "Cargo.toml")], check=True)


if __name__ == "__main__":
    main()
