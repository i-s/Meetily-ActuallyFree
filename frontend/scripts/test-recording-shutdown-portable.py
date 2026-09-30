#!/usr/bin/env python3
"""Device-free tests of the actual encoder/state modules (no desktop linking)."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
AUDIO = REPO / "frontend/src-tauri/src/audio"


def main():
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    names = ("anyhow", "log", "tokio", "tracing")
    versions = {name: next(p["version"] for p in lock["package"] if p["name"] == name) for name in names}
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise SystemExit("Install FFmpeg to run the real 35-second encoding test")
    with tempfile.TemporaryDirectory(prefix="meetily-shutdown-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        (work / "Cargo.toml").write_text(
            '[package]\nname="meetily-shutdown-portable"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n'
            + '\n'.join(f'{name}="={versions[name]}"' for name in ("anyhow", "log", "tracing"))
            + f'\ntokio={{version="={versions["tokio"]}",features=["full","test-util"]}}\n'
        )
        shutil.copyfile(REPO / "Cargo.lock", work / "Cargo.lock")
        (work / "src/lib.rs").write_text(f'''#![allow(dead_code)]
mod audio {{
    // Device metadata is replaced; production encoder and state are included intact.
    pub mod devices {{
        #[derive(Clone)] pub struct AudioDevice {{ pub name: String }}
        pub async fn list_audio_devices() -> anyhow::Result<Vec<AudioDevice>> {{ Ok(vec![]) }}
    }}
    #[path="{AUDIO}/buffer_pool.rs"] pub mod buffer_pool;
    #[path="{AUDIO}/recording_state.rs"] pub mod recording_state;
    pub use devices::AudioDevice;
    pub mod ffmpeg {{
        pub fn find_ffmpeg_path() -> Option<std::path::PathBuf> {{ Some("{ffmpeg}".into()) }}
    }}
    #[path="{AUDIO}/encode.rs"] pub mod encode;
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
