import React from 'react';
import { act, create } from 'react-test-renderer';
import { afterAll, beforeAll, beforeEach, describe, expect, mock, test } from 'bun:test';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createInterface } from 'node:readline';
import { readdirSync } from 'node:fs';
import type { AutomatedCall } from '../../src/lib/meeting-automation';
import type { RecordingLease } from '../../src/lib/recording-start-authority';

const binary = process.env.MEETILY_RECORDING_RPC;
const data = process.env.MEETILY_RECORDING_DATA ?? '/unused-synthetic-data';
let child: ChildProcessWithoutNullStreams;
let sequence = 0;
const waiting = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
const nativePending = new Set<Promise<any>>();
function rpc(command: string, args: any = {}): Promise<any> {
  const promise = new Promise((resolve, reject) => {
    const id = ++sequence;
    waiting.set(id, { resolve, reject });
    child.stdin.write(JSON.stringify({ id, command, args }) + '\n');
  });
  nativePending.add(promise);
  void promise.finally(() => nativePending.delete(promise)).catch(() => {});
  return promise;
}
const events = new EventTarget();
const store = new Map<string, string>();
Object.assign(globalThis, { sessionStorage: {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => store.set(key, value), removeItem: (key: string) => store.delete(key),
}, window: Object.assign(events, { location: { pathname: '/' }, setTimeout, clearTimeout }) });
const noop = () => {};
let enabled = true, loseReply = false;
let replyGate: Promise<void> | undefined;
let committedSignal: (() => void) | undefined;
let stopGate: Promise<void> | undefined;
let modelGate: Promise<void> | undefined;
let lives = 0, stopped = 0, successes = 0, stopInitiated = 0;
let notices: { text: string; options: any }[] = [];
const calls: { name: string; args: any }[] = [];
const pending = new Set<Promise<any>>();
async function invokeNative(name: string, args: any = {}) {
  calls.push({ name, args });
  if (name === 'check_transcription_runtime') { await modelGate; return; }
  if (name === 'parakeet_has_available_models' || name === 'parakeet_is_model_loaded') return true;
  if (name === 'parakeet_init') return;
  if (name === 'get_database_directory') return data;
  if (name === 'get_audio_devices') return [];
  if (name === 'get_recording_preferences') return {};
  if (name === 'stop_recording') await stopGate;
  const gate = name === 'start_recording_with_devices_and_meeting' ? replyGate : undefined;
  const value = await rpc(name, args);
  if (name === 'start_recording_with_devices_and_meeting') { committedSignal?.(); await gate; }
  if (name === 'start_recording_with_devices_and_meeting' && loseReply) throw new Error('Synthetic lost IPC response after native commit');
  return value;
}
mock.module('@tauri-apps/api/core', () => ({ invoke: (name: string, args?: any) => {
  const promise = invokeNative(name, args);
  pending.add(promise);
  void promise.finally(() => pending.delete(promise)).catch(noop);
  return promise;
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@/lib/labs', () => ({ loadLabsPreferences: () => ({ meetingAutomation: enabled }) }));
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ clearTranscripts: noop, setMeetingTitle: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ setIsMeetingActive: noop, meetings: [] }) }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null }, setSelectedDevices: noop }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { IDLE: 'idle', STARTING: 'starting', ERROR: 'error' }, useRecordingState: () => ({ setStatus: noop, activeDuration: 3, inPerson: false }) }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null, writePendingGroup: noop }));
mock.module('@/lib/live-session', () => ({ automaticTitle: () => 'Synthetic combined meeting', beginLiveSession: () => { lives++; } }));
mock.module('@/lib/recordingNotification', () => ({ showRecordingNotification: async () => {} }));
mock.module('@/lib/analytics', () => ({ default: { trackButtonClick: noop, trackTranscriptionSuccess: () => { successes++; }, trackTranscriptionError: noop } }));
mock.module('@/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
mock.module('@/hooks/usePendingGroup', () => ({ usePendingGroup: () => [null, noop] }));
mock.module('@/hooks/useAppAudio', () => ({ useAppAudio: () => ({ loaded: true, onlyApps: false, targets: [], scan: async () => {} }) }));
mock.module('@/components/recording/AppAudioSource', () => ({ useAppAudioSupported: () => true, AppAudioSource: () => null }));
mock.module('@/components/RecordingVoiceLane', () => ({ RecordingVoiceLane: () => null }));
mock.module('@/components/groups/GroupBits', () => ({ GroupPicker: () => null }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
const toast = Object.assign((text: string, options: any) => { notices.push({ text, options }); }, { error: noop, info: noop });
mock.module('sonner', () => ({ toast }));
const { useRecordingStart } = await import('../../src/hooks/useRecordingStart');
const { RecordingControls } = await import('../../src/components/RecordingControls');
const automation = await import('../../src/lib/meeting-automation');
const { recoverAutomatedStart } = await import('../../src/lib/recording-start-recovery');
const { launchRecording, requestRecordingStop } = await import('../../src/lib/recording-launch');
const { recordingService } = await import('../../src/services/recordingService');
const { createLifecycleAutomation } = await import('../../src/lib/meeting-lifecycle');
const { createDiscordAutomation, discordProcess } = await import('../../src/lib/discord-automation');
let manualStart: () => Promise<void>;
function Recorder() { manualStart = useRecordingStart(false, noop).handleRecordingStart; return null; }
async function mount() {
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<><Recorder /><RecordingControls isRecording={true} isRecordingDisabled={false}
    onRecordingStart={noop} onRecordingStop={() => { stopped++; }} onStopInitiated={() => { stopInitiated++; }} /></>); });
  return view;
}
async function drain() {
  for (let i = 0; i < 8; i++) await act(async () => {
    await Promise.allSettled([...pending, ...nativePending]);
    for (let j = 0; j < 20; j++) await Promise.resolve();
  });
}
function call(source: 'zoom' | 'discord', session: string): AutomatedCall {
  return { source, sessionId: session, app: source === 'zoom' ? 'Zoom' : 'Discord',
    process: source === 'zoom' ? 'us.zoom.xos' : discordProcess(session) };
}
async function fixture(source: 'zoom' | 'discord', session: string | null, state = 'active', qualified = true, optIn = true) {
  await rpc('fixture_source', { source, session_id: session, state, qualified, enabled: optIn,
    transition: state === 'inactive' ? 'end' : 'start' });
}
function reserve(source: 'zoom' | 'discord', session: string) {
  expect(automation.markAutomatedStart(call(source, session))).toBe(true);
  return automation.automatedStartInProgress()!;
}
async function start(source: 'zoom' | 'discord', session: string) {
  await fixture(source, session);
  const reserved = reserve(source, session);
  await act(async () => { launchRecording(noop, { automated: reserved }); });
  await drain();
  return { reserved, lease: (await recordingService.getRecordingState()).recording_lease! };
}
async function stopManual() {
  expect(await recordingService.stopRecording(`${data}/manual-${++sequence}.wav`)).toBe(true);
  await recoverAutomatedStart();
}

describe.skipIf(!binary)('combined mounted hook/service -> Rust production authorization (synthetic desktop/device adapters)', () => {
  beforeAll(() => {
    child = spawn(binary!, [], { stdio: ['pipe', 'pipe', 'pipe'] });
    createInterface({ input: child.stdout }).on('line', line => {
      const result = JSON.parse(line), request = waiting.get(result.id);
      if (!request) throw new Error('Unexpected native response');
      waiting.delete(result.id);
      if (result.error) request.reject(new Error(result.error)); else request.resolve(result.value);
    });
    child.stderr.on('data', chunk => process.stderr.write(chunk));
    child.on('exit', code => { for (const request of waiting.values()) request.reject(new Error(`Native fixture exited ${code}`)); waiting.clear(); });
  });
  afterAll(() => child.kill());
  beforeEach(async () => {
    await rpc('fixture_reset');
    store.clear(); calls.length = 0; lives = stopped = successes = stopInitiated = 0; notices = [];
    enabled = true; loseReply = false; stopGate = modelGate = replyGate = undefined; committedSignal = undefined; window.location.pathname = '/';
    await fixture('zoom', null, 'unknown', false); await fixture('discord', null, 'unknown', false);
  });

  for (const [first, second] of [['zoom', 'discord'], ['discord', 'zoom']] as const) {
    test(`${first} and ${second}: shared adapter claims, actual native owner, manual busy and no duplicate subscription/start`, async () => {
      const view = await mount();
      const zoom = createLifecycleAutomation({ enabled: () => enabled, state: () => recordingService.getRecordingState(),
        start: expected => launchRecording(noop, { automated: expected }), stop: expected => requestRecordingStop(noop, expected), error: error => { throw error; }, conflict: noop });
      const discord = createDiscordAutomation({ allowed: () => enabled, status: () => rpc('get_discord_automation_status'), recording: () => recordingService.getRecordingState(),
        start: expected => launchRecording(noop, { automated: expected }), stop: expected => requestRecordingStop(noop, expected), error: error => { throw error; } });
      try {
        await fixture('zoom', 'z-source'); await fixture('discord', 'd-source');
        async function signal(source: string) {
          if (source === 'zoom') zoom.accept(await rpc('get_meeting_lifecycle_snapshot'));
          else discord.update(await rpc('get_discord_automation_status'));
        }
        await act(async () => { await signal(first); await signal(second); }); await drain();
        const native = await recordingService.getRecordingState();
        expect(native.recording_lease?.owner?.source).toBe(first);
        expect((await rpc('fixture_counts')).captures).toBe(1);
        expect(store.has('discordAutomationClaim')).toBe(false); expect(store.has('meetingLifecycleConsumed')).toBe(false);
        await act(async () => { await signal(first); await signal(second); }); await drain();
        expect((await rpc('fixture_counts')).captures).toBe(1);
        // Different source cannot Stop the current source's take.
        await fixture(second, null, 'inactive'); await act(async () => { await signal(second); }); await drain();
        expect((await recordingService.getRecordingState()).is_recording).toBe(true);
        await fixture(first, first === 'zoom' ? null : 'd-source', 'inactive');
        await act(async () => { await signal(first); }); await drain();
        expect((await rpc('fixture_counts')).saves).toBe(1);
        await recoverAutomatedStart();
        // Explicit manual capture is never replaced by another observed session.
        await act(async () => { await manualStart(); }); await drain();
        await fixture(second, 'manual-busy'); await act(async () => { await signal(second); }); await drain();
        expect((await recordingService.getRecordingState()).recording_lease?.owner).toBeNull();
        expect((await rpc('fixture_counts')).captures).toBe(2);
        expect(readdirSync(data).some(path => path.endsWith('.wav'))).toBe(true);
      } finally { zoom.dispose(); discord.dispose(); act(() => view.unmount()); }
    });

    test(`${first} cancel-before-dispatch survives reload, fences delayed A after ${second} B`, async () => {
      await fixture(first, 'cancel-before-A');
      reserve(first, 'cancel-before-A'); const a = automation.takeAutomatedStart()!;
      const expected = automation.freezeAutomatedStart(a, await recordingService.getRecordingStartCapability())!;
      store.set('autoStartRecording', JSON.stringify({ kind: 'automated', call: a }));
      const view = await mount(); await drain();
      try {
        expect(automation.automatedStartState().kind).toBe('absent');
        const { lease } = await start(second, 'B-after-cancel');
        const livesBefore = lives;
        await expect(recordingService.startRecordingWithDevices(null, null, 'late A', a.sessionId, expected)).rejects.toThrow();
        automation.releaseAutomatedStart(a); automation.endAutomatedRecording(a);
        expect((await recordingService.getRecordingState()).recording_lease).toEqual(lease);
        expect(lives).toBe(livesBefore); expect((await rpc('fixture_counts')).captures).toBe(1);
      } finally { act(() => view.unmount()); }
    });

    test(`${first} late committed callback after reload cannot adopt or release ${second} B`, async () => {
      let view = await mount(); await fixture(first, 'late-reply-A');
      let deliver!: () => void; replyGate = new Promise(resolve => { deliver = resolve; });
      const committed = new Promise<void>(resolve => { committedSignal = resolve; });
      const a = reserve(first, 'late-reply-A');
      await act(async () => { launchRecording(noop, { automated: a }); });
      await committed; committedSignal = undefined; replyGate = undefined;
      const aLease = (await recordingService.getRecordingState()).recording_lease!;
      act(() => view.unmount()); view = await mount();
      expect((await recoverAutomatedStart())?.recording_lease).toEqual(aLease);
      await stopManual();
      await fixture(second, 'late-reply-B'); const b = reserve(second, 'late-reply-B');
      const bCommitted = new Promise<void>(resolve => { committedSignal = resolve; });
      await act(async () => { launchRecording(noop, { automated: b }); });
      await bCommitted; committedSignal = undefined;
      const bLease = (await recordingService.getRecordingState()).recording_lease!;
      deliver(); await drain();
      expect(bLease.owner?.source).toBe(second);
      expect((await recordingService.getRecordingState()).recording_lease).toEqual(bLease);
      expect(automation.automatedRecording()?.token).toBe(b.token);
      expect(lives).toBe(1); expect((await rpc('fixture_counts')).captures).toBe(2);
      act(() => view.unmount());
    });

    test(`${first} held native start settles committed on reload; ${second} cannot acquire on idle snapshot`, async () => {
      let view = await mount(); await fixture(first, 'held-A'); await rpc('fixture_hold');
      const a = reserve(first, 'held-A');
      await act(async () => { launchRecording(noop, { automated: a }); });
      await rpc('fixture_entered');
      expect((await recordingService.getRecordingState()).is_recording).toBe(false);
      const persisted = automation.automatedStartState();
      expect(persisted.kind).toBe('native_unresolved');
      act(() => view.unmount());
      view = await mount();
      expect(automation.markAutomatedStart(call(second, 'busy-observed'))).toBe(false);
      await rpc('fixture_release'); await drain();
      try {
        const receipt = await recoverAutomatedStart();
        expect(receipt?.outcome).toBe('committed'); expect(receipt?.is_recording).toBe(true);
        expect((await recordingService.getRecordingState()).recording_lease?.owner?.source).toBe(first);
        expect(automation.markAutomatedStart(call(second, 'still-busy'))).toBe(false);
        await stopManual();
        expect(automation.automatedStartState().kind).toBe('absent');
        await start(second, 'new-B-after-stop');
        expect((await rpc('fixture_counts')).captures).toBe(2);
      } finally { act(() => view.unmount()); }
    });
  }

  test('lost committed response restores only exact owner; keep-recording and stale callbacks cannot release B', async () => {
    let view = await mount(); loseReply = true;
    await start('discord', 'lost-response'); await drain();
    expect((await recordingService.getRecordingState()).is_recording).toBe(true);
    expect(automation.automatedStartState().kind).toBe('native_unresolved');
    loseReply = false;
    act(() => view.unmount()); view = await mount(); await drain();
    const current = (await recordingService.getRecordingState()).recording_lease!;
    const mirror = automation.automatedRecording()!;
    expect(mirror.source).toBe('discord');
    expect(await rpc('release_automated_recording', { expected: current })).toBe(true);
    automation.endAutomatedRecording(mirror);
    await recoverAutomatedStart();
    expect((await recordingService.getRecordingState()).recording_lease?.owner).toBeNull();
    expect(automation.markAutomatedStart(call('zoom', 'cannot-acquire-kept-take'))).toBe(false);
    await stopManual();
    await start('zoom', 'B-after-keep');
    const b = (await recordingService.getRecordingState()).recording_lease;
    expect(await rpc('release_automated_recording', { expected: current })).toBe(false);
    automation.endAutomatedRecording(mirror);
    expect((await recordingService.getRecordingState()).recording_lease).toEqual(b);
    const keepCurrent = notices.find(value => value.text === 'Recording your Zoom call')!.options.action;
    await act(async () => { keepCurrent.onClick(); }); await drain();
    expect((await recordingService.getRecordingState()).recording_lease?.owner).toBeNull();
    expect(automation.automatedStartState().kind).toBe('native_unresolved');
    act(() => view.unmount());
  });

  test('delayed routed guarded Stop A reaches actual Rust after manual B and has no save/UI/toast effects', async () => {
    let view = await mount(); const { lease: a } = await start('zoom', 'stop-A');
    const staleAction = notices.find(value => value.text.includes('Recording your'))!.options.action;
    await stopManual();
    await act(async () => { await manualStart(); }); await drain();
    const b = (await recordingService.getRecordingState()).recording_lease;
    const before = await rpc('fixture_counts'); const oldNotices = notices.length;
    act(() => view.unmount()); window.location.pathname = '/library';
    requestRecordingStop(noop, a); window.location.pathname = '/';
    view = await mount(); await drain();
    expect((await recordingService.getRecordingState()).recording_lease).toEqual(b);
    expect(await rpc('fixture_counts')).toEqual(before);
    expect(successes).toBe(0); expect(stopInitiated).toBe(0); expect(stopped).toBe(0); expect(notices.length).toBe(oldNotices);
    await act(async () => { staleAction.onClick(); }); await drain();
    expect((await recordingService.getRecordingState()).recording_lease).toEqual(b);
    await act(async () => { view.root.findByProps({ 'aria-label': 'Stop recording' }).props.onClick(); }); await drain();
    expect((await rpc('fixture_counts')).saves).toBe(before.saves + 1);
    expect(successes).toBe(1); expect(stopInitiated).toBe(1);
    act(() => view.unmount());
  });

  test('disable queued guarded Stop at native boundary, and real unqualified/source opt-out never capture', async () => {
    const view = await mount(); const { lease } = await start('discord', 'disable-A');
    let release!: () => void; stopGate = new Promise(resolve => { release = resolve; });
    await act(async () => { requestRecordingStop(noop, lease); for (let i = 0; i < 20; i++) await Promise.resolve(); });
    enabled = false; await rpc('set_meeting_automation_enabled', { enabled: false });
    release(); stopGate = undefined; await drain();
    expect((await recordingService.getRecordingState()).recording_lease).toEqual(lease);
    expect((await rpc('fixture_counts')).saves).toBe(0); expect(successes).toBe(0);
    await stopManual(); enabled = true; await rpc('set_meeting_automation_enabled', { enabled: true });
    for (const source of ['zoom', 'discord'] as const) for (const [qualified, optedIn] of [[false, true], [true, false]]) {
      const session = `${source}-${qualified}-${optedIn}`;
      await fixture(source, session, 'active', qualified, optedIn);
      const reserved = reserve(source, session);
      await act(async () => { launchRecording(noop, { automated: reserved }); }); await drain();
    }
    expect((await rpc('fixture_counts')).captures).toBe(1);
    act(() => view.unmount());
  });

  test('future barrier error retains fence; stopping/busy receipt never unlocks; valid cancellation eventually admits B', async () => {
    await fixture('zoom', 'future-A'); reserve('zoom', 'future-A');
    const a = automation.takeAutomatedStart()!, cap = await recordingService.getRecordingStartCapability();
    const expected = automation.freezeAutomatedStart(a, { ...cap, next_sequence: String(BigInt(cap.next_sequence) + BigInt(1)) })!;
    await expect(recoverAutomatedStart()).rejects.toThrow();
    expect(automation.automatedStartState().kind).toBe('native_unresolved');
    const preceding = { ...expected, sequence: cap.next_sequence, token: 'preceding-unseen-ticket' };
    await recordingService.cancelAutomatedRecordingStart(preceding);
    await rpc('fixture_stopping', { stopping: true });
    const busy = await recoverAutomatedStart();
    expect(busy?.is_stopping).toBe(true); expect(automation.automatedStartState().kind).toBe('native_unresolved');
    expect(automation.markAutomatedStart(call('discord', 'observed-while-stopping'))).toBe(false);
    await rpc('fixture_stopping', { stopping: false });
    expect((await recoverAutomatedStart())?.no_future_commit).toBe(true);
    expect(automation.automatedStartState().kind).toBe('absent');
    const view = await mount(); await start('discord', 'after-future-barrier');
    expect((await rpc('fixture_counts')).captures).toBe(1); act(() => view.unmount());
  });

  test('old process epoch receipt includes actual epoch, and failed dispatched A settles idle before B', async () => {
    await fixture('zoom', 'old-process-A'); reserve('zoom', 'old-process-A');
    const a = automation.takeAutomatedStart()!, capability = await recordingService.getRecordingStartCapability();
    const old = automation.freezeAutomatedStart(a, { ...capability, epoch: 'previous-native-process' })!;
    const receipt = await recoverAutomatedStart();
    expect(receipt?.expected).toEqual(old); expect(receipt?.outcome).toBe('epoch_mismatch');
    expect(receipt?.native_epoch).toBe(capability.epoch); expect(automation.automatedStartState().kind).toBe('absent');
    await expect(recordingService.startRecordingWithDevices(null, null, 'old process A', a.sessionId, old)).rejects.toThrow();
    const view = await mount(); await rpc('fixture_fail', { fail: true });
    await start('zoom', 'failed-device-start');
    expect((await rpc('fixture_counts')).captures).toBe(0);
    expect(automation.automatedStartState().kind).toBe('absent');
    await rpc('fixture_fail', { fail: false }); await start('discord', 'B-after-failure');
    expect((await rpc('fixture_counts')).captures).toBe(1); act(() => view.unmount());
  });

  test('malformed/missing automatic route never manual fallback; actual explicit manual routed start still saves', async () => {
    for (const value of ['{}', '{broken', '{"app":"Zoom","process":""}', null]) {
      store.clear(); if (value) store.set('labsAutoStartPending', value);
      store.set('autoStartRecording', JSON.stringify({ kind: 'automated', call: call('zoom', 'missing') }));
      const view = await mount(); await drain();
      await act(async () => { events.dispatchEvent(new Event('start-recording-from-sidebar')); }); await drain();
      expect((await rpc('fixture_counts')).captures).toBe(0); act(() => view.unmount());
    }
    store.clear(); window.location.pathname = '/library'; launchRecording(noop); window.location.pathname = '/';
    const view = await mount(); await drain();
    expect((await recordingService.getRecordingState()).recording_lease?.owner).toBeNull();
    expect((await rpc('fixture_counts')).captures).toBe(1);
    await act(async () => { view.root.findByProps({ 'aria-label': 'Stop recording' }).props.onClick(); }); await drain();
    expect((await rpc('fixture_counts')).saves).toBe(1); act(() => view.unmount());
  });
});
