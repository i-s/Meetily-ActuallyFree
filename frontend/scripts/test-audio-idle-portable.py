#!/usr/bin/env python3
"""Run device-free stream/mixer regressions without linking the desktop app.

Usage: python3 frontend/scripts/test-audio-idle-portable.py
Requires Python 3.11+, Cargo/Rust, and access to the pinned Cargo dependencies.
The temporary crate includes the production processing module, RecordingState,
and buffer pool directly. It extracts the mixer and its 10 applicable tests
verbatim from pipeline.rs; it does not reimplement any audio behavior.

This checks 7 stream, 10 mixer, and 3 buffer-pool tests on Linux. It deliberately
excludes desktop/DSP-dependent pipeline tests and the macOS AudioStream::stop
wrapper test; the native macOS workflow runs the complete production groups.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
AUDIO = REPO / "frontend/src-tauri/src/audio"


def between(text: str, start: str, end: str) -> str:
    begin = text.index(start)
    return text[begin:text.index(end, begin)]


def main() -> None:
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    versions = {
        name: next(p["version"] for p in lock["package"] if p["name"] == name)
        for name in ("anyhow", "log", "futures-util", "futures", "tokio")
    }
    pipeline = (AUDIO / "pipeline.rs").read_text()
    ring = between(pipeline, "struct AudioMixerRingBuffer", "/// Mixes mic + system")
    tests = between(
        pipeline,
        "    #[tokio::test(start_paused = true)]\n    async fn core_audio_partial_block",
        "    #[test]\n    fn completed_vad_segments_are_queued",
    )
    with tempfile.TemporaryDirectory(prefix="meetily-audio-idle-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        dependencies = "\n".join(
            f'{name} = "={versions[name]}"'
            for name in ("anyhow", "log", "futures-util", "futures")
        )
        (work / "Cargo.toml").write_text(
            '[package]\nname = "meetily-audio-idle-portable"\n'
            'version = "0.0.0"\nedition = "2021"\n[dependencies]\n'
            + dependencies
            + f'\ntokio = {{ version = "={versions["tokio"]}", features = ["full", "test-util"] }}\n'
        )
        # Cargo removes unused workspace entries and adds this harness root,
        # retaining the locked versions of transitive dependencies as well.
        shutil.copyfile(REPO / "Cargo.lock", work / "Cargo.lock")
        (work / "src/ring.rs").write_text(
            'use std::collections::VecDeque;\nuse std::sync::Arc;\n'
            'use log::{info,debug,warn,error};\nuse tokio::sync::mpsc;\n'
            'use super::recording_state::{DeviceType,RecordingState};\n'
            + ring + '\n#[cfg(test)] mod tests { use super::*;\n' + tests + '}\n'
        )
        (work / "src/lib.rs").write_text(f'''#![allow(dead_code, unused_imports)]
mod audio {{
    // Device metadata is never constructed by the selected tests. Native device
    // enumeration remains outside this portable harness.
    pub mod devices {{ pub struct AudioDevice {{ pub name: String }} }}
    #[path = "{AUDIO}/buffer_pool.rs"] pub mod buffer_pool;
    #[path = "{AUDIO}/recording_state.rs"] pub mod recording_state;
    pub mod stream {{
        #[path = "{AUDIO}/stream/core_audio_processing.rs"] pub mod core_audio_processing;
    }}
    #[path = "{work}/src/ring.rs"] pub mod pipeline;
}}
''')
        # Isolate the cfg(macOS) AudioStream wrapper test when this *portable*
        # harness itself runs on macOS: use the native Cargo filter there.
        import sys
        if sys.platform == "darwin":
            raise SystemExit("On macOS run native cargo test filters audio::stream and audio::pipeline.")
        subprocess.run(
            ["cargo", "test", "--manifest-path", str(work / "Cargo.toml"),
             "--", "--test-threads=1"], check=True,
        )
        original = {(p["name"], p["version"], p.get("source")) for p in lock["package"]}
        actual = tomllib.loads((work / "Cargo.lock").read_text())
        unexpected = [p for p in actual["package"]
                      if p["name"] != "meetily-audio-idle-portable"
                      and (p["name"], p["version"], p.get("source")) not in original]
        if unexpected:
            raise SystemExit(f"Harness resolved dependencies outside repository lock: {unexpected}")


if __name__ == "__main__":
    main()
