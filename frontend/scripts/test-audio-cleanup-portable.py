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
    diarize_start = diarization.index('pub async fn diarize_meeting(')
    diarize_command = diarization[diarize_start:diarization.index('/// Resolve authority', diarize_start)]
    diarize_command = diarize_command.replace("tauri::State<'_, crate::state::AppState>", '&crate::state::AppState')
    (work / 'diarization_reader.rs').write_text("""
use std::sync::{Arc,Barrier,Mutex};
pub static BARRIERS:Mutex<Option<(Arc<Barrier>,Arc<Barrier>)>>=Mutex::new(None);
type MeetingDiarizationResult=();
async fn operation_guard()->tokio::sync::MutexGuard<'static,()>{crate::diarization::operation_guard().await}
async fn diarize_meeting_inner(_state:&crate::state::AppState,_meeting:String,_path:Option<String>,_count:Option<usize>,_threshold:Option<f32>,_engine:Option<String>,_version:Option<&str>)->Result<(),String>{
    let (started,finish)=BARRIERS.lock().unwrap().as_ref().unwrap().clone();
    tokio::task::spawn_blocking(move||{started.wait();finish.wait();}).await.map_err(|e|e.to_string())?;
    Ok(())
}
""" + diarize_command)
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
    api = (native / 'api/api.rs').read_text()
    dto = api[api.index('pub struct MeetingAudio {'):api.index('/// Locates a meeting', api.index('pub struct MeetingAudio {'))]
    api_start = api.index('pub async fn api_get_meeting_audio<')
    playback_command = api[api_start:api.index('#[tauri::command]', api_start)]
    playback_command = playback_command.replace("tauri::State<'_, AppState>", '&AppState').replace('use tauri::Manager;', 'use crate::tauri::Manager;')
    (work / 'playback_api.rs').write_text('use serde::Serialize; use crate::tauri::{AppHandle,Runtime}; use crate::state::AppState;\n#[derive(Debug,Serialize)]\n' + dto + playback_command)
    action = (native / 'database/repositories/action_item.rs').read_text()
    owner = action[action.index('pub(crate) async fn resolve_owner('):action.index('pub struct ActionItemsRepository;')]
    (work / 'action_owner.rs').write_text('use sqlx::{Sqlite,Transaction}; use super::speaker_identity::{is_person_name,normalize_person_name}; pub(crate) use super::speaker_identity::normalize_person_name as normalize_task_text;\n' + owner)
    segment_start = api.index('pub struct TranscriptSegment {')
    segment = api[segment_start:api.index('\n#[derive',segment_start)]
    extra_production = f"""
mod api {{ #[derive(Debug,Clone,serde::Serialize,serde::Deserialize)] {segment} }}
mod config {{ pub const DEFAULT_PARAKEET_MODEL:&str="synthetic"; }}
mod codex_cli {{ #[path="{native}/codex_cli/vocabulary.rs"] pub mod vocabulary; }}
mod post_call {{
 #[path="{native}/post_call/store.rs"] pub mod store;
 #[path="{native}/post_call/runner.rs"] pub mod runner;
 #[path="{native}/post_call/summary.rs"] pub mod summary;
 #[path="{native}/post_call/priority.rs"] pub mod priority;
}}
mod database {{
 #[path="{native}/database/transcript_versions.rs"] pub mod transcript_versions;
 #[path="{native}/database/vocabulary_suggestions.rs"] pub mod vocabulary_suggestions;
 pub mod repositories {{
  #[path="{native}/database/repositories/speaker_identity.rs"] pub mod speaker_identity;
  #[path="{work}/action_owner.rs"] pub mod action_item;
  #[path="{native}/database/repositories/vocabulary.rs"] pub mod vocabulary;
 }}
}}
"""
    migrations = sorted((ROOT / 'frontend/src-tauri/migrations').glob('*.sql'))
    if not any('post_call_queue' in p.name for p in migrations):
        path = work / 'queue.sql'
        path.write_text(subprocess.check_output(['git','show','d7ed5d5fe9168051021c9fea1ce511c181275a36:frontend/src-tauri/migrations/20261118001500_post_call_queue.sql'],cwd=ROOT,text=True))
        migrations.append(path)
    (work / 'migrations.rs').write_text('pub const MIGRATIONS: &[&str] = &[' + ','.join(f'include_str!("{p}")' for p in migrations) + '];')
    source.write_text(f'''#![allow(dead_code)]
#[path="{native}/audio_cleanup.rs"] mod audio_cleanup;
#[path="{work}/commands.rs"] mod commands;
#[path="{work}/playback_api.rs"] mod playback_api;
#[path="{work}/recovery_reader.rs"] mod recovery_reader;
#[path="{work}/diarization_reader.rs"] mod diarization_reader;
#[path="{ROOT}/frontend/tests/native/audio-cleanup.rs"] mod tests;
#[path="{work}/migrations.rs"] mod migrations;
mod state {{ #[derive(Clone)] pub struct Db(pub sqlx::SqlitePool); impl Db {{ pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}} }} pub struct AppState{{pub db_manager:Db}} }}
mod tauri {{
    pub trait Runtime: Send + Sync + 'static {{}} pub struct TestRuntime; impl Runtime for TestRuntime {{}}
    pub struct AppHandle<R>(std::marker::PhantomData<R>);
    impl<R> Default for AppHandle<R> {{ fn default()->Self{{Self(std::marker::PhantomData)}} }}
    pub trait Manager {{ fn asset_protocol_scope(&self)->AssetScope; }}
    pub struct AssetScope; impl AssetScope {{ pub fn allow_directory(&self,_path:&std::path::Path,_recursive:bool)->Result<(),String>{{Ok(())}} }}
    impl<R> Manager for AppHandle<R> {{ fn asset_protocol_scope(&self)->AssetScope{{AssetScope}} }}
    pub static EMITTED:std::sync::Mutex<Vec<serde_json::Value>>=std::sync::Mutex::new(Vec::new());
    pub trait Emitter {{ fn emit<T:serde::Serialize>(&self,_event:&str,payload:T)->Result<(),String>{{ EMITTED.lock().unwrap().push(serde_json::to_value(payload).unwrap()); Ok(()) }} }}
    impl<R> Emitter for AppHandle<R> {{}}
}}
{extra_production}
mod audio {{
 pub mod retranscription {{ pub fn recording_in_person(_folder:&std::path::Path)->Result<bool,String>{{Ok(false)}} }}
 #[path="{native}/audio/playback_identity.rs"] pub mod playback_identity;
 pub mod common {{
 use once_cell::sync::Lazy; use std::sync::Arc; use tokio::sync::{{Mutex as AsyncMutex,OwnedMutexGuard}};
 static ENGINE_LIFECYCLE_LOCK:Lazy<Arc<AsyncMutex<()>>>=Lazy::new(||Arc::new(AsyncMutex::new(())));
 {lifecycle}
 }}
 pub mod recording_commands {{
 pub static ACTIVE: std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
 pub fn is_recording_active()->bool{{ACTIVE.load(std::sync::atomic::Ordering::SeqCst)}}
 pub static STOPPING: std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
 pub fn is_recording_stopping()->bool{{STOPPING.load(std::sync::atomic::Ordering::SeqCst)}}
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
    for name in ['serde', 'serde_json', 'sqlx', 'tokio', 'chrono', 'uuid', 'once_cell', 'tracing', 'regex', 'tokio_util']:
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
