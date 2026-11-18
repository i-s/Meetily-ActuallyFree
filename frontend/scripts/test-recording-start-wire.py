#!/usr/bin/env python3
"""Check frontend fixtures against the pinned #10 production Rust serializers.

Requires Cargo/Rust and the pinned commit object, not a desktop build. Reads
sibling code via git show only; never checks it out, modifies it or runs capture.
"""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / 'frontend/tests/fixtures/recording-start-v1.json'


def main():
    fixture = json.loads(FIXTURE.read_text())
    ref = fixture['native_commit']
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n)
                for n in ('serde', 'serde_json', 'uuid')}
    with tempfile.TemporaryDirectory(prefix='meetily-start-wire-') as directory:
        work = Path(directory)
        (work / 'src').mkdir()
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-start-wire"
version="0.0.0"
edition="2021"
[dependencies]
serde={{version="={versions['serde']}",features=["derive"]}}
serde_json="={versions['serde_json']}"
uuid={{version="={versions['uuid']}",features=["v4"]}}
''')
        (work / 'Cargo.lock').write_bytes((ROOT / 'Cargo.lock').read_bytes())
        for name in ('recording_ownership', 'recording_start_authority'):
            source = subprocess.check_output(['git', 'show', f'{ref}:frontend/src-tauri/src/audio/{name}.rs'], cwd=ROOT, text=True)
            (work / f'src/{name}.rs').write_text(source.replace('#[tauri::command]', ''))
        (work / 'src/main.rs').write_text(r'''#![allow(dead_code)]
mod recording_ownership;
mod recording_start_authority;
use recording_ownership::RecordingLease;
use recording_start_authority::{StartCapability,StartRequest,StartResolution,StartOutcome};
fn main() {
    let fixture:serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let c=&fixture["capability"];
    let capability=StartCapability{version:1,epoch:c["epoch"].as_str().unwrap().into(),next_sequence:c["next_sequence"].as_str().unwrap().into()};
    assert_eq!(serde_json::to_value(capability).unwrap(),*c);
    let expected:StartRequest=serde_json::from_value(fixture["request"].clone()).unwrap();
    assert_eq!(serde_json::to_value(&expected).unwrap(),fixture["request"]);
    for value in fixture["resolutions"].as_array().unwrap() {
        let outcome=match value["outcome"].as_str().unwrap() {
            "committed"=>StartOutcome::Committed,"cancelled"=>StartOutcome::Cancelled,
            "failed"=>StartOutcome::Failed,"rejected"=>StartOutcome::Rejected,
            "retired"=>StartOutcome::Retired,"epoch_mismatch"=>StartOutcome::EpochMismatch,_=>panic!("bad outcome")};
        let receipt=StartResolution{version:1,native_epoch:value["native_epoch"].as_str().unwrap().into(),
            expected:expected.clone(),terminal:true,no_future_commit:true,outcome,
            committed_lease:serde_json::from_value::<Option<RecordingLease>>(value["committed_lease"].clone()).unwrap(),
            recording_lease:serde_json::from_value::<Option<RecordingLease>>(value["recording_lease"].clone()).unwrap(),
            is_recording:value["is_recording"].as_bool().unwrap(),is_stopping:value["is_stopping"].as_bool().unwrap()};
        assert_eq!(serde_json::to_value(receipt).unwrap(),*value);
    }
    println!("Pinned production native serializers: capability, exact request and {} resolution fixtures match",fixture["resolutions"].as_array().unwrap().len());
}
''')
        subprocess.run(['cargo', 'run', '--manifest-path', str(work / 'Cargo.toml'), '--offline', '--', str(FIXTURE)], cwd=ROOT, check=True, env=os.environ)


if __name__ == '__main__':
    main()
