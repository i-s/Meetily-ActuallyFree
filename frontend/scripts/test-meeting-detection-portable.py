#!/usr/bin/env python3
"""Compile production call classifiers and lifecycle tests without desktop SDKs.

No native API invocation or physical Zoom/Discord qualification is implied.
"""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'frontend/src-tauri/src/meeting_detection'
with tempfile.TemporaryDirectory(prefix='meetily-detection-') as directory:
    work = Path(directory)
    source = work / 'tests.rs'
    source.write_text(
        f'#[path = "{SOURCE / "evidence.rs"}"] mod evidence;\n'
        f'#[path = "{SOURCE / "macos.rs"}"] mod macos;\n'
        'mod windows_discord {\n'
        f'#[path = "{SOURCE / "discord_evidence.rs"}"] mod discord_evidence;\n'
        '}\n'
    )
    binary = work / 'tests'
    subprocess.run(['rustc', '--edition=2021', '--test', str(source), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True)
