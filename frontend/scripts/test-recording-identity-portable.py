#!/usr/bin/env python3
"""Run production recording persistence SQL without desktop/audio dependencies.

The people dependency uses SQL-only rename/reassign stubs and no-op contact
linking. This harness qualifies recording IDs, writer ordering, display aliases,
and notes retention; full native tests cover the production contacts repository.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'frontend/src-tauri/src'
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
versions = {name: next(p['version'] for p in lock['package'] if p['name'] == name) for name in ('chrono', 'sqlx', 'tokio', 'tracing', 'uuid')}
with tempfile.TemporaryDirectory(prefix='meetily-recording-identity-') as directory:
    work = Path(directory)
    (work / 'src').mkdir()
    shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
    deps = '\n'.join(f'{name}={{version="={versions[name]}",features={features}}}' for name, features in [('chrono','["serde"]'),('sqlx','["runtime-tokio","sqlite","chrono"]'),('tokio','["full"]'),('tracing','[]'),('uuid','["v4"]')])
    (work / 'Cargo.toml').write_text('[package]\nname="meetily-recording-identity-test"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n' + deps)
    (work / 'src/lib.rs').write_text('''#![allow(dead_code)]
mod api {
 pub struct TranscriptSearchResult { pub id:String, pub title:String, pub match_context:String, pub timestamp:String }
 pub struct TranscriptSegment { pub id:String, pub text:String, pub timestamp:String, pub audio_start_time:Option<f64>, pub audio_end_time:Option<f64>, pub duration:Option<f64>, pub speaker:Option<String> }
}
mod database { pub mod repositories { pub mod person {
 pub struct PeopleRepository;
 pub struct SpeakerRenameOutcome { pub count:u64, pub speaker:String, pub removed_name:bool }
 impl PeopleRepository {
 pub async fn rename_meeting_speaker(pool:&sqlx::SqlitePool, id:&str, from:&str, to:&str) -> Result<SpeakerRenameOutcome,sqlx::Error> {
 let result=sqlx::query("UPDATE transcripts SET speaker=? WHERE meeting_id=? AND speaker=?").bind(to).bind(id).bind(from).execute(pool).await?;
 Ok(SpeakerRenameOutcome { count:result.rows_affected(), speaker:to.into(), removed_name:false }) }
 pub async fn reassign_transcript_speaker(pool:&sqlx::SqlitePool, id:&str, turn:&str, to:&str) -> Result<SpeakerRenameOutcome,sqlx::Error> {
 let result=sqlx::query("UPDATE transcripts SET speaker=? WHERE meeting_id=? AND id=?").bind(to).bind(id).bind(turn).execute(pool).await?;
 Ok(SpeakerRenameOutcome { count:result.rows_affected(), speaker:to.into(), removed_name:false }) }
 }

 impl PeopleRepository { pub async fn link_named_speakers(_: &mut sqlx::Transaction<'_,sqlx::Sqlite>, _: &str) -> Result<(),sqlx::Error> { Ok(()) } }
} #[path="{transcript_path}"] pub mod transcript; } }
'''.replace('{transcript_path}', str(SOURCE / 'database/repositories/transcript.rs')) + f'#[path="{SOURCE}/audio/recording_persistence.rs"] mod recording_persistence;\n')
    subprocess.run(['cargo', 'test', '--manifest-path', str(work / 'Cargo.toml'), 'recording_identity', '--', '--nocapture'], check=True)
    actual = tomllib.loads((work / 'Cargo.lock').read_text())
    original = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
    unexpected = [p for p in actual['package'] if p['name'] != 'meetily-recording-identity-test' and (p['name'], p['version'], p.get('source')) not in original]
    if unexpected:
        raise SystemExit(f'Dependencies outside repository lock: {unexpected}')
