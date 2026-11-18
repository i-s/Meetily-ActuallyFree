import type { RecordingLease } from '../../src/lib/native-recording';
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
let nativeLease: RecordingLease | null = null;
let generation = 0;
let replaceBeforeReply = false;
let liveSessions = 0;
let automationEnabled = true;
let lastStopLease: RecordingLease | undefined;
events.addEventListener('af-stop-recording', (event) => { stopRequests++; lastStopLease = (event as CustomEvent).detail; });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push({ name, args });
  if (name === 'check_transcription_runtime') await modelGate;
  if (name === 'parakeet_has_available_models' || name === 'parakeet_is_model_loaded') return true;
  if (name === 'get_recording_state') return { is_recording: !!nativeLease, recording_lease: nativeLease };
  if (name === 'start_recording_with_devices_and_meeting') {
    await startGate;
    if (failStart) throw new Error('Synthetic capture unavailable');
    nativeLease = { generation: String(++generation), owner: args.lifecycleSession ? {
      source: 'zoom', session_id: args.lifecycleSession, app: 'Zoom', process: 'us.zoom.xos',
    } : null };
    const reply = nativeLease;
    if (replaceBeforeReply) nativeLease = { generation: 'manual-B', owner: null };
    return reply;
  }
} }));
mock.module('@/lib/labs', () => ({ loadLabsPreferences: () => ({ meetingAutomation: automationEnabled }) }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
const noop = () => {};
const meetings: any[] = [];
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ clearTranscripts: noop, setMeetingTitle: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ setIsMeetingActive: noop, meetings }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: 'synthetic-mic', systemDevice: 'synthetic-system' } }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { IDLE: 'idle', STARTING: 'starting', ERROR: 'error' }, useRecordingState: () => ({ setStatus: noop }) }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null, writePendingGroup: noop }));
mock.module('@/lib/live-session', () => ({ automaticTitle: () => 'Synthetic call', beginLiveSession: () => { liveSessions++; } }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop } }));
mock.module('@/lib/recordingNotification', () => ({ showRecordingNotification: async () => {} }));
mock.module('sonner', () => ({ toast: Object.assign(noop, { error: noop, info: noop }) }));
const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');
const { launchRecording } = await import('../../src/lib/recording-launch');
const { createLifecycleAutomation } = await import('../../src/lib/meeting-lifecycle');
const { automatedRecording, endAutomatedRecording, automatedStartInProgress } = await import('../../src/lib/meeting-automation');
let revision = 0;
let handler: () => Promise<void>;
function Recorder() { handler = useRecordingStart(false, noop).handleRecordingStart; return null; }
const tick = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function detector() {
  return createLifecycleAutomation({ enabled: () => true, state: async () => ({}),
    start: () => { const call = automatedStartInProgress(); if (call) launchRecording(noop, { automated: call }); }, stop: noop, error: e => { throw e; }, conflict: noop,
  });
}
function signal(c: ReturnType<typeof detector>, id: string | null) {
  c.accept({ revision: ++revision, enabled: true, sources: [{ source: 'zoom', app: 'Zoom', process: 'us.zoom.xos', presence: id ? 'active' : 'inactive', reason: 'synthetic', session_id: id, qualified: true }] });
}

test('normal root-to-recorder event and real service carry the native call token; manual button has no owner', async () => {
  store.clear(); calls.length = 0; nativeLease = null;
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
  store.clear(); calls.length = 0; nativeLease = null;
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
  store.clear(); calls.length = 0; nativeLease = null; stopRequests = 0;
  let release!: () => void; startGate = new Promise(r => { release = r; });
  const c = detector(); let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); });
  try {
    await act(async () => { signal(c, 'native-loading'); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(true);
    await act(async () => { signal(c, null); release(); await tick(); });
    expect(stopRequests).toBe(1);
    expect(lastStopLease).toEqual(nativeLease!);
    endAutomatedRecording(); startGate = undefined; failStart = true;
    await act(async () => { signal(c, 'failed-next'); await tick(); });
    expect(automatedStartInProgress()).toBeNull(); expect(automatedRecording()).toBeNull();
  } finally { failStart = false; startGate = undefined; c.dispose(); act(() => view!.unmount()); }
});

test('late automatic A reply cannot adopt or clear manually started B', async () => {
  store.clear(); calls.length = 0; nativeLease = null; replaceBeforeReply = true; liveSessions = 0; stopRequests = 0;
  const c = detector(); let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); });
  try {
    await act(async () => { signal(c, 'late-A'); await tick(); });
    expect(nativeLease as RecordingLease | null).toEqual({ generation: 'manual-B', owner: null });
    expect(liveSessions).toBe(0); expect(stopRequests).toBe(0);
    expect(automatedRecording()).toBeNull(); expect(automatedStartInProgress()).toBeNull();
  } finally { replaceBeforeReply = false; c.dispose(); act(() => view!.unmount()); }
});

test('tokenless legacy automation handoff cannot become a manual capture', async () => {
  store.clear(); calls.length = 0; nativeLease = null;
  store.set('labsAutoStartPending', JSON.stringify({ app: 'Zoom', process: 'zoom.us', at: Date.now() }));
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); });
  try {
    await act(async () => { events.dispatchEvent(new Event('start-recording-from-sidebar')); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
    expect(store.has('labsAutoStartPending')).toBe(true); // #11 owns expiry/recovery, not this rejected event
  } finally { act(() => view!.unmount()); }
});

test('disable while model is awaited cancels the reserved automatic start', async () => {
  store.clear(); calls.length = 0; nativeLease = null; automationEnabled = true;
  let release!: () => void; modelGate = new Promise(r => { release = r; });
  const c = detector(); let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); signal(c, 'disable-loading'); await tick(); });
  try {
    automationEnabled = false;
    await act(async () => { release(); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
    expect(automatedStartInProgress()).toBeNull();
  } finally { automationEnabled = true; modelGate = undefined; c.dispose(); act(() => view!.unmount()); }
});

test('corrupt or missing routed automation is never implicit manual intent, including reload', async () => {
  for (const value of ['{}', '{broken', '{"app":"Zoom"}', '{"app":"Zoom","process":""}', null]) {
    for (const routed of [false, true]) {
      store.clear(); calls.length = 0; nativeLease = null;
      if (value !== null) store.set('labsAutoStartPending', value);
      // Legacy route flag has no explicit manual/automation provenance.
      if (routed) store.set('autoStartRecording', 'true');
      let view: ReturnType<typeof create>;
      await act(async () => { view = create(<Recorder />); await tick(); });
      try {
        await act(async () => {
          if (!routed) events.dispatchEvent(new Event('start-recording-from-sidebar'));
          await tick();
        });
        expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
        expect(calls.some(c => c.name === 'check_transcription_runtime')).toBe(false);
      } finally { act(() => view!.unmount()); }
    }
  }
});

test('explicit automation with missing reservation or invalid event/route payload does nothing', async () => {
  const expected = { app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'missing', token: 'missing-token' };
  for (const payload of [{ kind: 'automated', call: expected }, {}, { kind: 'automated', call: {} }, '{bad']) {
    for (const routed of [false, true]) {
      store.clear(); calls.length = 0; nativeLease = null;
      if (routed) store.set('autoStartRecording', typeof payload === 'string' ? payload : JSON.stringify(payload));
      let view: ReturnType<typeof create>;
      await act(async () => { view = create(<Recorder />); await tick(); });
      try {
        await act(async () => {
          if (!routed) events.dispatchEvent(new CustomEvent('start-recording-from-sidebar', { detail: payload }));
          await tick();
        });
        expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
      } finally { act(() => view!.unmount()); }
    }
  }
});

test('explicit manual button ignores corrupt automation and routed manual controls still work', async () => {
  for (const mode of ['button', 'event', 'route']) {
    store.clear(); calls.length = 0; nativeLease = null;
    store.set('labsAutoStartPending', '{}');
    if (mode === 'route') store.set('autoStartRecording', JSON.stringify({ kind: 'manual' }));
    let view: ReturnType<typeof create>;
    await act(async () => { view = create(<Recorder />); await tick(); });
    try {
      await act(async () => {
        if (mode === 'button') await handler();
        if (mode === 'event') events.dispatchEvent(new CustomEvent('start-recording-from-sidebar', { detail: { kind: 'manual' } }));
        await tick();
      });
      const start = calls.find(c => c.name === 'start_recording_with_devices_and_meeting');
      expect(start).toBeDefined();
      expect(start?.args.lifecycleSession).toBeUndefined();
    } finally { act(() => view!.unmount()); }
  }
});


test('late A launch cannot transfer pending B, and valid typed route survives recorder reload', async () => {
  store.clear(); calls.length = 0; nativeLease = null;
  const c = createLifecycleAutomation({ enabled: () => true, state: async () => ({}), start: noop, stop: noop, error: e => { throw e; }, conflict: noop });
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Recorder />); signal(c, 'pending-B'); await tick(); });
  try {
    const b = automatedStartInProgress()!;
    expect(b.sessionId).toBe('pending-B');
    const a = { ...b, token: 'stale-A', sessionId: 'old-A' };
    await act(async () => { launchRecording(noop, { automated: a }); await tick(); });
    expect(calls.some(c => c.name === 'start_recording_with_devices_and_meeting')).toBe(false);
    expect(automatedStartInProgress()?.token).toBe(b.token);
    act(() => view!.unmount());
    window.location.pathname = '/library';
    launchRecording(noop, { automated: b });
    window.location.pathname = '/';
    await act(async () => { view = create(<Recorder />); await tick(); });
    expect(calls.find(c => c.name === 'start_recording_with_devices_and_meeting')?.args.lifecycleSession).toBe('pending-B');
  } finally { window.location.pathname = '/'; c.dispose(); act(() => view!.unmount()); }
});
