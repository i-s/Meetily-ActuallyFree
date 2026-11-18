#!/usr/bin/env python3
"""Compile production Codex adapter/context/command bodies without desktop builds.

Only Tauri state transport, current-recorder ownership and Notes DB transport are
stubbed. Real command logic, subprocesses and text validation run unchanged.
This does not establish desktop ABI, SQLite integration or physical capture.
"""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import sys

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'frontend/src-tauri/src/codex_cli'
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
names = ('tokio', 'tokio-util', 'serde', 'serde_json', 'tempfile')
versions = {name: next(p['version'] for p in lock['package'] if p['name'] == name) for name in names}
with tempfile.TemporaryDirectory(prefix='meetily-codex-portable-') as directory:
    work = Path(directory)
    (work / 'src').mkdir()
    (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-codex-portable"
version="0.0.0"
edition="2021"
[dependencies]
tokio={{version="={versions['tokio']}",features=["full"]}}
tokio-util="={versions['tokio-util']}"
serde={{version="={versions['serde']}",features=["derive"]}}
serde_json="={versions['serde_json']}"
tempfile="={versions['tempfile']}"
''')
    (work / 'src/commands.rs').write_text((SOURCE / 'commands.rs').read_text().replace('#[tauri::command]', ''))
    recording = (ROOT / 'frontend/src-tauri/src/audio/recording_commands.rs').read_text()
    recording = recording[recording.index('pub(crate) fn current_text_clock()'):recording.index('/// Get accumulated transcript segments from current recording session')]
    modules = '\n'.join(f'#[path="{SOURCE}/{name}.rs"] pub mod {name};' for name in ('cli', 'context', 'jobs', 'service'))
    (work / 'src/lib.rs').write_text(modules + f'''
extern crate self as tauri;
pub struct State<'a,T>(pub &'a T);
impl<'a,T> std::ops::Deref for State<'a,T> {{type Target=T;fn deref(&self)->&T{{self.0}}}}
pub mod state {{pub struct AppState {{pub db_manager:DbManager}} pub struct DbManager;impl DbManager {{pub fn pool(&self)->&(){{&()}}}}}}
pub mod test_runtime {{
    use std::sync::Mutex;
    pub static ROOT:Mutex<Option<std::path::PathBuf>>=Mutex::new(None);
    pub static TARGETS:Mutex<Vec<String>>=Mutex::new(Vec::new());
    pub fn configure(root:&std::path::Path){{*ROOT.lock().unwrap()=Some(root.to_owned());TARGETS.lock().unwrap().clear();}}
    pub fn note_targets()->Vec<String>{{TARGETS.lock().unwrap().clone()}}
}}
pub mod paths {{pub fn install_data_root()->std::path::PathBuf{{crate::test_runtime::ROOT.lock().unwrap().clone().unwrap()}}}}
pub mod audio {{
  pub mod recording_saver {{
    #[derive(Clone)] pub struct TranscriptSegment {{pub sequence_id:u64,pub audio_start_time:f64,pub audio_end_time:f64,pub text:String,pub speaker:Option<String>}}
  }}
  pub mod recording_commands {{
    use std::sync::{{Mutex,LazyLock}};
    struct Manager;
    impl Manager {{
      fn get_meeting_folder(&self)->Option<std::path::PathBuf>{{Some("/synthetic/current".into())}}
      fn get_active_recording_duration(&self)->Option<f64>{{Some(600.0)}}
      #[allow(dead_code)] fn get_recording_duration(&self)->Option<f64>{{Some(1200.0)}}
      fn get_transcript_segments(&self)->Vec<super::recording_saver::TranscriptSegment>{{vec![super::recording_saver::TranscriptSegment{{sequence_id:1,audio_start_time:12.0,audio_end_time:15.0,text:"Budget 42".into(),speaker:Some("You".into())}}]}}
    }}
    static RECORDING_MANAGER:LazyLock<Mutex<Option<Manager>>>=LazyLock::new(||Mutex::new(Some(Manager)));
    fn is_recording_active()->bool{{true}}
    fn is_recording_stopping()->bool{{false}}
    {recording}
  }}
}}
pub mod database {{pub mod repositories {{pub mod notes_document {{
    pub struct Notes {{pub markdown:Option<String>,pub revision:i64}}
    pub async fn read(_pool:&(),target:&str)->Result<Option<Notes>,String>{{crate::test_runtime::TARGETS.lock().unwrap().push(target.into());Ok(Some(Notes{{markdown:Some("My idea, not agreed".into()),revision:7}}))}}
}}}}}}
#[path="{work}/src/commands.rs"] pub mod commands;
#[cfg(all(test,unix))] #[path="{ROOT}/frontend/tests/native/codex-commands.rs"] mod command_tests;
''')
    (work / 'Cargo.lock').write_bytes((ROOT / 'Cargo.lock').read_bytes())
    args = sys.argv[1:]
    test_args = [arg for arg in args if arg in ('--ignored', '--nocapture')]
    cargo_args = [arg for arg in args if arg not in test_args]
    subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), *cargo_args, '--', '--test-threads=1', *test_args], check=True)
    allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
    actual = tomllib.loads((work / 'Cargo.lock').read_text())
    assert all(p['name'] == 'meetily-codex-portable' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])
