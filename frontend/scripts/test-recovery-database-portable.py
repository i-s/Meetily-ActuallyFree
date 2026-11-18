#!/usr/bin/env python3
"""Run actual recovery commands + repository transactions with real SQLite/FFmpeg.

Only Tauri handle/state transport, device metadata and unrelated contact linking
are stubbed. Production command bodies, JSON types, audio commit logic and SQL
are included verbatim. This does not establish macOS/Tauri ABI or model behavior.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / 'frontend/src-tauri/src/audio'
REPOSITORY = ROOT / 'frontend/src-tauri/src/database/repositories/transcript.rs'


def main():
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    names = ('sqlx', 'chrono', 'tokio', 'uuid', 'tracing', 'sha2', 'anyhow', 'log', 'serde', 'serde_json', 'tempfile', 'bytemuck')
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in names}
    ffmpeg = shutil.which('ffmpeg')
    if not ffmpeg:
        raise SystemExit('Real FFmpeg on PATH is required')
    api = (ROOT / 'frontend/src-tauri/src/api/api.rs').read_text()
    start = api.index('pub async fn api_save_transcript<R: Runtime>')
    command = api[start:api.index('/// Opens the meeting\'s recording folder', start)]
    def data_type(name):
        start = api.index(f'pub struct {name} {{')
        return '#[derive(Debug,serde::Serialize,serde::Deserialize)]\n' + api[start:api.index('\n#[derive', start)]
    with tempfile.TemporaryDirectory(prefix='meetily-recovery-db-') as directory:
        work = Path(directory)
        (work / 'src').mkdir()
        dependencies = '\n'.join(f'{name}="={versions[name]}"' for name in ('chrono', 'tracing', 'sha2', 'anyhow', 'log', 'serde_json', 'tempfile', 'bytemuck'))
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-recovery-db"
version="0.0.0"
edition="2021"
[dependencies]
{dependencies}
sqlx={{version="={versions['sqlx']}",default-features=false,features=["sqlite","runtime-tokio","chrono"]}}
tokio={{version="={versions['tokio']}",features=["full"]}}
uuid={{version="={versions['uuid']}",features=["v4"]}}
serde={{version="={versions['serde']}",features=["derive"]}}
''')
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        (work / 'src/incremental_saver.rs').write_text((AUDIO / 'incremental_saver.rs').read_text().replace('#[tauri::command]', ''))
        (work / 'src/api.rs').write_text('use chrono::{DateTime,Utc}; use crate::{AppHandle,Runtime}; use crate::state::AppState; use crate::repository::TranscriptsRepository;\nuse log::{debug as log_debug,error as log_error,info as log_info,warn as log_warn};\n' + data_type('TranscriptSegment') + '\n' + data_type('TranscriptSearchResult') + '\n' + command)
        (work / 'src/lib.rs').write_text(f'''#![allow(dead_code)]
extern crate self as tauri;
pub trait Runtime {{}}
impl Runtime for () {{}}
pub struct AppHandle<R:Runtime>(pub std::marker::PhantomData<R>);
pub struct State<'a,T>(pub &'a T);
impl<'a,T> std::ops::Deref for State<'a,T> {{ type Target=T; fn deref(&self)->&T{{self.0}} }}
mod state {{
    pub struct AppState {{ pub db_manager: DbManager }}
    pub struct DbManager(pub sqlx::SqlitePool);
    impl DbManager {{ pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}} }}
}}
mod api;
mod database {{ pub mod repositories {{ pub mod person {{
    pub struct PeopleRepository;
    impl PeopleRepository {{ pub async fn link_named_speakers(_tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>,_meeting:&str)->Result<(),sqlx::Error>{{Ok(())}} }}
}} }} }}
#[path="{REPOSITORY}"] mod repository;
mod audio {{
    pub mod devices {{ #[derive(Clone)] pub struct AudioDevice {{ pub name:String }} }}
    pub use devices::AudioDevice;
    pub mod ffmpeg {{ pub fn find_ffmpeg_path()->Option<std::path::PathBuf>{{Some({ffmpeg!r}.into())}} }}
    #[path="{AUDIO}/buffer_pool.rs"] pub mod buffer_pool;
    #[path="{AUDIO}/recording_state.rs"] pub mod recording_state;
    #[path="{AUDIO}/encode.rs"] pub mod encode;
    #[path="{AUDIO}/track_commit.rs"] pub mod track_commit;
    #[path="{work}/src/incremental_saver.rs"] pub mod incremental_saver;
}}
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/recovery-api.rs"] mod recovery_api_tests;
'''.replace(repr(ffmpeg), '"' + ffmpeg + '"'))
        subprocess.run(['cargo', 'test', '--manifest-path', str(work / 'Cargo.toml'), 'recovery', '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-recovery-db' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
