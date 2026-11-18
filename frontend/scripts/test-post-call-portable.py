#!/usr/bin/env python3
"""Small offline Rust/SQLite tests of production post-call modules; no desktop/model build."""
from pathlib import Path
import subprocess
import tempfile
import tomllib
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / 'frontend/src-tauri/src'
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
names = ('sqlx', 'chrono', 'tokio', 'tokio-util', 'uuid', 'serde', 'serde_json', 'tempfile', 'anyhow')
v = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in names}
api = (SRC / 'api/api.rs').read_text()
start = api.index('pub struct TranscriptSegment {')
segment = '#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n' + api[start:api.index('\n#[derive', start)]
with tempfile.TemporaryDirectory(prefix='meetily-post-call-') as directory:
    work = Path(directory)
    (work / 'src').mkdir()
    (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-post-call-portable"
version="0.0.0"
edition="2021"
[dependencies]
chrono="={v['chrono']}"
anyhow="={v['anyhow']}"
tokio={{version="={v['tokio']}",features=["full"]}}
tokio-util="={v['tokio-util']}"
sqlx={{version="={v['sqlx']}",default-features=false,features=["sqlite","runtime-tokio","chrono","derive"]}}
uuid={{version="={v['uuid']}",features=["v4"]}}
serde={{version="={v['serde']}",features=["derive"]}}
serde_json="={v['serde_json']}"
tempfile="={v['tempfile']}"
''')
    (work / 'Cargo.lock').write_bytes((ROOT / 'Cargo.lock').read_bytes())
    modules = '\n'.join(f'#[path="{SRC}/post_call/{n}.rs"] pub mod {n};' for n in ('store', 'summary', 'priority', 'runner'))
    (work / 'src/lib.rs').write_text(f'''pub mod api {{{segment}}}
{modules}
pub mod config {{pub const DEFAULT_PARAKEET_MODEL:&str="synthetic";}}
pub mod codex_cli {{#[path="{SRC}/codex_cli/vocabulary.rs"]pub mod vocabulary;}}
pub mod database {{pub mod repositories {{#[path="{SRC}/database/repositories/vocabulary.rs"]pub mod vocabulary;}}}}
#[path="{SRC}/diarization/batch_control.rs"] pub mod batch_control;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/post-call-fixture.rs"] mod test_schema;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/post-call.rs"] mod tests;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/post-call-summary.rs"] mod summary_tests;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/post-call-priority.rs"] mod priority_tests;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/post-call-runner.rs"] mod runner_tests;
''')
    subprocess.run(['cargo', 'test', '--offline', '-j', '1', '--manifest-path', str(work / 'Cargo.toml'), *sys.argv[1:], '--', '--test-threads=1'], check=True)
    allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
    actual = tomllib.loads((work / 'Cargo.lock').read_text())
    assert all(p['name'] == "meetily-post-call-portable" or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])
