import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const events = new EventTarget();
Object.assign(globalThis, { window: Object.assign(events, { location: { pathname: '/' }, clearTimeout, setTimeout }), sessionStorage: { getItem: () => null, setItem: () => {}, removeItem: () => {} } });
let prefs = { microphone_enabled: true, system_audio_enabled: false };
const calls: string[] = [];
let starts = 0;
let phase = 'idle';
let sourceReadFails = false;
const noop = () => {};
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push(name);
  if (name === 'get_recording_preferences') { if (sourceReadFails) throw new Error('Synthetic temporary read'); return { ...prefs }; }
  if (name === 'get_audio_devices') return [];
  if (name === 'set_recording_source_enabled') {
    prefs = { ...prefs, [args.source === 'microphone' ? 'microphone_enabled' : 'system_audio_enabled']: args.enabled };
    return { ...prefs };
  }
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { STARTING: 'starting' }, useRecordingState: () => ({ status: phase, activeDuration: 3, microphoneCaptured: true, systemAudioCaptured: false, isMicrophoneMuted: false, isSystemAudioMuted: true }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null }, setSelectedDevices: noop }) }));
mock.module('@/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
mock.module('@/hooks/usePendingGroup', () => ({ usePendingGroup: () => [null, noop] }));
mock.module('@/hooks/useAppAudio', () => ({ useAppAudio: () => ({ loaded: true, onlyApps: false, targets: [] }) }));
mock.module('@/components/recording/AppAudioSource', () => ({ useAppAudioSupported: () => true, AppAudioSource: () => null }));
mock.module('@/components/RecordingVoiceLane', () => ({ RecordingVoiceLane: (props: any) => <button data-lane={props.kind} data-muted={props.muted} disabled={props.disabled} onClick={props.onMute} /> }));
mock.module('@/components/groups/GroupBits', () => ({ GroupPicker: () => null }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop } }));
mock.module('sonner', () => ({ toast: { error: noop } }));
const { RecordingControls } = await import('../../src/components/RecordingControls');
const flush = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const props = { isRecording: false, isRecordingDisabled: false, onRecordingStop: noop, onRecordingStart: () => { starts++; } };

test('existing output icon shows migrated off state and persists a deliberate on choice without a new mode', async () => {
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); await flush(); });
  try {
    const output = () => view.root.findByProps({ 'data-lane': 'output' });
    expect(output().props['data-muted']).toBe(true);
    expect(JSON.stringify(view.toJSON())).not.toContain('In-person');
    await act(async () => { output().props.onClick(); await flush(); });
    expect(output().props['data-muted']).toBe(false);
    expect(prefs.system_audio_enabled).toBe(true);
    expect(calls).not.toContain('set_system_audio_muted');
    act(() => view.unmount());
    await act(async () => { view = create(<RecordingControls {...props} />); await flush(); });
    expect(output().props['data-muted']).toBe(false);
    await act(async () => { view.root.findByProps({ 'aria-label': 'Start recording' }).props.onClick(); await flush(); });
    expect(starts).toBe(1);
  } finally { act(() => view.unmount()); }
});

test('restored live take shows excluded system source off and does not apply late post-start mutes', async () => {
  calls.length = 0;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} isRecording />); await flush(); });
  try {
    const output = view.root.findByProps({ 'data-lane': 'output' });
    expect(output.props['data-muted']).toBe(true);
    expect(calls).not.toContain('set_system_audio_muted');
    const lane = view.root.findAll(node => node.props.kind === 'output' && node.props.live === true)[0];
    expect(lane.props.captureIncluded).toBe(false);
  } finally { act(() => view.unmount()); }
});


test('routed model preparation locks the same source icons before native capture begins', async () => {
  phase = 'starting';
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); await flush(); });
  try {
    expect(view.root.findByProps({ 'data-lane': 'mic' }).props.disabled).toBe(true);
    expect(view.root.findByProps({ 'data-lane': 'output' }).props.disabled).toBe(true);
  } finally { phase = 'idle'; act(() => view.unmount()); }
});


test('temporary source read error offers a contextual retry without enabling Start first', async () => {
  sourceReadFails = true;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); await flush(); });
  try {
    expect(view.root.findByProps({ 'aria-label': 'Start recording' }).props.disabled).toBe(true);
    const retry = view.root.findByProps({ 'aria-label': 'Retry loading recording sources' });
    sourceReadFails = false;
    await act(async () => { retry.props.onClick(); await flush(); });
    expect(view.root.findByProps({ 'aria-label': 'Start recording' }).props.disabled).toBe(false);
    expect(view.root.findAllByProps({ 'aria-label': 'Retry loading recording sources' })).toHaveLength(0);
  } finally { sourceReadFails = false; act(() => view.unmount()); }
});
