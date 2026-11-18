#!/usr/bin/env python3
"""Exercise the actual native start/stop ownership boundary without desktop/DSP.

Includes production stop wrappers, pre-effect stop prefix and get_recording_state
body. Tauri transport, manager/device metadata and post-accept capture/save work
are stubbed. Engine lock, generation/owner authority and disable are real. The
baseline mode adds only an unused optional expected field to its old args type
so stale-generation behavior can fail at runtime rather than fail to compile.
"""
from pathlib import Path
import argparse
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / 'frontend/src-tauri/src/audio'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', action='store_true')
    args = parser.parse_args()
    source = (subprocess.check_output(['git', 'show', '7d3ff8a:frontend/src-tauri/src/audio/recording_commands.rs'], text=True)
              if args.baseline else (AUDIO / 'recording_commands.rs').read_text())
    types = source[source.index('#[derive(Debug, Deserialize)]\npub struct RecordingArgs'):source.index('#[derive(Debug, Serialize, Clone)]\npub struct TranscriptionStatus')]
    if args.baseline:
        types = types.replace('pub save_path: String,', 'pub save_path: String, pub expected: Option<super::recording_ownership::RecordingLease>,')
    wrappers = source[source.index('pub async fn stop_recording<R: Runtime>'):source.index('async fn stop_recording_inner<R: Runtime>')]
    prefix = source[source.index('async fn stop_recording_inner<R: Runtime>'):source.index('    if IS_STOPPING\n        .compare_exchange')]
    body = prefix + '''    EFFECTS.fetch_add(1, Ordering::SeqCst);
    IS_RECORDING.store(false, Ordering::SeqCst);
    Ok(StopOutcome::Completed)
}
'''
    snapshot = source[source.index('pub async fn get_recording_state()'):source.index('/// Get the meeting folder path for the current recording')]
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in ('tokio', 'serde', 'serde_json', 'log')}
    with tempfile.TemporaryDirectory(prefix='meetily-guarded-stop-') as directory:
        work = Path(directory); (work / 'src').mkdir()
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-guarded-stop"
version="0.0.0"
edition="2021"
[dependencies]
serde={{version="={versions['serde']}",features=["derive"]}}
serde_json="={versions['serde_json']}"
log="={versions['log']}"
tokio={{version="={versions['tokio']}",features=["full"]}}
''')
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        (work / 'src/ownership.rs').write_text((AUDIO / 'recording_ownership.rs').read_text().replace('#[tauri::command]', ''))
        (work / 'src/commands.rs').write_text('''use crate::{AppHandle,Runtime};
use serde::Deserialize;
use std::sync::{Mutex,atomic::{AtomicBool,AtomicU64,Ordering}};
macro_rules! info { ($($t:tt)*)=>{} }
macro_rules! log_info { ($($t:tt)*)=>{} }
pub static IS_RECORDING:AtomicBool=AtomicBool::new(false);
pub static IS_STOPPING:AtomicBool=AtomicBool::new(false);
pub static EFFECTS:AtomicU64=AtomicU64::new(0);
struct Manager;
impl Manager {
 fn is_paused(&self)->bool{false} fn get_state(&self)->&Self{self}
 fn is_microphone_muted(&self)->bool{false} fn is_system_audio_muted(&self)->bool{false}
 fn is_active(&self)->bool{true} fn get_recording_duration(&self)->Option<f64>{Some(1.)}
 fn get_active_recording_duration(&self)->Option<f64>{Some(1.)}
 fn get_total_pause_duration(&self)->f64{0.} fn get_current_pause_duration(&self)->Option<f64>{None}
}
static RECORDING_MANAGER:Mutex<Option<Manager>>=Mutex::new(None);
''' + types + wrappers + body + snapshot)
        (work / 'src/lib.rs').write_text(f'''#![allow(dead_code,unused_variables)]
pub trait Runtime {{}} impl Runtime for () {{}}
pub struct AppHandle<R:Runtime>(std::marker::PhantomData<R>);
mod audio {{
 #[path="{work}/src/ownership.rs"] pub mod recording_ownership;
 #[path="{AUDIO}/recording_mode.rs"] pub mod recording_mode;
 pub mod common {{
  static ENGINE:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
  pub async fn acquire_engine_lifecycle_lock()->tokio::sync::MutexGuard<'static,()> {{ENGINE.lock().await}}
 }}
 #[path="{work}/src/commands.rs"] pub mod recording_commands;
}}
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/guarded-recording-stop.rs"] mod tests;
''')
        subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-guarded-stop' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
