#!/usr/bin/env python3
"""Actual diarize_meeting command/SQL and voiceprint decision, synthetic inference.

Only Tauri transport, models/DSP and unrelated contact links are stubbed. The
production command, user-cluster decision block, naming/persistence path and
metadata policy are exercised with real SQLite. No acoustic/model claim. Baseline
uses 7d3ff8a's unmodified command and voiceprint decision for runtime red evidence.
"""
from pathlib import Path
import argparse
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
NATIVE = ROOT / 'frontend/src-tauri/src'


def section(source, start, end):
    return source[source.index(start):source.index(end, source.index(start))]


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--baseline', action='store_true'); args = parser.parse_args()
    source = (subprocess.check_output(['git', 'show', '7d3ff8a:frontend/src-tauri/src/diarization/mod.rs'], text=True)
              if args.baseline else (NATIVE / 'diarization/mod.rs').read_text())
    types = section(source, '#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct DiarizationSegment', '/// Required model files')
    response = section(source, '#[derive(Debug, Clone, Serialize)]\npub struct MeetingDiarizationResult', '/// Audio container extensions')
    command = section(source, 'pub async fn diarize_meeting(', '/// Slide the analysis window')
    names = section(source, 'fn is_manual_speaker_label(', '/// Decode any supported')
    hint = section(source, 'fn refinement_source_accepts(' if 'fn refinement_source_accepts(' in source else 'fn apply_source_track_hint(', '#[cfg(test)]\nmod tests')
    user = section(source, '    let user_speaker = ', '\n    log::info!(')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in ('anyhow', 'log', 'serde', 'serde_json', 'tokio', 'sqlx', 'tempfile', 'chrono', 'uuid', 'tracing', 'sha2', 'bytemuck')}
    with tempfile.TemporaryDirectory(prefix='meetily-offline-identity-') as directory:
        work = Path(directory); (work / 'src').mkdir()
        # Match the warmed recovery harness's dependency/features surface so this
        # command-path test does not trigger an unrelated SQLx dependency rebuild.
        deps = '\n'.join(f'{n}="={versions[n]}"' for n in ('anyhow', 'log', 'serde_json', 'tempfile', 'chrono', 'tracing', 'sha2', 'bytemuck'))
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-offline-identity"
version="0.0.0"
edition="2021"
[dependencies]
{deps}
uuid={{version="={versions['uuid']}",features=["v4"]}}
serde={{version="={versions['serde']}",features=["derive"]}}
tokio={{version="={versions['tokio']}",features=["full"]}}
sqlx={{version="={versions['sqlx']}",default-features=false,features=["sqlite","runtime-tokio","chrono","derive"]}}
''')
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        config = section(source, '#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct DiarizationConfig', 'impl Default for DiarizationConfig') if 'pub struct DiarizationConfig' in source else ''
        retranscription = (NATIVE / 'audio/retranscription.rs').read_text()
        source_policy = section(retranscription, 'pub(crate) fn find_audio_file', 'fn create_source_labeled_segments')
        preflight = section(retranscription, '    let folder_path = PathBuf::from(&meeting_folder_path);', '    // Determine which provider to use')
        preflight_adapter = """
struct TestApp<'a>(&'a crate::state::AppState);
impl TestApp<'_> { fn try_state<T: 'static>(&self)->Option<&T> { (self.0 as &dyn std::any::Any).downcast_ref::<T>() } }
pub async fn asr_preflight(state:&crate::state::AppState,meeting_id:String,meeting_folder_path:String,version_id:String)->Result<Vec<(String,Option<String>)>> {
 let app=TestApp(state);
""" + preflight + """
 Ok(sources.into_iter().map(|source|(source.source_id.to_string(),source.speaker_hint.map(str::to_string))).collect())
}
"""
        # The historical red target predates candidate APIs; keep its ordinary
        # command repro runnable rather than failing to compile the new fixture.
        if args.baseline:
            preflight_adapter = ''
        combined_tests = '' if args.baseline else f'#[cfg(test)]#[path="{ROOT}/frontend/tests/native/identity-versions.rs"] mod identity_versions_tests;'
        (work / 'src/retranscription.rs').write_text('use std::path::{Path,PathBuf}; use anyhow::{anyhow,Result}; use crate::state::AppState; const AUDIO_EXTENSIONS: &[&str] = &["mp4","wav","m4a"];\n' + source_policy + '\npub fn refinement_cancelled()->bool { false }' + preflight_adapter)
        for name in ('meeting_notes', 'notes_document'):
            text = (NATIVE / f'database/repositories/{name}.rs').read_text().replace('#[tauri::command]', '').replace('../../../migrations/20261118001200_recording_notes.sql', str(NATIVE.parent / 'migrations/20261118001200_recording_notes.sql'))
            (work / f'src/{name}.rs').write_text(text)
        (work / 'src/command.rs').write_text('use std::path::{Path,PathBuf}; use anyhow::{anyhow,Result}; use serde::{Serialize,Deserialize};\n' + types + config + response + names + command + hint + f'''
#[path="{NATIVE}/diarization/identity_policy.rs"] pub mod identity_policy;
const DEFAULT_THRESHOLD:f32=0.6;
fn get_active_engine()->String{{"pyannote".into()}}
async fn operation_guard()->tokio::sync::MutexGuard<'static,()> {{static OP:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());OP.lock().await}}
fn find_meeting_audio(_folder:Option<String>,_title:Option<&str>)->Option<PathBuf>{{None}}
pub static DECODES:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
pub static FAIL_ENGINE:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
fn ensure_wav(path:&Path)->Result<(PathBuf,bool)>{{
 DECODES.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
 let file=tempfile::Builder::new().prefix(&format!("decoded-{{}}-",path.file_stem().unwrap().to_string_lossy())).suffix(".wav").tempfile().unwrap();
 Ok((file.into_temp_path().keep().unwrap(),true))
}}
fn diarization_model_dir()->PathBuf{{PathBuf::new()}}
fn diarization_user_model_dir()->PathBuf{{PathBuf::new()}}
fn embeddings_for_debug(_path:&Path,_dir:&Path)->Result<Vec<Vec<f32>>>{{Ok(vec![vec![1.,0.];3])}}
fn get_diarization_config()->DiarizationConfig{{DiarizationConfig{{engine:"pyannote".into(),nemotron_max_speakers:8,nemotron_threshold:0.5,pyannote_threshold:0.6}}}}
fn fake_result(user:Option<usize>)->DiarizationResult{{DiarizationResult{{segments:vec![DiarizationSegment{{start:0.,end:2.,speaker:0,overlapped:false}},DiarizationSegment{{start:2.,end:4.,speaker:1,overlapped:false}}],num_speakers:2,duration:4.,user_speaker:user}}}}
// This is the PRODUCTION voiceprint decision block, with deterministic embeddings.
fn model_result(trust_legacy_user:bool)->DiarizationResult{{
 let num_found=2; let embeddings=vec![vec![1f32,0.],vec![0.,1.]]; let labels=vec![0usize,1];
{user}
 fake_result(user_speaker)
}}
fn diarize_file_with_models_policy(_p:&Path,_d:&Path,_n:Option<usize>,_t:Option<f32>,trust:bool)->Result<DiarizationResult>{{if FAIL_ENGINE.load(std::sync::atomic::Ordering::SeqCst){{return Err(anyhow!("synthetic selected-engine failure"));}}Ok(model_result(trust))}}
fn diarize_file_with_models(p:&Path,d:&Path,n:Option<usize>,t:Option<f32>)->Result<DiarizationResult>{{diarize_file_with_models_policy(p,d,n,t,true)}}
fn diarize_file_with_engine(p:&Path,e:&str,n:Option<usize>,t:Option<f32>)->Result<DiarizationResult>{{diarize_file_with_engine_policy(p,e,n,t,true)}}
fn diarize_file_with_engine_policy(_p:&Path,_e:&str,_n:Option<usize>,_t:Option<f32>,trust:bool)->Result<DiarizationResult>{{if FAIL_ENGINE.load(std::sync::atomic::Ordering::SeqCst){{return Err(anyhow!("synthetic selected-engine failure"));}}Ok(model_result(trust))}}
mod dsp{{pub fn read_wav(_p:&std::path::Path)->anyhow::Result<(Vec<f32>,u32)>{{Ok((vec![],16000))}}}}
mod clustering{{pub fn agglomerative(_e:&[Vec<f32>],_n:Option<usize>,_t:f32)->Vec<usize>{{vec![0,0,0]}}}}
pub mod voiceprint{{
 pub static READS:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
 pub static WRITES:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
 pub fn match_cluster(_e:&[Vec<f32>])->Option<usize>{{READS.fetch_add(1,std::sync::atomic::Ordering::SeqCst);Some(0)}}
 pub fn update_from_embeddings(_e:&[Vec<f32>],_s:&str)->anyhow::Result<()>{{WRITES.fetch_add(1,std::sync::atomic::Ordering::SeqCst);Ok(())}}
}}
mod voice_profiles{{
 pub fn match_offline_speakers(_p:&std::path::Path,_s:&[super::DiarizationSegment])->anyhow::Result<std::collections::HashMap<usize,String>>{{Ok(Default::default())}}
 pub fn active_person_links()->Vec<(String,String)>{{vec![]}}
}}
mod nemotron{{
 pub const NEMOTRON_MODEL_FILENAME:&str="synthetic"; pub struct NemotronDiarizationModel;
 impl NemotronDiarizationModel{{pub fn new(_p:&std::path::Path,_n:usize,_t:f32)->anyhow::Result<Self>{{Ok(Self)}} pub fn diarize(&mut self,_s:&[f32],_sr:u32)->anyhow::Result<super::DiarizationResult>{{if super::FAIL_ENGINE.load(std::sync::atomic::Ordering::SeqCst){{return Err(anyhow::anyhow!("synthetic selected-engine failure"));}}Ok(super::fake_result(Some(0)))}} pub fn reset_streaming_state(&mut self){{}}}}
}}
''')
        (work / 'src/lib.rs').write_text(f'''#![allow(dead_code,unused_variables)]
extern crate self as tauri;
pub struct State<'a,T>(pub &'a T);
impl<'a,T> std::ops::Deref for State<'a,T>{{type Target=T;fn deref(&self)->&T{{self.0}}}}
mod state{{pub struct AppState{{pub db_manager:DbManager}} pub struct DbManager(pub sqlx::SqlitePool);impl DbManager{{pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}}}}}}
#[path="{NATIVE}/speaker_labels.rs"] mod speaker_labels;
pub mod command;
pub use command as diarization;
mod api{{#[derive(Clone,serde::Serialize,serde::Deserialize)]pub struct TranscriptSegment{{pub id:String,pub text:String,pub timestamp:String,pub audio_start_time:Option<f64>,pub audio_end_time:Option<f64>,pub duration:Option<f64>,pub speaker:Option<String>}}}}
mod audio{{#[path="{work}/src/retranscription.rs"]pub mod retranscription;pub mod recording_preferences{{pub fn get_default_recordings_folder()->std::path::PathBuf{{std::path::PathBuf::new()}}}}pub mod common{{pub fn write_transcripts_json(p:&std::path::Path,s:&[crate::api::TranscriptSegment])->anyhow::Result<()>{{std::fs::write(p.join("transcripts.json"),serde_json::to_vec(s)?)?;Ok(())}}}}}}
mod database{{#[path="{NATIVE}/database/transcript_versions.rs"]pub mod transcript_versions;pub mod repositories{{
#[path="{NATIVE}/database/repositories/speaker_identity.rs"]pub mod speaker_identity;
#[path="{work}/src/meeting_notes.rs"]pub mod meeting_notes;
#[path="{work}/src/notes_document.rs"]pub mod notes_document;
pub mod person{{pub fn is_person_name(s:&str)->bool{{crate::speaker_labels::is_manual_label(s)}}pub struct PeopleRepository;impl PeopleRepository{{pub async fn reconcile_speaker_identity(_tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>,_m:&str,_a:&str,_b:&str)->Result<(),sqlx::Error>{{Ok(())}}}}}}}}}}
#[cfg(test)]#[path="{ROOT}/frontend/tests/native/offline-identity.rs"] mod tests;
{combined_tests}
#[cfg(test)]pub async fn test_migrate(pool:&sqlx::SqlitePool){{
 let mut files:Vec<_>=std::fs::read_dir("{NATIVE.parent}/migrations").unwrap().map(|f|f.unwrap().path()).filter(|p|p.extension().is_some_and(|s|s=="sql")).collect();files.sort();
 for path in files{{sqlx::raw_sql(&std::fs::read_to_string(path).unwrap()).execute(pool).await.unwrap();}}
}}
''')
        subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-offline-identity' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
