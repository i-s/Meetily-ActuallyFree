import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const store = new Map<string, string>();
const events = new EventTarget();
Object.assign(globalThis, { sessionStorage: {
  getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k),
}, window: Object.assign(events, { location: { pathname: '/' } }) });
const calls: { name: string; args: any }[] = [];
let modelGate: Promise<void> | undefined;
let startGate: Promise<void> | undefined;
let failStart = false;
let stopRequests = 0;
events.addEventListener('af-stop-recording', () => { stopRequests++; });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push({ name, args });
  if (name === 'check_transcription_runtime') await modelGate;
  if (name === 'parakeet_has_available_models' || name === 'parakeet_is_model_loaded') return true;
  if (name === 'start_recording_with_devices_and_meeting') {
    await startGate;
    if (failStart) throw new Error('Synthetic capture unavailable');
  }
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
const noop = () => {};
const meetings: any[] = [];
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ clearTranscripts: noop, setMeetingTitle: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ setIsMeetingActive: noop, meetings }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: 'synthetic-mic', systemDevice: 'synthetic-system' } }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { IDLE: 'idle', STARTING: 'starting', ERROR: 'error' }, useRecordingState: () => ({ setStatus: noop }) }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null, writePendingGroup: noop }));
mock.module('@/lib/live-session', () => ({ automaticTitle: () => 'Synthetic call', beginLiveSession: noop }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop } }));
mock.module('@/lib/recordingNotification', () => ({ showRecordingNotification: async () => {} }));
mock.module('sonner', () => ({ toast: Object.assign(noop, { error: noop, info: noop }) }));
const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');
const { createLifecycleAutomation } = await import('../../src/lib/meeting-lifecycle');
const { automatedRecording, endAutomatedRecording, automatedStartInProgress } = await import('../../src/lib/meeting-automation');
let revision = 0;
let handler: () => Promise<void>;
function Recorder() { handler = useRecordingStart(false, noop).handleRecordingStart; return null; }
const tick = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function detector() {
  return createLifecycleAutomation({ enabled: () => true, state: async () => ({}),
    start: () => { events.dispatchEvent(new Event('start-recording-from-sidebar')); }, stop: noop, error: e => { throw e; }, conflict: noop,
  });
}
function signal(c: ReturnType<typeof detector>, id: string | null) {
  c.accept({ revision: ++revision, enabled: true, sources: [{ source: 'zoom', app: 'Zoom', process: 'us.zoom.xos', presence: id ? 'active' : 'inactive', reason: 'synthetic', session_id: id }] });
}

test('normal root-to-recorder event and real service carry the native call token; manual button has no owner', async () => {
  store.clear(); calls.length = 0;
  let view: ReturnType<typeof create>;
  const c = detector();
  await act(async () => { view = create(<Recorder />); });
  try {
    await act(async () => { signal(c, 'normal'); await tick(); });
    const command = calls.find(c => c.name === 'start_recording_with_devices_and_meeting');
    expect(command?.args).toEqual({ micDeviceName: 'synthetic-mic', systemDeviceName: 'synthetic-system', meetingName: 'Synthetic call', lifecycleSession: 'normal' });
    expect(automatedRecording()?.sessionId).toBe('normal');
    expect(automatedStartInProgress()).toBeNull();
    endAutomatedRecording(); calls.length = 0;
    await act(async () => { await handler(); });
    expect(calls.find(c => c.name === 'start_recording_with_devices_and_meeting')?.args.lifecycleSession).toBeUndefined();
    expect(automatedRecording()).toBeNull();
  } finally { c.dispose(); act(() => view!.unmount()); }
});

test('confirmed exit while model loads cancels capture through the normal recorder hook', async () => {
  store.clear(); calls.length = 0;
  let release!: () => void; modelGate = new Promise(r => { release = r; });
  const c = detector(); let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); signal(c, 'loading'); await tick(); });
  try {
    await act(async () => { signal(c, null); release(); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
    expect(automatedRecording()).toBeNull(); expect(automatedStartInProgress()).toBeNull();
  } finally { modelGate = undefined; c.dispose(); act(() => view!.unmount()); }
});

test('leave racing native stream start requests normal save/Stop and errors release the startup reservation', async () => {
  store.clear(); calls.length = 0; stopRequests = 0;
  let release!: () => void; startGate = new Promise(r => { release = r; });
  const c = detector(); let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); });
  try {
    await act(async () => { signal(c, 'native-loading'); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(true);
    await act(async () => { signal(c, null); release(); await tick(); });
    expect(stopRequests).toBe(1);
    endAutomatedRecording(); startGate = undefined; failStart = true;
    await act(async () => { signal(c, 'failed-next'); await tick(); });
    expect(automatedStartInProgress()).toBeNull(); expect(automatedRecording()).toBeNull();
  } finally { failStart = false; startGate = undefined; c.dispose(); act(() => view!.unmount()); }
});
