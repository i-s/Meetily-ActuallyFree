#!/usr/bin/env python3
"""Production native start prefix/commit and settling IPC bodies, synthetic capture.

No desktop/model/audio claims. Only transport, qualification fixture and device
setup/capture are stubbed. Engine serialization, ticket authority, lease commit
and resolve/cancel bodies are production code. Dependencies use repository lock.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / 'frontend/src-tauri/src/audio'


def main():
    source = (AUDIO / 'recording_commands.rs').read_text()
    start = source[source.index('pub async fn start_recording_with_lifecycle<R: Runtime>'):]
    prefix = start[:start.index('    crate::onnx_runtime::ensure_available()')]
    boundary = start[start.index('    // Model/device setup may outlive'):start.index('    let transcription_receiver = manager')]
    commit = start[start.index('    let recording_lease = super::recording_ownership::begin(lifecycle_owner);'):start.index('    // Live speaker identification:')]
    barriers = source[source.index('pub async fn resolve_automated_recording_start('):source.index('/// Stop recording with optimized graceful shutdown')].replace('#[tauri::command]', '')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in ('serde', 'serde_json', 'tokio', 'uuid')}
    with tempfile.TemporaryDirectory(prefix='meetily-start-authority-') as directory:
        work = Path(directory); (work / 'src').mkdir()
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-start-authority"
version="0.0.0"
edition="2021"
[dependencies]
serde={{version="={versions['serde']}",features=["derive"]}}
serde_json="={versions['serde_json']}"
tokio={{version="={versions['tokio']}",features=["full"]}}
uuid={{version="={versions['uuid']}",features=["v4"]}}
''')
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        for name in ('recording_ownership', 'recording_start_authority'):
            (work / f'src/{name}.rs').write_text((AUDIO / f'{name}.rs').read_text().replace('#[tauri::command]', ''))
        (work / 'src/commands.rs').write_text('''use crate::{AppHandle,Runtime};
use std::sync::atomic::{AtomicBool,AtomicU64,Ordering};
macro_rules! info { ($($t:tt)*)=>{} }
pub static IS_RECORDING:AtomicBool=AtomicBool::new(false);
pub static IS_STOPPING:AtomicBool=AtomicBool::new(false);
pub static CAPTURES:AtomicU64=AtomicU64::new(0);
pub static HOLD:AtomicBool=AtomicBool::new(false);
pub static FAIL:AtomicBool=AtomicBool::new(false);
pub static ENTERED:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(0);
pub static RELEASE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(0);
''' + prefix + '''
    if HOLD.load(Ordering::SeqCst) {
        ENTERED.add_permits(1);
        RELEASE.acquire().await.unwrap().forget();
    }
''' + boundary + '''
    if FAIL.load(Ordering::SeqCst) { return Err("Synthetic device startup failure".into()); }
    CAPTURES.fetch_add(1,Ordering::SeqCst);
''' + commit + '''
    drop(engine_lifecycle_guard);
    Ok(recording_lease)
}
''' + barriers)
        (work / 'src/lib.rs').write_text(f'''#![allow(dead_code,unused_variables)]
pub trait Runtime {{}} impl Runtime for () {{}}
pub struct AppHandle<R:Runtime>(std::marker::PhantomData<R>);
mod audio {{
 #[path="{work}/src/recording_ownership.rs"] pub mod recording_ownership;
 #[path="{work}/src/recording_start_authority.rs"] pub mod recording_start_authority;
 #[path="{work}/src/commands.rs"] pub mod recording_commands;
 pub mod common {{
  static ENGINE:tokio::sync::Mutex<()>=tokio::sync::Mutex::const_new(());
  pub async fn acquire_engine_lifecycle_lock()->tokio::sync::MutexGuard<'static,()> {{ENGINE.lock().await}}
 }}
}}
mod diarization{{pub mod online{{pub fn stop(){{}}}}}}
// Synthetic qualification ONLY; production validate_start has no qualified real classes.
mod meeting_detection {{ pub mod lifecycle {{
 pub fn validate_start(session:&str)->Result<(),String>{{
  if crate::audio::recording_ownership::automation_enabled() && session.starts_with("qualified-") {{Ok(())}} else {{Err("Unqualified fixture".into())}}
 }}
 pub fn session_owner(session:&str)->Option<crate::audio::recording_ownership::LifecycleOwner>{{
  Some(crate::audio::recording_ownership::LifecycleOwner{{source:if session.contains("discord"){{"discord"}}else{{"zoom"}}.into(), session_id:session.into(),app:"Synthetic".into(),process:"synthetic".into()}})
 }}
}}}}
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/recording-start-authority.rs"] mod tests;
''')
        subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-start-authority' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
