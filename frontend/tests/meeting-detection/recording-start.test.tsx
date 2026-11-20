// Run separately: this file mocks the recorder's native and context dependencies.
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, beforeEach, expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
const events = new EventTarget();
Object.assign(events, { location: { pathname: '/' } });
Object.assign(globalThis, {
  sessionStorage: { getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) },
  window: events,
});
const calls: string[] = [];
const statuses: string[] = [];
let confirmed = false;
let sessionExists = true;
let automationEnabled = true;
let stops = 0;
let finishModel!: () => void;
let modelLoading!: () => void;
let loading: Promise<void>;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push(command);
  if (command === 'parakeet_has_available_models') return true;
  if (command === 'parakeet_is_model_loaded') return false;
  if (command === 'parakeet_get_available_models') return [{ name: 'model', status: 'Available' }];
  if (command === 'parakeet_load_model') {
    modelLoading();
    await new Promise<void>(resolve => { finishModel = resolve; });
  }
  if (command === 'validate_meeting_detection_session') {
    expect(args).toEqual({ process: 'zoom', sessionId: 42 });
    return confirmed;
  }
  if (command === 'meeting_detection_session_exists') return sessionExists;
  return true;
} }));
const noop = () => {};
const router = { push: noop };
mock.module('next/navigation', () => ({ useRouter: () => router }));
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ clearTranscripts: noop, setMeetingTitle: noop }) }));
const meetings: any[] = [];
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ setIsMeetingActive: noop, meetings }) }));
const config = { selectedDevices: null, transcriptModelConfig: { provider: 'parakeet' } };
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => config }));
const setStatus = (status: string) => { statuses.push(status); };
mock.module('@/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ setStatus }), RecordingStatus: { IDLE: 'idle', STARTING: 'starting', ERROR: 'error' } }));
mock.module('@/services/recordingService', () => ({ recordingService: { startRecordingWithDevices: async () => { calls.push('capture'); } } }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null, writePendingGroup: noop }));
mock.module('@/lib/live-session', () => ({ automaticTitle: () => 'Meeting', beginLiveSession: noop }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop } }));
mock.module('@/lib/recordingNotification', () => ({ showRecordingNotification: async () => {} }));
mock.module('@/lib/labs', () => ({ loadLabsPreferences: () => ({ meetingAutomation: automationEnabled }) }));
const toast = Object.assign(noop, { error: noop, info: noop });
mock.module('sonner', () => ({ toast }));
const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');
const { automatedRecording, markAutomatedStart } = await import('../../src/lib/meeting-automation');
const { START_RECORDING_EVENT } = await import('../../src/lib/recording-launch');
events.addEventListener('af-stop-recording', () => { stops++; });
const zoom = { app: 'Zoom', process: 'zoom', session_id: 42 };
let view: ReactTestRenderer;
function Harness() {
  const { handleRecordingStart } = useRecordingStart(false, noop);
  return <button onClick={handleRecordingStart}>Record</button>;
}
beforeEach(() => {
  storage.clear(); calls.length = 0; statuses.length = 0; confirmed = false;
  sessionExists = true; automationEnabled = true; stops = 0;
  loading = new Promise<void>(resolve => { modelLoading = resolve; });
});
afterEach(async () => { await act(async () => view?.unmount()); });

test('a call ending during model loading does not start capture or claim ownership', async () => {
  markAutomatedStart(zoom);
  await act(async () => { view = create(<Harness />); });
  await act(async () => { events.dispatchEvent(new Event(START_RECORDING_EVENT)); await loading; });
  expect(calls).not.toContain('validate_meeting_detection_session');
  await act(async () => { finishModel(); });
  expect(calls).toContain('validate_meeting_detection_session');
  expect(calls).not.toContain('capture');
  expect(automatedRecording()).toBeNull();
  expect(statuses.at(-1)).toBe('idle');
});

test('a still-confirmed call starts capture and keeps its session ownership', async () => {
  confirmed = true;
  markAutomatedStart(zoom);
  await act(async () => { view = create(<Harness />); });
  await act(async () => { events.dispatchEvent(new Event(START_RECORDING_EVENT)); await loading; });
  await act(async () => { finishModel(); });
  expect(calls).toContain('validate_meeting_detection_session');
  expect(calls).toContain('capture');
  expect(calls.indexOf('validate_meeting_detection_session')).toBeLessThan(calls.indexOf('capture'));
  expect(automatedRecording()).toEqual(zoom);
  expect(calls).toContain('meeting_detection_session_exists');
  expect(stops).toBe(0);
});

test('manual Record ignores an automation hand-off and does not depend on call evidence', async () => {
  markAutomatedStart(zoom);
  await act(async () => { view = create(<Harness />); });
  let recording!: Promise<void>;
  await act(async () => { recording = view.root.findByType('button').props.onClick(); await loading; });
  await act(async () => { finishModel(); await recording; });
  expect(calls).toContain('capture');
  expect(calls).not.toContain('validate_meeting_detection_session');
  expect(automatedRecording()).toBeNull();
});

test('an end during native start is reconciled after ownership', async () => {
  confirmed = true; sessionExists = false;
  markAutomatedStart(zoom);
  await act(async () => { view = create(<Harness />); });
  await act(async () => { events.dispatchEvent(new Event(START_RECORDING_EVENT)); await loading; });
  await act(async () => { finishModel(); });
  expect(calls).toContain('capture');
  expect(calls).toContain('meeting_detection_session_exists');
  expect(stops).toBe(1);
});

test('disabling automation during model preparation cancels the pending start', async () => {
  confirmed = true;
  markAutomatedStart(zoom);
  await act(async () => { view = create(<Harness />); });
  await act(async () => { events.dispatchEvent(new Event(START_RECORDING_EVENT)); await loading; });
  automationEnabled = false;
  await act(async () => { finishModel(); });
  expect(calls).not.toContain('capture');
  expect(statuses.at(-1)).toBe('idle');
});

test('expired automated requests are cancelled instead of becoming manual recordings', async () => {
  storage.set('labsAutoStartPending', JSON.stringify({ ...zoom, at: Date.now() - 120_000 }));
  await act(async () => { view = create(<Harness />); });
  await act(async () => { events.dispatchEvent(new Event(START_RECORDING_EVENT)); });
  expect(calls).not.toContain('parakeet_load_model');
  expect(calls).not.toContain('capture');
  expect(statuses.at(-1)).toBe('idle');
});
