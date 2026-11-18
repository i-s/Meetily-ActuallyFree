#!/usr/bin/env python3
"""Run mounted frontend recorder/Controls against production Rust authorization.

JSON-lines replaces Tauri transport, with synthetic source/device/save adapters.
This is cross-language command-path coverage, not desktop/audio qualification.
"""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def main():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('native_fixture', ROOT / 'frontend/scripts/test-recording-start-authority-portable.py')
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    bun = os.environ.get('BUN') or shutil.which('bun')
    if not bun:
        raise SystemExit('Set BUN to the installed Bun executable')
    with tempfile.TemporaryDirectory(prefix='meetily-combined-recorder-') as directory:
        work = Path(directory)
        fixture.prepare(work)
        shutil.copyfile(ROOT / 'frontend/tests/native/recording-integration-rpc.rs', work / 'src/main.rs')
        subprocess.run(['cargo', 'build', '--offline', '--manifest-path', str(work / 'Cargo.toml')], check=True)
        target = Path(os.environ.get('CARGO_TARGET_DIR', work / 'target'))
        data = work / 'synthetic-data'; data.mkdir()
        env = dict(os.environ, MEETILY_RECORDING_RPC=str(target / 'debug/meetily-start-authority'), MEETILY_RECORDING_DATA=str(data))
        subprocess.run([bun, 'test', '--cwd', str(ROOT / 'frontend'), 'tests/hooks/meeting-native-integration.test.tsx', '--timeout', '60000'], env=env, check=True)


if __name__ == '__main__':
    main()
