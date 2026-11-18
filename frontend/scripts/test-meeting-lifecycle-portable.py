#!/usr/bin/env python3
"""Portable tests of the production Zoom classifier/tracker/lifecycle state.

Requires Python 3.11+, Cargo/Rust. The temporary lifecycle copy removes only
Tauri command annotations; command bodies, serde payloads, and native session
ownership are unchanged. This does not compile macOS AX FFI, link the desktop
app, exercise IPC/device capture, or establish real Zoom signal reliability.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
DETECTION = REPO / "frontend/src-tauri/src/meeting_detection"


def main():
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    versions = {name: next(p["version"] for p in lock["package"] if p["name"] == name)
                for name in ("serde", "serde_json")}
    with tempfile.TemporaryDirectory(prefix="meetily-lifecycle-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        (work / "Cargo.toml").write_text(
            '[package]\nname="meetily-lifecycle-portable"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n'
            + f'serde={{version="={versions["serde"]}",features=["derive"]}}\n'
            + f'serde_json="={versions["serde_json"]}"\n'
        )
        shutil.copyfile(REPO / "Cargo.lock", work / "Cargo.lock")
        lifecycle = (DETECTION / "lifecycle.rs").read_text().replace("#[tauri::command]", "")
        (work / "src/lifecycle.rs").write_text(lifecycle)
        for name, path in [("ownership", REPO / "frontend/src-tauri/src/audio/recording_ownership.rs"),
                           ("zoom_observation", DETECTION / "zoom_observation.rs")]:
            (work / f"src/{name}.rs").write_text(path.read_text().replace("#[tauri::command]", ""))
        (work / "src/lib.rs").write_text(f'''#![allow(dead_code)]
fn zoom_automation_enabled()->bool {{ true }}
fn publish_automation_observation(source:&str,qualified:bool,presence:audio::recording_ownership::SourcePresence<'_>) {{
 audio::recording_ownership::observe_source(source,true,qualified,presence);
}}
static MONITOR_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[path="{DETECTION}/evidence.rs"] mod evidence;
#[path="{DETECTION}/zoom.rs"] mod zoom;
mod lifecycle;
mod zoom_observation;
#[path="{REPO}/frontend/src-tauri/src/speaker_labels.rs"] mod speaker_labels;
#[path="{REPO}/frontend/src-tauri/src/diarization/identity_policy.rs"] mod identity_policy;
mod audio {{ #[path="{work}/src/ownership.rs"] pub mod recording_ownership; }}
#[path="{REPO}/frontend/src-tauri/src/audio/recording_mode.rs"] mod recording_mode;
''')
        subprocess.run(["cargo", "test", "--offline", "--manifest-path", str(work / "Cargo.toml"),
                        "--", "--test-threads=1"], check=True)
        original = {(p["name"], p["version"], p.get("source")) for p in lock["package"]}
        actual = tomllib.loads((work / "Cargo.lock").read_text())
        unexpected = [p for p in actual["package"] if p["name"] != "meetily-lifecycle-portable"
                      and (p["name"], p["version"], p.get("source")) not in original]
        if unexpected:
            raise SystemExit(f"Dependencies outside repository lock: {unexpected}")


if __name__ == "__main__":
    main()
