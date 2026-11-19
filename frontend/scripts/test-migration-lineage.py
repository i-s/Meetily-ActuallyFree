#!/usr/bin/env python3
"""Run the app's migration-lineage tests with real SQLx, without desktop/audio deps.

All DBs are generated persistent temporary files. Uses workspace-locked dependency
versions and authentic historical SQL fixtures; never opens application data.
"""
from pathlib import Path
import argparse
import os
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--offline', action='store_true')
args = parser.parse_args()
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n)
            for n in ('sqlx', 'tokio', 'tempfile', 'log')}
with tempfile.TemporaryDirectory(prefix='meetily-migration-lineage-') as folder:
    work = Path(folder)
    (work / 'src').mkdir()
    (work / 'migrations').symlink_to(ROOT / 'frontend/src-tauri/migrations', target_is_directory=True)
    (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-migration-lineage"
version="0.0.0"
edition="2021"
[dependencies]
sqlx={{version="={versions['sqlx']}",default-features=false,features=["runtime-tokio","sqlite","migrate","macros"]}}
tokio={{version="={versions['tokio']}",features=["macros","rt-multi-thread"]}}
tempfile="={versions['tempfile']}"
log="={versions['log']}"
''')
    (work / 'Cargo.lock').write_bytes((ROOT / 'Cargo.lock').read_bytes())
    (work / 'src/lib.rs').write_text(f'''extern crate self as tauri;
pub struct AppHandle;
mod paths {{ pub fn install_data_root() -> std::path::PathBuf {{ panic!("desktop paths must not be used by generated lineage fixtures") }} }}
#[path="{ROOT}/frontend/src-tauri/src/database/manager.rs"]
mod manager;
''')
    env = dict(os.environ, CARGO_BUILD_JOBS='1')
    offline = ['--offline'] if args.offline else []
    subprocess.run(['cargo', 'tree', *offline, '--manifest-path', str(work / 'Cargo.toml')], env=env, stdout=subprocess.DEVNULL, check=True)
    allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
    actual = tomllib.loads((work / 'Cargo.lock').read_text())
    assert all(p['name'] == 'meetily-migration-lineage' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package']), 'unlocked dependency version'
    subprocess.run(['cargo', 'test', *offline, '--locked', '--manifest-path', str(work / 'Cargo.toml'), 'migration_lineage', '--', '--test-threads=1'], env=env, check=True)
