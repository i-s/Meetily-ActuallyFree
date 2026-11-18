#!/usr/bin/env python3
"""Device-free tests of production encoder/state/savers with real FFmpeg.

Requires Python 3.11+, Cargo/Rust and FFmpeg on PATH. Dependencies must stay
inside the repository lock. Includes production modules intact except for
removing Tauri command annotations from the temporary incremental-saver copy.
Tauri event emission, device metadata and the default folder lookup are stubbed;
folder creation functions are extracted verbatim (no desktop/DSP linking).
This exercises neither macOS capture nor native Tauri commands/IPC, models,
ASR speech accuracy, database persistence, or real disk-full hardware behavior.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
AUDIO = REPO / "frontend/src-tauri/src/audio"


def main():
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    names = ("anyhow", "log", "tokio", "tracing", "serde", "serde_json", "chrono", "tempfile", "bytemuck")
    versions = {name: next(p["version"] for p in lock["package"] if p["name"] == name) for name in names}
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise SystemExit("Install FFmpeg to run the real 35-second encoding test")
    with tempfile.TemporaryDirectory(prefix="meetily-shutdown-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        (work / "Cargo.toml").write_text(
            '[package]\nname="meetily-shutdown-portable"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n'
            + '\n'.join(f'{name}="={versions[name]}"' for name in ("anyhow", "log", "tracing", "serde_json", "chrono", "tempfile", "bytemuck"))
            + f'\nserde={{version="={versions["serde"]}",features=["derive"]}}\n'
            + f'\ntokio={{version="={versions["tokio"]}",features=["full","test-util"]}}\n'
        )
        shutil.copyfile(REPO / "Cargo.lock", work / "Cargo.lock")
        incremental = (AUDIO / "incremental_saver.rs").read_text().replace("#[tauri::command]", "")
        (work / "src/incremental_saver.rs").write_text(incremental)
        processing = (AUDIO / "audio_processing.rs").read_text()
        folder_functions = processing[processing.index("pub fn sanitize_filename"):processing.index("pub fn normalize_v2")]
        (work / "src/audio_processing.rs").write_text("use std::path::PathBuf; use anyhow::Result; use chrono::Utc;\n" + folder_functions)
        (work / "src/lib.rs").write_text(f'''#![allow(dead_code)]
extern crate self as tauri;
pub trait Runtime {{}}
impl Runtime for () {{}}
pub struct AppHandle<R: Runtime>(pub std::marker::PhantomData<R>);
pub trait Emitter {{
    fn emit<S: serde::Serialize + Clone>(&self, _name: &str, _payload: S) -> Result<(), String> {{ Ok(()) }}
}}
impl<R: Runtime> Emitter for AppHandle<R> {{}}
mod audio {{
    // Device metadata is replaced; production encoder and state are included intact.
    pub mod devices {{
        #[derive(Clone)] pub struct AudioDevice {{ pub name: String }}
        pub async fn list_audio_devices() -> anyhow::Result<Vec<AudioDevice>> {{ Ok(vec![]) }}
    }}
    #[path="{AUDIO}/buffer_pool.rs"] pub mod buffer_pool;
    #[path="{AUDIO}/recording_state.rs"] pub mod recording_state;
    pub use devices::AudioDevice;
    pub mod recording_preferences {{
        pub fn get_default_recordings_folder() -> std::path::PathBuf {{ panic!("tests must set an isolated recordings folder") }}
    }}
    pub mod ffmpeg {{
        pub fn find_ffmpeg_path() -> Option<std::path::PathBuf> {{ Some("{ffmpeg}".into()) }}
    }}
    #[path="{AUDIO}/encode.rs"] pub mod encode;
    #[path="{AUDIO}/recording_mode.rs"] pub mod recording_mode;
    #[path="{work}/src/incremental_saver.rs"] pub mod incremental_saver;
    #[path="{work}/src/audio_processing.rs"] pub mod audio_processing;
    #[path="{AUDIO}/recording_saver.rs"] pub mod recording_saver;
}}
''')
        subprocess.run(["cargo", "test", "--manifest-path", str(work / "Cargo.toml"), "--", "--test-threads=1"], check=True)
        original = {(p["name"], p["version"], p.get("source")) for p in lock["package"]}
        actual = tomllib.loads((work / "Cargo.lock").read_text())
        unexpected = [p for p in actual["package"] if p["name"] != "meetily-shutdown-portable" and (p["name"], p["version"], p.get("source")) not in original]
        if unexpected:
            raise SystemExit(f"Dependencies outside repository lock: {unexpected}")


if __name__ == "__main__":
    main()
