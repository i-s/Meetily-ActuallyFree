import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
import type { RecordingLease } from '../../src/lib/native-recording';

const events = new EventTarget();
const storage = new Map<string, string>();
Object.assign(globalThis, { window: Object.assign(events, { location: { pathname: '/home' }, clearTimeout, setTimeout }),
  sessionStorage: { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => storage.set(k, v), removeItem: (k: string) => storage.delete(k) } });
let enabled = true;
let failStopOnce = false;
let current: RecordingLease | null = null;
let dataGate: Promise<void> | undefined;
const calls: { name: string; args: any }[] = [];
let successes = 0;
let stopInitiated = 0;
let fallbackStops = 0;
const noop = () => {};
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push({ name, args });
  if (name === 'get_recording_state') return { is_recording: !!current, is_stopping: false, recording_lease: current };
  if (name === 'get_audio_devices') return [];
  if (name === 'get_recording_preferences') return {};
  if (name === 'get_database_directory') { await dataGate; return '/synthetic'; }
  if (name === 'stop_recording') {
    if (failStopOnce) { failStopOnce = false; throw new Error('Synthetic Stop transport failure'); }
    if (args.args.manual_generation && args.args.manual_generation !== current?.generation) return false;
    if (args.args.expected && (!enabled || JSON.stringify(args.args.expected) !== JSON.stringify(current))) return false;
    current = null; return true;
  }
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@/lib/labs', () => ({ loadLabsPreferences: () => ({ meetingAutomation: enabled }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ activeDuration: 3, inPerson: false }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null }, setSelectedDevices: noop }) }));
mock.module('@/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
mock.module('@/hooks/usePendingGroup', () => ({ usePendingGroup: () => [null, noop] }));
mock.module('@/hooks/useAppAudio', () => ({ useAppAudio: () => ({ loaded: true, onlyApps: false, targets: [], scan: async () => {} }) }));
mock.module('@/components/recording/AppAudioSource', () => ({ useAppAudioSupported: () => true, AppAudioSource: () => null }));
mock.module('@/components/RecordingVoiceLane', () => ({ RecordingVoiceLane: () => null }));
mock.module('@/components/groups/GroupBits', () => ({ GroupPicker: () => null }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
mock.module('@/lib/analytics', () => ({ default: { trackTranscriptionSuccess: () => { successes++; }, trackButtonClick: noop, trackTranscriptionError: noop } }));
mock.module('sonner', () => ({ toast: { error: noop } }));
const { RecordingControls } = await import('../../src/components/RecordingControls');
const { requestRecordingStop } = await import('../../src/lib/recording-launch');
const a: RecordingLease = { generation: 'native-A', owner: { source: 'zoom', session_id: 'A', app: 'Zoom', process: 'us.zoom.xos' } };
const b: RecordingLease = { generation: 'native-B', owner: null };
const props = { isRecording: true, isRecordingDisabled: false, onRecordingStart: noop,
  onRecordingStop: () => { fallbackStops++; }, onStopInitiated: () => { stopInitiated++; } };
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function reset() { failStopOnce = false; storage.clear(); calls.length = 0; enabled = true; successes = 0; stopInitiated = 0; fallbackStops = 0; dataGate = undefined; }

test('actual routed Controls preserve A lease and rejected stale stop has no UI/save effects on manual B', async () => {
  reset(); window.location.pathname = '/home'; current = a;
  let navigated = '';
  requestRecordingStop(href => { navigated = href; }, a);
  expect(navigated).toBe('/');
  current = b; // A manually stopped, manual B started before recorder route mounted.
  window.location.pathname = '/';
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); await flush(); });
  try {
    expect(calls.find(c => c.name === 'stop_recording')?.args.args.expected).toEqual(a);
    expect(current).toEqual(b); expect(successes).toBe(0); expect(stopInitiated).toBe(0); expect(fallbackStops).toBe(0);
    const button = view!.root.findByProps({ 'aria-label': 'Stop recording' });
    expect(button.props['data-loading']).toBeUndefined();
    await act(async () => { button.props.onClick(); await flush(); });
    expect(current).toBeNull(); expect(successes).toBe(1); expect(stopInitiated).toBe(1);
    expect(calls.filter(c => c.name === 'stop_recording').at(-1)?.args.args.expected).toBeUndefined();
  } finally { act(() => view!.unmount()); }
});

test('actual Controls recheck disable after awaited directory and never submit unguarded or missing-lease automated requests', async () => {
  reset(); current = a; window.location.pathname = '/';
  let release!: () => void; dataGate = new Promise(r => { release = r; });
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); });
  try {
    await act(async () => { requestRecordingStop(noop, a); await flush(); });
    enabled = false;
    await act(async () => { release(); await flush(); });
    expect(calls.some(c => c.name === 'stop_recording')).toBe(false);
    expect(current).toEqual(a); expect(successes).toBe(0); expect(stopInitiated).toBe(0); expect(fallbackStops).toBe(0);
    await act(async () => { events.dispatchEvent(new CustomEvent('af-stop-recording', { detail: {} })); await flush(); });
    expect(calls.some(c => c.name === 'stop_recording')).toBe(false);
  } finally { dataGate = undefined; act(() => view!.unmount()); }
});


test('explicit manual routed Stop can be retried after a transport failure without remount', async () => {
  reset(); current = a; window.location.pathname = '/'; failStopOnce = true;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} />); });
  try {
    await act(async () => { requestRecordingStop(noop); await flush(); });
    expect(current).toEqual(a); expect(successes).toBe(0); expect(fallbackStops).toBe(0);
    await act(async () => { requestRecordingStop(noop); await flush(); });
    expect(current).toBeNull(); expect(successes).toBe(1); expect(fallbackStops).toBe(0);
  } finally { act(() => view.unmount()); }
});


test('R2 explicit manual B is not dropped behind stale guarded A before initial sync', async () => {
  reset(); current = b; window.location.pathname = '/library';
  requestRecordingStop(noop, a); requestRecordingStop(noop); window.location.pathname = '/';
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingControls {...props} isRecording={false} />); await flush(); });
  try {
    await act(async () => { view.update(<RecordingControls {...props} />); await flush(); });
    expect(current).toBeNull(); expect(successes).toBe(1); expect(stopInitiated).toBe(1);
  } finally { act(() => view.unmount()); }
});
