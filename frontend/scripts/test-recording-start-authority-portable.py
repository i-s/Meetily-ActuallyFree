#!/usr/bin/env python3
"""Production start/Stop authorization and settling bodies; synthetic devices.

The same generated crate serves native regressions and the combined frontend RPC
harness. Source dispatch, Zoom validation and Discord qualified_owner are actual
production code. Only test-built setters can inject qualified source snapshots.
No desktop/model/audio/physical qualification is implied.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / 'frontend/src-tauri/src/audio'


def prepare(work):
    source = (AUDIO / 'recording_commands.rs').read_text()
    start = source[source.index('pub async fn start_recording_with_lifecycle<R: Runtime>'):]
    prefix = start[:start.index('    crate::onnx_runtime::ensure_available()')]
    boundary = start[start.index('    // Model/device setup may outlive'):start.index('    let transcription_receiver = manager')]
    commit = start[start.index('    let recording_lease = match stop_admission'):start.index('    // Live speaker identification:')]
    barriers = source[source.index('pub async fn resolve_automated_recording_start('):source.index('/// Stop recording with optimized graceful shutdown')].replace('#[tauri::command]', '')
    types = source[source.index('#[derive(Debug, Deserialize)]\npub struct RecordingArgs'):source.index('#[derive(Debug, Serialize, Clone)]\npub struct TranscriptionStatus')]
    stop = source[source.index('pub async fn stop_recording<R: Runtime>'):source.index('    if IS_STOPPING\n        .compare_exchange')]
    snapshot = source[source.index('pub async fn get_recording_state()'):source.index('/// Get the meeting folder path for the current recording')]
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    names = ('serde', 'serde_json', 'tokio', 'tokio-util', 'uuid', 'log')
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in names}
    (work / 'src').mkdir()
    (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-start-authority"
version="0.0.0"
edition="2021"
[dependencies]
serde={{version="={versions['serde']}",features=["derive"]}}
serde_json="={versions['serde_json']}"
tokio={{version="={versions['tokio']}",features=["full"]}}
tokio-util="={versions['tokio-util']}"
uuid={{version="={versions['uuid']}",features=["v4"]}}
log="={versions['log']}"
''')
    shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
    for name in ('recording_ownership', 'recording_start_authority'):
        (work / f'src/{name}.rs').write_text((AUDIO / f'{name}.rs').read_text().replace('#[tauri::command]', ''))
    lifecycle = (ROOT / 'frontend/src-tauri/src/meeting_detection/lifecycle.rs').read_text().replace('#[tauri::command]', '')
    lifecycle += '''
// Compiled ONLY into this synthetic test crate, never the application.
pub fn fixture(session:Option<String>, presence:String, qualified:bool) {
 let mut state=STATE.lock().unwrap(); state.snapshot.enabled=true;
 state.monitor_generation=super::MONITOR_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
 state.snapshot.automation_enabled=super::zoom_automation_enabled(); state.snapshot.revision+=1;
 let previous=state.snapshot.sources.iter().find(|s|s.source=="zoom").and_then(|s|s.session_id.as_deref());
 let observed=match (presence.as_str(),session.as_deref(),previous) {
  ("active",Some(id),_)=>SourcePresence::Active(id),
  ("inactive",None,Some(id))=>SourcePresence::End(id),
  ("inactive",None,None)=>SourcePresence::Inactive,
  _=>SourcePresence::Unknown,
 };
 super::publish_automation_observation("zoom",qualified,observed);
 state.snapshot.sources=vec![SourceState{source:"zoom".into(),app:"Zoom".into(),process:"us.zoom.xos".into(),presence,reason:"synthetic".into(),session_id:session,qualified}];
}
'''
    (work / 'src/lifecycle.rs').write_text(lifecycle)
    (work / 'src/zoom_observation.rs').write_text((ROOT / 'frontend/src-tauri/src/meeting_detection/zoom_observation.rs').read_text().replace('#[tauri::command]', ''))
    discord = (ROOT / 'frontend/src-tauri/src/discord_automation/mod.rs').read_text()
    owner = discord[discord.index('pub fn qualified_owner('):discord.index('fn journal_json(')]
    (work / 'src/discord.rs').write_text('''use std::sync::Mutex;
#[derive(Clone,serde::Serialize,serde::Deserialize)]
pub struct Status{pub enabled:bool,pub qualified:bool,pub state:String,pub session_id:Option<String>,pub transition:String}
static STATUS:Mutex<Option<Status>>=Mutex::new(None);
pub fn fixture(value:Status){
 use crate::audio::recording_ownership::SourcePresence;
 crate::meeting_detection::fixture_permission("discord",value.enabled);
 let presence=match (value.state.as_str(),value.transition.as_str(),value.session_id.as_deref()) {
  ("active",_,Some(id))=>SourcePresence::Active(id),
  ("inactive","end",Some(id))=>SourcePresence::End(id),
  ("inactive",_,None)=>SourcePresence::Inactive,
  _=>SourcePresence::Unknown,
 };
 crate::meeting_detection::publish_automation_observation("discord",value.qualified,presence);
 *STATUS.lock().unwrap()=Some(value);
}
pub fn get_discord_automation_status()->Status{STATUS.lock().unwrap().clone().unwrap_or(Status{enabled:false,qualified:false,state:"unknown".into(),session_id:None,transition:"none".into()})}
''' + owner)
    detection = (ROOT / 'frontend/src-tauri/src/meeting_detection.rs').read_text()
    permissions = detection[detection.index('fn source_automation_enabled('):detection.index('/// Payload emitted')]
    settings_types = detection[detection.index('#[derive(Clone, Debug, Serialize, Deserialize)]'):detection.index('/// Map a matched keyword')]
    monitor = detection[detection.index('fn start_monitor<R: Runtime>'):detection.index('/// Called once at app startup')]
    payloads = detection[detection.index('/// Payload emitted'):detection.index('/// True for our own process')]
    settings_commands = detection[detection.index('pub async fn get_meeting_detection_settings()'):].replace('#[tauri::command]', '')
    manager = '''
struct Manager;
impl Manager {
 fn is_paused(&self)->bool{false} fn get_state(&self)->&Self{self}
 fn active_capture_sources(&self)->Option<(bool,bool)>{Some((true,true))}
 fn is_microphone_muted(&self)->bool{false} fn is_system_audio_muted(&self)->bool{false}
 fn is_active(&self)->bool{true} fn get_recording_duration(&self)->Option<f64>{Some(1.)}
 fn get_active_recording_duration(&self)->Option<f64>{Some(1.)}
 fn get_total_pause_duration(&self)->f64{0.} fn get_current_pause_duration(&self)->Option<f64>{None}
}
static RECORDING_MANAGER:std::sync::Mutex<Option<Manager>>=std::sync::Mutex::new(None);
'''
    (work / 'src/commands.rs').write_text('''use crate::{AppHandle,Runtime};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool,AtomicU64,Ordering};
macro_rules! info { ($($t:tt)*)=>{} }
pub static IS_RECORDING:AtomicBool=AtomicBool::new(false);
pub static IS_STOPPING:AtomicBool=AtomicBool::new(false);
pub static CAPTURES:AtomicU64=AtomicU64::new(0);
pub static SAVES:AtomicU64=AtomicU64::new(0);
pub static HOLD:AtomicBool=AtomicBool::new(false);
pub static HOLD_COMMIT:AtomicBool=AtomicBool::new(false);
pub static FAIL:AtomicBool=AtomicBool::new(false);
pub static ENTERED:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(0);
pub static RELEASE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(0);
''' + manager + prefix + '''
    if HOLD.load(Ordering::SeqCst) {
        ENTERED.add_permits(1);
        RELEASE.acquire().await.unwrap().forget();
    }
''' + boundary + '''
    if FAIL.load(Ordering::SeqCst) { return Err("Synthetic device startup failure".into()); }
    CAPTURES.fetch_add(1,Ordering::SeqCst);
    if HOLD_COMMIT.load(Ordering::SeqCst) {
        ENTERED.add_permits(1);
        RELEASE.acquire().await.unwrap().forget();
    }
''' + commit + '''
    drop(engine_lifecycle_guard);
    Ok(recording_lease)
}
''' + barriers + types + stop + '''
    // Test-only stand-in for post-accept capture drain/FFmpeg/save effects.
    // Prefix above is unchanged production authorization, not simulated in JS.
    IS_STOPPING.store(true,Ordering::SeqCst);
    IS_RECORDING.store(false,Ordering::SeqCst);
    SAVES.fetch_add(1,Ordering::SeqCst);
    if !args.save_path.is_empty(){std::fs::write(&args.save_path,b"synthetic saved take").map_err(|e|e.to_string())?;}
    IS_STOPPING.store(false,Ordering::SeqCst);
    Ok(StopOutcome::Completed)
}
''' + snapshot)
    (work / 'src/lib.rs').write_text(f'''#![allow(dead_code,unused_variables)]
extern crate self as tauri;
pub trait Runtime:Send+Sync+'static {{}} impl Runtime for () {{}}
pub struct AppHandle<R:Runtime>(pub std::marker::PhantomData<R>);
impl<R:Runtime> Clone for AppHandle<R>{{fn clone(&self)->Self{{Self(std::marker::PhantomData)}}}}
pub static MONITOR_SPAWNS:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
pub static EVENTS:std::sync::Mutex<Vec<serde_json::Value>>=std::sync::Mutex::new(Vec::new());
pub trait Emitter {{fn emit<S:serde::Serialize+Clone>(&self,name:&str,value:S)->Result<(),String>{{
 EVENTS.lock().unwrap().push(serde_json::json!({{"name":name,"payload":value}}));Ok(())
}}}}
impl<R:Runtime> Emitter for AppHandle<R>{{}}
pub mod async_runtime {{pub fn spawn<F:std::future::Future+Send+'static>(_:F){{super::MONITOR_SPAWNS.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}}}}
pub mod post_call {{
 #[path="{ROOT}/frontend/src-tauri/src/post_call/priority.rs"]pub mod priority;
 pub mod worker {{pub static PRIORITY:std::sync::LazyLock<std::sync::Arc<super::priority::Priority>>=std::sync::LazyLock::new(||std::sync::Arc::new(super::priority::Priority::default()));}}
}}
pub mod audio {{
 #[path="{work}/src/recording_ownership.rs"] pub mod recording_ownership;
 #[path="{work}/src/recording_start_authority.rs"] pub mod recording_start_authority;
 #[path="{AUDIO}/recording_start_sources.rs"] pub mod recording_start_sources;
 #[path="{AUDIO}/recording_mode.rs"] pub mod recording_mode;
 #[path="{work}/src/commands.rs"] pub mod recording_commands;
 pub mod common {{
  static ENGINE:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
  pub static QUEUED:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(0);
  pub async fn acquire_engine_lifecycle_lock()->tokio::sync::MutexGuard<'static,()> {{QUEUED.add_permits(1);ENGINE.lock().await}}
 }}
}}
mod diarization{{pub mod online{{pub fn stop(){{}}}}}}
pub mod meeting_detection {{
 use std::sync::atomic::{{AtomicBool,Ordering}};
 use serde::{{Serialize,Deserialize}};
 use crate::{{AppHandle,Emitter,Runtime}};
 {settings_types}
 static SETTINGS:std::sync::LazyLock<std::sync::Mutex<Option<MeetingDetectionSettings>>>=std::sync::LazyLock::new(||std::sync::Mutex::new(Some(MeetingDetectionSettings{{enabled:true,zoom_automation:true,discord_automation:true,..MeetingDetectionSettings::default()}})));
 static MONITOR_PAUSED:AtomicBool=AtomicBool::new(false);
 // Fixture transport feeds the actual production settings publication body.
 pub struct OptIn;
 pub static OPT_IN:OptIn=OptIn;
 impl OptIn {{pub fn store(&self,enabled:bool,_order:Ordering){{fixture_permission("zoom",enabled);}}}}
 pub fn fixture_permission(source:&str,enabled:bool){{
  let mut settings=SETTINGS.lock().unwrap().clone().unwrap();
  if source=="zoom"{{settings.zoom_automation=enabled;}}else{{settings.discord_automation=enabled;}}
  apply_automation_settings(&settings);
 }}
 {permissions}
 {payloads}
 fn save_settings_to_disk(_settings:&MeetingDetectionSettings)->Result<(),String>{{Ok(())}}
 fn load_settings_from_disk()->MeetingDetectionSettings{{MeetingDetectionSettings::default()}}
 fn scan_for_meeting_app(_settings:&MeetingDetectionSettings)->Option<(String,String,bool)>{{None}}
 {monitor}
 {settings_commands}
 static MONITOR_GENERATION:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
 #[path="{ROOT}/frontend/src-tauri/src/meeting_detection/evidence.rs"] pub mod evidence;
 #[path="{work}/src/zoom_observation.rs"] pub mod zoom_observation;
 #[path="{work}/src/lifecycle.rs"] pub mod lifecycle;
}}
#[path="{work}/src/discord.rs"] pub mod discord_automation;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/recording-start-authority.rs"] mod tests;
''')
    return lock


def main():
    with tempfile.TemporaryDirectory(prefix='meetily-start-authority-') as directory:
        work = Path(directory)
        lock = prepare(work)
        subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-start-authority' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
