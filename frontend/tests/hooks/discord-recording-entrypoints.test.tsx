import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const data = new Map<string, string>();
const target = new EventTarget();
Object.assign(globalThis, {
  IS_REACT_ACT_ENVIRONMENT: true,
  sessionStorage: { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => data.set(k, v), removeItem: (k: string) => data.delete(k) },
  window: Object.assign(target, { location: { pathname: '/' } }),
  CustomEvent: class extends Event { detail: unknown; constructor(name: string, options?: CustomEventInit) { super(name); this.detail = options?.detail; } },
});
let status: any = { enabled: true, qualified: true, state: 'active', session_id: 'fixture:1', transition: 'start', reason: 'synthetic_not_qualified' };
let starts = 0;
let capabilityAvailable = true;
let afterModelReady = () => {};
const noop = () => {};
const messages: string[] = [];
const toast = Object.assign((message: string) => messages.push(message), { error: noop, info: noop });
mock.module('sonner', () => ({ toast }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string) => {
  if (name === 'get_discord_automation_status') return status;
  if (name === 'check_transcription_runtime' || name === 'parakeet_init') return;
  if (name === 'parakeet_has_available_models') return true;
  if (name === 'parakeet_is_model_loaded') { afterModelReady(); return true; }
  throw new Error(`unexpected native command: ${name}`);
} }));
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ clearTranscripts: noop, setMeetingTitle: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ setIsMeetingActive: noop, meetings: [] }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: 'synthetic-mic', systemDevice: 'synthetic-output' }, transcriptModelConfig: { provider: 'parakeet' } }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ setStatus: noop }), RecordingStatus: { IDLE: 'idle', STARTING: 'starting', ERROR: 'error' } }));
mock.module('@/services/recordingService', () => ({ recordingService: { getRecordingStartCapability: async () => { if (!capabilityAvailable) throw new Error('Native recording start authority v1 is unavailable'); return { version: 1, epoch: 'fixture-native', next_sequence: '1' }; }, startRecordingWithDevices: async (mic: string, output: string, _title: string, session?: string, expected?: any) => { expect(mic).toBe('synthetic-mic'); expect(output).toBe('synthetic-output'); starts++; if (session) { expect(expected.session_id).toBe(session); expect(expected.token).toBeTruthy(); return { generation: 'fixture:lease', owner: { source: 'discord', session_id: session, app: 'Discord', process: `discord-macos:${session}` } }; } } } }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null, writePendingGroup: noop }));
mock.module('@/lib/live-session', () => ({ automaticTitle: () => 'Synthetic Discord entrypoint test', beginLiveSession: noop }));
mock.module('@/lib/labs', () => ({ loadLabsPreferences: () => ({ meetingAutomation: true }) }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop } }));
mock.module('@/lib/recordingNotification', () => ({ showRecordingNotification: async () => {} }));

const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');
const { createDiscordAutomation } = await import('../../src/lib/discord-automation');
const { automatedRecording, endAutomatedRecording, markAutomatedStart, automatedStartInProgress } = await import('../../src/lib/meeting-automation');
const { launchRecording, requestRecordingStop, STOP_RECORDING_EVENT } = await import('../../src/lib/recording-launch');
let stopRequests = 0;
window.addEventListener(STOP_RECORDING_EVENT, () => { stopRequests++; endAutomatedRecording(); });
let manualStart: () => Promise<void>;
function Recorder() { manualStart = useRecordingStart(false, noop).handleRecordingStart; return null; }

function bridge() {
  return createDiscordAutomation({ allowed: () => true, status: async () => status, recording: async () => ({ is_recording: !!automatedRecording() }),
    start: (call) => launchRecording(() => { throw new Error('already on recorder'); }, { automated: call }), stop: () => requestRecordingStop(() => { throw new Error('already on recorder'); }), error: (e) => { throw e; } });
}
const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

test('Discord event bridge drives ordinary launcher and real useRecordingStart; unqualified evidence cannot capture', async () => {
  let view: ReturnType<typeof create> | undefined;
  let controller = bridge();
  try {
    await act(async () => { view = create(<Recorder />); });
    status = { ...status, qualified: false };
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(0);
    status = { ...status, qualified: true };
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(1);
    expect(automatedRecording()?.process).toBe('discord-macos:fixture:1');
    await act(async () => { status = { ...status, state: 'inactive', transition: 'end' }; controller.update(status); await settle(); });
    expect(stopRequests).toBe(0); // No native lease on the assigned base: auto-Stop is blocked.
    expect(automatedRecording()?.source).toBe('discord');
    await act(async () => requestRecordingStop(() => {})); // Explicit manual Stop remains ordinary.
    expect(stopRequests).toBe(1);
    expect(automatedRecording()).toBeNull();
    endAutomatedRecording(); // Same ownership revocation used by manual Stop / Don't stop it.
    status = { ...status, state: 'active', transition: 'none' };
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(1);
    controller.dispose();

    // Slow model loading invalidates the original join before the real service start.
    data.clear(); status = { ...status, session_id: 'fixture:2', qualified: true };
    afterModelReady = () => { status = { ...status, qualified: false }; };
    controller = bridge();
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(1);
    expect(messages).toContain('Discord automatic start cancelled');
    expect(automatedRecording()).toBeNull();

    // Revocation while STT loads must not clear/override a replacement Zoom request.
    data.clear(); status = { ...status, session_id: 'fixture:3', qualified: true };
    afterModelReady = () => { endAutomatedRecording(); markAutomatedStart({ app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'z2' }); };
    controller.dispose(); controller = bridge();
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(1);
    expect(automatedStartInProgress()?.source).toBe('zoom');
    expect(automatedRecording()).toBeNull();
    // Manual Start remains available even though Discord evidence is unqualified.
    afterModelReady = noop;
    await act(async () => { await manualStart(); });
    expect(starts).toBe(2);
    expect(automatedRecording()).toBeNull();
  } finally { controller.dispose(); await act(async () => view?.unmount()); }
}, 120_000);


test('missing native v1 support fails closed before automatic capture, without disabling explicit manual Start', async () => {
  data.clear(); afterModelReady = noop; capabilityAvailable = false;
  status = { ...status, session_id: 'no-native-capability', qualified: true, state: 'active', transition: 'start' };
  const initial = starts; const controller = bridge();
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<Recorder />); });
    await act(async () => { controller.update(status); await settle(); });
    expect(starts).toBe(initial); expect(automatedRecording()).toBeNull();
    await act(async () => { await manualStart(); });
    expect(starts).toBe(initial + 1); expect(automatedRecording()).toBeNull();
  } finally { capabilityAvailable = true; controller.dispose(); await act(async () => view?.unmount()); }
}, 120_000);
