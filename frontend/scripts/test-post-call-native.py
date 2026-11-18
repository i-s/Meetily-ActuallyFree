#!/usr/bin/env python3
"""Compile native scheduler and awaited refinement bodies without a desktop build.

SQLite, queue orchestration, version storage, subprocess isolation and JSON evidence
validation are production code. Tauri transport, local model inference/source IO,
engine locks and preference transport are synthetic; no real service is invoked.
"""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / 'frontend/src-tauri/src'
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
names = ('sqlx', 'chrono', 'tokio', 'tokio-util', 'uuid', 'serde', 'serde_json', 'tempfile', 'anyhow', 'log')
v = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in names}
api = (SRC / 'api/api.rs').read_text()
start = api.index('pub struct TranscriptSegment {')
segment = '#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n' + api[start:api.index('\n#[derive', start)]
r = (SRC / 'audio/retranscription.rs').read_text()
owned = r[r.index('/// Global flag to track'):r.index('/// Find audio file')]
queued = r[r.index('/// Awaited saved-meeting operation'):r.index('#[tauri::command]\npub async fn cancel_retranscription_command')]
with tempfile.TemporaryDirectory(prefix='meetily-post-call-native-') as directory:
    work = Path(directory)
    (work / 'src').mkdir()
    (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-post-call-native"
version="0.0.0"
edition="2021"
[dependencies]
chrono="={v['chrono']}"
anyhow="={v['anyhow']}"
log="={v['log']}"
tokio={{version="={v['tokio']}",features=["full"]}}
tokio-util="={v['tokio-util']}"
sqlx={{version="={v['sqlx']}",default-features=false,features=["sqlite","runtime-tokio","chrono","derive"]}}
uuid={{version="={v['uuid']}",features=["v4"]}}
serde={{version="={v['serde']}",features=["derive"]}}
serde_json="={v['serde_json']}"
tempfile="={v['tempfile']}"
''')
    (work / 'Cargo.lock').write_bytes((ROOT / 'Cargo.lock').read_bytes())
    post = (SRC / 'post_call/mod.rs').read_text().replace('#[tauri::command]', '')
    for name in ('store', 'summary', 'priority', 'runner', 'worker'):
        post = post.replace(f'pub mod {name};', f'#[path="{SRC}/post_call/{name}.rs"] pub mod {name};')
    (work / 'src/post_call.rs').write_text(post)
    (work / 'src/lib.rs').write_text(f'''#![allow(dead_code,unused_imports)]
extern crate self as tauri;
use std::sync::{{Arc,Mutex}};
pub trait Runtime:Send+Sync+'static{{}} impl Runtime for (){{}}
pub trait Manager{{}} pub trait Emitter{{}}
pub struct AppHandle<R:Runtime>(pub std::marker::PhantomData<R>);
impl<R:Runtime> Clone for AppHandle<R>{{fn clone(&self)->Self{{Self(std::marker::PhantomData)}}}}
static STATE:Mutex<Option<Arc<dyn std::any::Any+Send+Sync>>>=Mutex::new(None);
impl<R:Runtime> AppHandle<R>{{
 pub fn state<T:std::any::Any+Send+Sync>(&self)->Arc<T>{{STATE.lock().unwrap().as_ref().unwrap().clone().downcast::<T>().ok().unwrap()}}
 pub fn try_state<T:std::any::Any+Send+Sync>(&self)->Option<Arc<T>>{{STATE.lock().unwrap().as_ref()?.clone().downcast::<T>().ok()}}
 pub fn emit<T:serde::Serialize>(&self,_event:&str,_value:T)->Result<(),String>{{Ok(())}}
}}
pub struct State<'a,T>(pub &'a T); impl<'a,T> std::ops::Deref for State<'a,T>{{type Target=T;fn deref(&self)->&T{{self.0}}}}
pub mod async_runtime{{pub fn spawn<F>(f:F)->tokio::task::JoinHandle<F::Output> where F:std::future::Future+Send+'static,F::Output:Send+'static{{tokio::spawn(f)}}}}
pub mod api{{{segment}}}
pub mod config{{pub const DEFAULT_PARAKEET_MODEL:&str="synthetic";}}
pub mod state{{pub struct AppState{{pub db_manager:DbManager}}pub struct DbManager(pub sqlx::SqlitePool);impl DbManager{{pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}}}}}}
pub mod database{{
 #[path="{SRC}/database/transcript_versions.rs"]pub mod transcript_versions;
 pub mod repositories{{
  #[path="{SRC}/database/repositories/speaker_identity.rs"]pub mod speaker_identity;
  pub mod setting{{pub struct SettingsRepository;impl SettingsRepository{{pub async fn get_post_call_transcript_config(p:&sqlx::SqlitePool)->Result<Option<(String,String)>,sqlx::Error>{{sqlx::query_as("SELECT postCallProvider,postCallModel FROM transcript_settings WHERE id='1' AND postCallProvider IS NOT NULL AND postCallModel IS NOT NULL").fetch_optional(p).await}}}}}}
  #[path="{SRC}/database/repositories/vocabulary.rs"]pub mod vocabulary;
 }}
}}
static ROOT:Mutex<Option<std::path::PathBuf>>=Mutex::new(None);
pub mod codex_cli{{
 #[path="{SRC}/codex_cli/vocabulary.rs"]pub mod vocabulary;
 #[path="{SRC}/codex_cli/cli.rs"]pub mod cli;
 pub mod commands{{pub async fn codex_cli_get_path()->Result<Option<String>,String>{{Ok(Some(crate::ROOT.lock().unwrap().as_ref().unwrap().join("fake codex").to_string_lossy().into_owned()))}}pub async fn isolated_auth_home()->Result<std::path::PathBuf,String>{{Ok(crate::ROOT.lock().unwrap().as_ref().unwrap().join("isolated-auth"))}}}}
}}
pub mod diarization{{
 static LOCK:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
 pub async fn operation_guard()->tokio::sync::MutexGuard<'static,()>{{LOCK.lock().await}}
 #[derive(serde::Serialize)]pub struct Config{{pub engine:String,pub nemotron_max_speakers:usize}}
 pub fn get_diarization_config()->Config{{Config{{engine:"nemotron".into(),nemotron_max_speakers:8}}}}
 pub async fn diarization_models_available()->Result<bool,String>{{Ok(true)}}
 pub async fn diarize_version(_s:&crate::state::AppState,_m:&str,_v:&str)->Result<(),String>{{Ok(())}}
}}
pub mod audio{{
 pub mod common{{pub struct BatchLease;pub async fn acquire_stt_batch_lease()->BatchLease{{BatchLease}}pub async fn unload_engine_after_batch(_p:bool){{}}}}
 pub mod recording_commands{{use std::sync::atomic::{{AtomicBool,Ordering}};pub static LIVE:AtomicBool=AtomicBool::new(false);pub fn is_recording_active()->bool{{LIVE.load(Ordering::SeqCst)}}pub fn is_recording_stopping()->bool{{false}}}}
 pub mod retranscription{{
 use crate::{{AppHandle,Runtime,Manager,Emitter,state::AppState,database::repositories::vocabulary::VocabularyRepository}};
 use serde::{{Serialize,Deserialize}};use anyhow::{{anyhow,Result}};use std::sync::{{Mutex,atomic::{{AtomicBool,Ordering}}}};use std::path::Path;
 pub static MUTATE_APPROVAL:AtomicBool=AtomicBool::new(false);
 pub static ASR_CALLS:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
 {owned}
 {queued}
 fn emit_progress<R:Runtime>(_app:&AppHandle<R>,_meeting:&str,_stage:&str,_progress:u32,_message:&str){{}}
 fn saved_sources(_folder:&Path)->Result<()>{{Ok(())}}
 fn recording_metadata(_folder:&Path)->Result<serde_json::Value>{{Ok(serde_json::json!({{"in_person":true,"future_policy":"opaque"}}))}}
 fn recording_in_person(_folder:&Path)->Result<bool>{{Ok(true)}}
 async fn run_retranscription<R:Runtime>(app:AppHandle<R>,meeting_id:String,_folder:String,language:Option<String>,_model:Option<String>,_provider:Option<String>,_prompt:Option<String>,version_id:String)->Result<RetranscriptionResult>{{
  ASR_CALLS.fetch_add(1,Ordering::SeqCst);
  if MUTATE_APPROVAL.swap(false,Ordering::SeqCst){{VocabularyRepository::confirm_global(app.state::<AppState>().db_manager.pool(),"LaterApproved").await.map_err(anyhow::Error::msg)?;}}
  let row=crate::api::TranscriptSegment{{id:"candidate-room".into(),text:"Synthetic refined speech".into(),timestamp:"2026".into(),audio_start_time:Some(1.),audio_end_time:Some(2.),duration:Some(1.),speaker:Some("Microphone".into())}};
  crate::database::transcript_versions::finish_with_sources(app.state::<AppState>().db_manager.pool(),&version_id,&[row],&std::collections::HashMap::from([("candidate-room".into(),"room".into())])).await.map_err(anyhow::Error::msg)?;
  Ok(RetranscriptionResult{{meeting_id,version_id,segments_count:1,duration_seconds:2.,language}})
 }}
 }}
}}
#[path="{work}/src/post_call.rs"]pub mod post_call;
#[cfg(test)]#[path="{ROOT}/frontend/tests/native/post-call-native.rs"]mod native_tests;
''')
    subprocess.run(['cargo', 'test', '--offline', '-j', '1', '--manifest-path', str(work / 'Cargo.toml'), *(sys.argv[1:] or ['native_queue']), '--', '--test-threads=1'], check=True)
    allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
    actual = tomllib.loads((work / 'Cargo.lock').read_text())
    assert all(p['name'] == "meetily-post-call-native" or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])
