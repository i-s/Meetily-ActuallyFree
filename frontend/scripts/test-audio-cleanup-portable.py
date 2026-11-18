#!/usr/bin/env python3
"""Compile only cleanup source/tests against already-built offline dependencies.

No Cargo invocation, dependency build/download, models or desktop build. Cache
path and compiler are explicit inputs: these are local helpers, not a portable
or authoritative public build environment. All filesystem fixtures are generated.
"""
import argparse
from pathlib import Path
import json
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--rustc', required=True)
parser.add_argument('--deps', required=True)
parser.add_argument('--extra-deps', required=True, help='Warm pulldown-cmark dependency directory')
args = parser.parse_args()
deps = Path(args.deps)
with tempfile.TemporaryDirectory(prefix='meetily-cleanup-test-build-') as folder:
    work = Path(folder)
    source = work / 'test.rs'
    native = ROOT / 'frontend/src-tauri/src'
    person = (native / 'database/repositories/person.rs').read_text()
    visible = person[person.index('pub(crate) fn visible_summary_text'):person.index('fn escape_like')]
    snippet = person[person.index('pub(crate) fn snippet_around'):person.index('pub(crate) fn truncate_chars')]
    (work / 'person.rs').write_text(visible + '\n' + snippet)
    common = (native / 'audio/common.rs').read_text()
    lifecycle = common[common.index('pub(crate) fn try_engine_lifecycle_lock'):common.index('/// Keeps a batch transcription')]
    diarization = (native / 'diarization/mod.rs').read_text()
    offline = diarization[diarization.index('static OPERATION_LOCK:'):diarization.index('/// Record where the bundled')]
    commands = (native / 'audio_cleanup_commands.rs').read_text().replace('#[tauri::command]\n', '').replace("tauri::State<'_, AppState>", '&AppState').replace('tauri::', 'crate::tauri::')
    (work / 'commands.rs').write_text(commands)
    recovery_source = (native / 'audio/incremental_saver.rs').read_text()
    recovery_command = recovery_source[recovery_source.index('pub async fn recover_audio_from_checkpoints'):recovery_source.index('/// Startup detection')]
    (work / 'recovery_reader.rs').write_text('''
use std::sync::{Arc,Barrier,Mutex};
pub static BARRIERS:Mutex<Option<(Arc<Barrier>,Arc<Barrier>)>>=Mutex::new(None);
pub struct AudioRecoveryStatus;
fn recover_recording_audio(_folder:&std::path::Path)->Result<AudioRecoveryStatus,String>{
    let (started,finish)=BARRIERS.lock().unwrap().as_ref().unwrap().clone();
    started.wait();finish.wait();Ok(AudioRecoveryStatus)
}
''' + recovery_command)
    migrations = sorted((ROOT / 'frontend/src-tauri/migrations').glob('*.sql'))
    if not any('post_call_queue' in p.name for p in migrations):
        path = work / 'queue.sql'
        path.write_text(subprocess.check_output(['git','show','d7ed5d5fe9168051021c9fea1ce511c181275a36:frontend/src-tauri/migrations/20261118001500_post_call_queue.sql'],cwd=ROOT,text=True))
        migrations.append(path)
    (work / 'migrations.rs').write_text('pub const MIGRATIONS: &[&str] = &[' + ','.join(f'include_str!("{p}")' for p in migrations) + '];')
    source.write_text(f'''#![allow(dead_code)]
#[path="{native}/audio_cleanup.rs"] mod audio_cleanup;
#[path="{work}/commands.rs"] mod commands;
#[path="{work}/recovery_reader.rs"] mod recovery_reader;
#[path="{ROOT}/frontend/tests/native/audio-cleanup.rs"] mod tests;
#[path="{work}/migrations.rs"] mod migrations;
mod state {{ pub struct Db(pub sqlx::SqlitePool); impl Db {{ pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}} }} pub struct AppState{{pub db_manager:Db}} }}
mod tauri {{
    pub trait Runtime: Send + Sync + 'static {{}} pub struct TestRuntime; impl Runtime for TestRuntime {{}}
    pub struct AppHandle<R>(std::marker::PhantomData<R>);
    impl<R> Default for AppHandle<R> {{ fn default()->Self{{Self(std::marker::PhantomData)}} }}
    pub trait Emitter {{ fn emit<T:serde::Serialize>(&self,_event:&str,_payload:T)->Result<(),String>{{Ok(())}} }}
    impl<R> Emitter for AppHandle<R> {{}}
}}
mod audio {{
 pub mod common {{
 use once_cell::sync::Lazy; use std::sync::Arc; use tokio::sync::{{Mutex as AsyncMutex,OwnedMutexGuard}};
 static ENGINE_LIFECYCLE_LOCK:Lazy<Arc<AsyncMutex<()>>>=Lazy::new(||Arc::new(AsyncMutex::new(())));
 {lifecycle}
 }}
 pub mod recording_commands {{
 pub static ACTIVE: std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
 pub fn is_recording_active()->bool{{ACTIVE.load(std::sync::atomic::Ordering::SeqCst)}}
 pub fn is_recording_stopping()->bool{{false}}
 }}
}}
mod diarization {{ {offline} }}
mod repositories {{
 #[path="{work}/person.rs"] pub mod person;
 #[path="{native}/database/repositories/library.rs"] pub mod library;
}}
''')
    command = [args.rustc, '--edition=2021', '--test', str(source), '-L', f'dependency={deps}', '-L', f'dependency={args.extra_deps}', '-o', str(work / 'tests')]
    command += ['--extern', f'pulldown_cmark={next(Path(args.extra_deps).glob("libpulldown_cmark-*.rlib"))}']
    sqlx_artifact = next(deps.glob('libsqlx-*.rlib'))
    fingerprints = deps.parent / '.fingerprint'
    def dependency(name, parent):
        metadata = json.loads((fingerprints / parent / ('lib-' + parent.rsplit('-', 1)[0].replace('-', '_') + '.json')).read_text())
        expected = next(d[3] for d in metadata['deps'] if d[1] == name)
        for fp in fingerprints.glob(name.replace('_', '-') + '-*'):
            marker = fp / ('lib-' + name)
            if marker.exists() and int.from_bytes(bytes.fromhex(marker.read_text().strip()), 'little') == expected:
                return fp.name
        raise SystemExit(f'No exact warm {name} dependency for {parent}')
    sqlx_fp = sqlx_artifact.stem.removeprefix('lib')
    core_fp = dependency('sqlx_core', sqlx_fp)
    tokio_fp = dependency('tokio', core_fp)
    for name in ['serde', 'serde_json', 'sqlx', 'tokio', 'chrono', 'uuid', 'once_cell']:
        artifacts = sorted(deps.glob(f'lib{name}-*.rlib'), key=lambda p: p.stat().st_mtime, reverse=True)
        if not artifacts:
            raise SystemExit(f'No warm {name} library in {deps}; refusing a dependency build')
        artifact = deps / f'lib{tokio_fp}.rlib' if name == 'tokio' else artifacts[0]
        command += ['--extern', f'{name}={artifact}']
    # Bundled SQLite is an already-built native dependency of sqlx-sqlite.
    for path in deps.parent.glob('build/libsqlite3-sys-*/out'):
        command += ['-L', f'native={path}']
    subprocess.run(command, check=True)
    subprocess.run([str(work / 'tests'), '--test-threads=1', '--nocapture'], check=True)
