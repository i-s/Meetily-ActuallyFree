#!/usr/bin/env python3
"""Device-free SQL qualification of production library reads and command bodies.

Uses real SQLite, checked-in migrations, production search/summary helpers, and
normal repository calls. Only Tauri State transport is adapted. Optional #14
source is read with git show into the temporary fixture, never merged.
"""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
REPO = ROOT / 'frontend/src-tauri/src/database/repositories'


def main():
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n)
                for n in ('sqlx', 'tokio', 'serde', 'serde_json', 'chrono', 'tempfile', 'uuid', 'pulldown-cmark')}
    with tempfile.TemporaryDirectory(prefix='meetily-library-sql-') as folder:
        work = Path(folder)
        (work / 'src').mkdir()
        version_ref = os.environ.get('MEETILY_LIBRARY_VERSIONS_REF')
        extra_modules = ''
        identity_module = ''
        extra_migrations = []
        if version_ref:
            def source(path):
                return subprocess.check_output(['git', 'show', f'{version_ref}:{path}'], cwd=ROOT, text=True)
            for name, path in [('transcript_versions', 'frontend/src-tauri/src/database/transcript_versions.rs'), ('speaker_identity', 'frontend/src-tauri/src/database/repositories/speaker_identity.rs')]:
                text = source(path).replace('../../migrations/', f'{work}/src/')
                (work / 'src' / f'{name}.rs').write_text(text)
            for name in ['20261118001400_transcript_versions.sql', '20261118001401_transcript_version_references.sql']:
                path = work / 'src' / name
                path.write_text(source('frontend/src-tauri/migrations/' + name))
                extra_migrations.append(path)
            identity_module = f'#[path="{work}/src/speaker_identity.rs"] pub mod speaker_identity;'
            extra_modules = f'''mod api {{ #[derive(Debug,serde::Serialize,serde::Deserialize)] pub struct TranscriptSegment {{ pub id:String,pub text:String,pub timestamp:String,pub audio_start_time:Option<f64>,pub audio_end_time:Option<f64>,pub duration:Option<f64>,pub speaker:Option<String> }} }}
#[path="{work}/src/transcript_versions.rs"] mod transcript_versions;
'''
            print(f'Read-only selected-version compatibility source: {version_ref}', flush=True)
        person = (REPO / 'person.rs').read_text()
        visible = person[person.index('pub(crate) fn visible_summary_text'):person.index('fn escape_like')]
        snippet = person[person.index('pub(crate) fn snippet_around'):person.index('pub(crate) fn truncate_chars')]
        search = person[person.index('    pub async fn global_search('):person.index('    pub async fn get_profile(')]
        dto = '#[derive(Debug,serde::Serialize)]\n#[serde(rename_all="camelCase")]\n' + person[person.index('pub struct GlobalSearchResult {'):person.index('\n#[derive',person.index('pub struct GlobalSearchResult {'))]
        ranking = person[person.index('struct RankedResult {'):person.index('pub(crate) struct SpeakerRenameOutcome')]
        command_start = person.index('pub async fn api_global_search(')
        global_command = person[command_start:person.index('#[tauri::command]', command_start)].replace("tauri::State<'_, AppState>", '&AppState')
        helpers = person[person.index('fn escape_like'):person.index('pub(crate) fn snippet_around')]
        normalize = person[person.index('pub(crate) fn normalize_person_name'):person.index('pub(crate) fn is_person_name')]
        (work / 'src/person.rs').write_text('use sqlx::SqlitePool; use std::cmp::Ordering; use crate::state::AppState; const DEFAULT_SEARCH_LIMIT:i64=40; const MAX_SEARCH_LIMIT:i64=100;\n' + dto + ranking + 'impl PeopleRepository {\n' + search + '}\n' + global_command + normalize + helpers + visible + '\n' + snippet)
        commands = (ROOT / 'frontend/src-tauri/src/database/library_commands.rs').read_text().replace('#[tauri::command]\n', '')
        commands = commands.replace("tauri::State<'_, AppState>", '&AppState').replace('use super::repositories::library;', 'use crate::database::repositories::library;')
        (work / 'src/commands.rs').write_text(commands)
        migrations = sorted((ROOT / 'frontend/src-tauri/migrations').glob('*.sql')) + extra_migrations
        (work / 'src/migrations.rs').write_text('pub const MIGRATIONS: &[&str] = &[\n' + ''.join(f'include_str!("{p}"),\n' for p in migrations) + '];')
        deps = '\n'.join(f'{n}="={versions[n]}"' for n in ('serde_json', 'chrono', 'tempfile'))
        (work / 'Cargo.toml').write_text(f'''[package]
name="meetily-library-sql"
version="0.0.0"
edition="2021"
[features]
versions=[]
[dependencies]
{deps}
pulldown-cmark={{version="={versions['pulldown-cmark']}",default-features=false}}
uuid={{version="={versions['uuid']}",features=["v4"]}}
serde={{version="={versions['serde']}",features=["derive"]}}
sqlx={{version="={versions['sqlx']}",features=["runtime-tokio", "sqlite", "chrono"]}}
tokio={{version="={versions['tokio']}",features=["macros", "rt-multi-thread", "sync", "time"]}}
''')
        (work / 'src/lib.rs').write_text(f'''
{extra_modules}
mod state {{ pub struct Db(pub sqlx::SqlitePool); impl Db {{ pub fn pool(&self)->&sqlx::SqlitePool{{&self.0}} }} pub struct AppState{{pub db_manager:Db}} }}
mod database {{ pub mod repositories {{
#[path="{work}/src/person.rs"] pub mod person;
#[path="{REPO}/library.rs"] pub mod library;
{identity_module}
}} }}
#[path="{work}/src/commands.rs"] mod commands;
#[path="{work}/src/migrations.rs"] mod migrations;
#[cfg(test)] #[path="{ROOT}/frontend/tests/native/library.rs"] mod tests;
''')
        # Reuse the workspace lock versions; Cargo removes unused entries and
        # adds only this fixture's root. The second invocation is locked.
        (work / 'Cargo.lock').write_text((ROOT / 'Cargo.lock').read_text())
        env = dict(os.environ, CARGO_BUILD_JOBS='1')
        subprocess.run(['cargo', 'metadata', '--offline', '--filter-platform', 'x86_64-unknown-linux-gnu', '--format-version', '1', '--manifest-path', str(work / 'Cargo.toml')], check=True, env=env, stdout=subprocess.DEVNULL)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-library-sql' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package']), 'unlocked dependency version'
        subprocess.run(['cargo', 'test', '--offline', '--locked', '--manifest-path', str(work / 'Cargo.toml'), *(['--features', 'versions'] if version_ref else []), *sys.argv[1:], '--', '--test-threads=1'], check=True, env=env)


if __name__ == '__main__':
    main()
