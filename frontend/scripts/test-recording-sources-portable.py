#!/usr/bin/env python3
"""Device/model-free source tests of production serde, store commands and setup.

Compiles native preferences through the source setter, both commands' unchanged
source/device setup blocks, and manager's unchanged pre-stream source boundary.
Tauri/store transport and devices are synthetic; no capture/DSP/model is loaded.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / 'frontend/src-tauri/src/audio'


def main():
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    names = ('serde', 'serde_json', 'anyhow', 'log', 'once_cell', 'dirs', 'chrono', 'tokio', 'tempfile')
    versions = {n: next(p['version'] for p in lock['package'] if p['name'] == n) for n in names}
    with tempfile.TemporaryDirectory(prefix='meetily23-sources-') as directory:
        work = Path(directory)
        (work / 'src').mkdir()
        dependencies = '\n'.join(f'{n}="={versions[n]}"' for n in names if n not in ('serde', 'tokio'))
        dependencies += f'\nserde={{version="={versions["serde"]}",features=["derive"]}}\ntokio={{version="={versions["tokio"]}",features=["full"]}}\n'
        (work / 'Cargo.toml').write_text('[package]\nname="meetily-sources-portable"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n' + dependencies)
        shutil.copyfile(ROOT / 'Cargo.lock', work / 'Cargo.lock')
        prefs = (AUDIO / 'recording_preferences.rs').read_text()
        prefs = prefs[:prefs.index('#[tauri::command]\npub async fn get_default_recordings_folder_path')].replace('#[tauri::command]', '')
        (work / 'src/preferences.rs').write_text(prefs)
        commands = (AUDIO / 'recording_commands.rs').read_text()
        normal = commands[commands.index('pub async fn start_recording_with_meeting_name'):]
        normal = normal[normal.index('    // Create new recording manager'):normal.index('    // Always ensure a meeting name')]
        custom = commands[commands.index('pub async fn start_recording_with_lifecycle'):]
        custom = custom[custom.index('    // Create new recording manager'):custom.index('    // Async-first approach for custom devices')]
        manager = (AUDIO / 'recording_manager.rs').read_text()
        boundary = manager[manager.index('        let (microphone_device, system_device) = self.sources.devices'):manager.index('        self.recording_saver.set_recording_state')]
        provenance = manager[manager.index('    pub fn prepare_source_provenance'):manager.index('    /// Start recording with specified devices')]
        validation = (AUDIO / 'recording_start_sources.rs').read_text()
        validation = validation[validation.index('/// Capture selection'):]
        meter_source = (AUDIO / 'simple_level_monitor.rs').read_text()
        meter_selection = meter_source[meter_source.index('fn default_monitor_devices('):meter_source.index('fn build_input_level_stream(')]
        meter = (ROOT / 'frontend/tests/native/recording-source-meter.rs').read_text().replace('%METER_SELECTION%', meter_selection)
        fixture = (ROOT / 'frontend/tests/native/recording-source-fixture.rs').read_text()
        for key, value in {'AUDIO': str(AUDIO), 'ROOT': str(ROOT), 'WORK': str(work), 'DEFAULT_SETUP': normal, 'CUSTOM_SETUP': custom, 'MANAGER_BOUNDARY': boundary, 'SOURCE_VALIDATION': validation, 'PREPARE_PROVENANCE': provenance, 'METER_TEST_MODULE': meter, 'MUTE_COMMANDS': commands[commands.index('pub async fn set_microphone_muted'):commands.index('/// Get detailed recording state')].replace('#[tauri::command]', '')}.items():
            fixture = fixture.replace(f'%{key}%', value)
        (work / 'src/lib.rs').write_text('#![allow(dead_code,unused_imports,unused_variables)]\npub use audio::{recording_preferences,recording_start_sources,recording_mode};\n' + fixture)
        subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(work / 'Cargo.toml'), '--', '--test-threads=1'], check=True)
        allowed = {(p['name'], p['version'], p.get('source')) for p in lock['package']}
        actual = tomllib.loads((work / 'Cargo.lock').read_text())
        assert all(p['name'] == 'meetily-sources-portable' or (p['name'], p['version'], p.get('source')) in allowed for p in actual['package'])


if __name__ == '__main__':
    main()
