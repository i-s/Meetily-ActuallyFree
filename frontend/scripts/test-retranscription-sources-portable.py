#!/usr/bin/env python3
"""Extract production source/label policy and its regressions, without models/Tauri."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'frontend/src-tauri/src/audio/retranscription.rs'


def main():
    source = SOURCE.read_text()
    policy = source[source.index('fn find_audio_file'):source.index('fn create_source_labeled_segments')]
    tests = source[source.index('#[cfg(test)]\nmod issue9_room_tests'):]
    diarization = (ROOT / 'frontend/src-tauri/src/diarization/mod.rs').read_text()
    policy += diarization[diarization.index('fn refinement_source_accepts'):diarization.index('fn apply_source_track_hint')]
    tests += diarization[diarization.index('#[cfg(test)]\nmod issue14_source_attribution_tests'):]
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in ('anyhow', 'serde_json', 'tempfile')}
    with tempfile.TemporaryDirectory(prefix='meetily-retranscription-policy-') as directory:
        work = Path(directory)
        (work / 'src').mkdir()
        (work / 'Cargo.toml').write_text('[package]\nname="meetily-retranscription-policy"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n' + '\n'.join(f'{n}="={v}"' for n, v in versions.items()))
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        (work / 'src/lib.rs').write_text('use std::path::{Path,PathBuf}; use anyhow::{anyhow,Result}; const AUDIO_EXTENSIONS: &[&str] = &["mp4","wav","m4a"];\n' + policy + '\n' + tests)
        subprocess.run(['cargo', 'test', '--manifest-path', str(work / 'Cargo.toml')], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-retranscription-policy' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
