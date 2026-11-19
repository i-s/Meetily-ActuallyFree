import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

let preferences: any = {
  save_folder: '/synthetic/recordings', auto_save: true, file_format: 'mp4',
  preferred_mic_device: 'USB', preferred_system_device: null,
  microphone_enabled: true, system_audio_enabled: false,
  per_app_recording_enabled: true, per_app_targets: [{ id: 'call', name: 'Call app', executable: 'call' }],
};
let saved: any;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  if (name === 'get_recording_preferences') return { ...preferences };
  if (name === 'set_recording_preferences') { saved = args.preferences; preferences = saved; return; }
  throw new Error(`unexpected ${name}`);
} }));
mock.module('@tauri-apps/plugin-store', () => ({ Store: { load: async () => ({ get: async () => true }) } }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ updateRecordingsLocation: () => {}, setSelectedDevices: () => {} }) }));
mock.module('@/lib/analytics', () => ({ default: { track: async () => {} } }));
mock.module('sonner', () => ({ toast: { success: () => {}, error: () => {} } }));
mock.module('@/components/ui/switch', () => ({ Switch: (props: any) => <button {...props} /> }));
const { RecordingSettings } = await import('../../src/components/RecordingSettings');

test('Recording settings no longer offer a separate mode and unrelated edits retain source choices', async () => {
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<RecordingSettings />); });
    expect(JSON.stringify(view!.toJSON())).not.toContain('In-person');
    const switches = view!.root.findAllByType('button').filter(node => typeof node.props.onCheckedChange === 'function');
    await act(async () => { await switches[0].props.onCheckedChange(false); });
    expect(saved.auto_save).toBe(false);
    expect(saved.microphone_enabled).toBe(true);
    expect(saved.system_audio_enabled).toBe(false);
    expect(saved.preferred_mic_device).toBe('USB');
    expect(saved.per_app_recording_enabled).toBe(true);
    expect(saved.per_app_targets[0].executable).toBe('call');
  } finally {
    act(() => view?.unmount());
  }
});
