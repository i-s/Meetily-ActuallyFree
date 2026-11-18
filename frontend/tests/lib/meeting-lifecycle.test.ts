import type { RecordingLease } from '../../src/lib/native-recording';
import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { createLifecycleAutomation, type LifecycleSnapshot } from '../../src/lib/meeting-lifecycle';
import { automatedRecording, beginAutomatedRecording, endAutomatedRecording, takeAutomatedStart, releaseAutomatedStart, freezeAutomatedStart, automatedStartState, applyAutomatedStartResolution, markAutomatedStart } from '../../src/lib/meeting-automation';

const store = new Map<string, string>();
Object.assign(globalThis, { sessionStorage: {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k),
}, window: { dispatchEvent: () => true } });
let revision = 0;
let recording = false;
let nativeLease: RecordingLease | null = null;
let generation = 0;
let stopping = false;
let enabled = true;
let starts = 0;
let stops = 0;
let conflicts = 0;
const errors: unknown[] = [];
function snapshot(id: string | null, presence: 'active' | 'inactive' | 'unknown' = 'active', source = 'zoom'): LifecycleSnapshot {
  return { revision: ++revision, enabled: true, automation_enabled: true, sources: [{ source, app: source, process: source, presence, reason: 'synthetic_controls', session_id: id, qualified: true }] };
}
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };
type RecordingQuery = { is_recording?: boolean; is_stopping?: boolean };
function controller(state: () => Promise<RecordingQuery> = async () => ({ is_recording: recording, is_stopping: stopping })) {
  return createLifecycleAutomation({ enabled: () => enabled, state: async () => ({ ...await state(), recording_lease: nativeLease }),
    start: () => { starts++; }, stop: (expected) => { assert.deepEqual(expected, nativeLease); stops++; recording = false; nativeLease = null; settleIdle(); },
    conflict: () => { conflicts++; }, error: e => errors.push(e),
  });
}
function settleIdle() {
  const state = automatedStartState();
  if (state.kind === 'native_unresolved' && state.expected) applyAutomatedStartResolution(state.call, state.expected, {
    version: 1, native_epoch: state.expected.epoch, expected: state.expected, terminal: true, no_future_commit: true,
    outcome: 'cancelled', committed_lease: null, recording_lease: null, is_recording: false, is_stopping: false,
  });
}
function finishStart() {
  const call = takeAutomatedStart(); assert.ok(call);
  freezeAutomatedStart(call, { version: 1, epoch: 'synthetic-native', next_sequence: String(++generation) });
  nativeLease = { generation: String(generation), owner: { source: call.source!, session_id: call.sessionId!, app: call.app, process: call.process } };
  assert.equal(beginAutomatedRecording(call, nativeLease), true); recording = true; return call;
}
beforeEach(() => { store.clear(); recording = false; nativeLease = null; stopping = false; enabled = true; starts = 0; stops = 0; conflicts = 0; errors.length = 0; });

test('idle, preview/device tests and unknown never request a start', async () => {
  const c = controller();
  for (const state of ['inactive', 'unknown'] as const) { c.accept(snapshot(null, state)); await flush(); }
  assert.equal(starts, 0); assert.equal(stops, 0); c.dispose();
});
test('confirmed muted join starts once; unknown, missing source, silence/reconnect retain ownership', async () => {
  const c = controller(); c.accept(snapshot('one')); await flush(); const call = finishStart();
  for (const presence of ['active', 'unknown', 'inactive', 'active'] as const) {
    // Native grace retains the session even on observed Home while reconnecting.
    c.accept(snapshot('one', presence)); await flush();
  }
  c.accept({ revision: ++revision, enabled: true, automation_enabled: true, sources: [] }); await flush();
  assert.equal(starts, 1); assert.equal(stops, 0); assert.equal(automatedRecording()?.sessionId, call.sessionId);
  c.accept(snapshot(null, 'inactive')); await flush(); assert.equal(stops, 1); c.dispose();
});
test('manual stop suppresses same session across controller remount; next actual call rearms', async () => {
  let c = controller(); c.accept(snapshot('one')); await flush(); finishStart();
  endAutomatedRecording(); recording = false; nativeLease = null; settleIdle(); c.dispose(); c = controller();
  c.accept(snapshot('one')); await flush(); assert.equal(starts, 1);
  c.accept(snapshot(null, 'inactive')); await flush(); c.accept(snapshot('two')); await flush();
  assert.equal(starts, 2); c.dispose();
});
test('manual recording, stopping state and other source owner are never replaced', async () => {
  const c = controller(); recording = true; c.accept(snapshot('busy')); await flush();
  assert.equal(starts, 0); assert.equal(conflicts, 1);
  nativeLease = { generation: String(++generation), owner: { app: 'Discord', process: 'discord', source: 'discord', session_id: 'discord-one' } };
  c.accept(snapshot(null, 'inactive')); await flush(); assert.equal(stops, 0);
  endAutomatedRecording(); recording = false; nativeLease = null; stopping = true; c.accept(snapshot('new')); await flush();
  assert.equal(starts, 0); c.dispose();
});
test('pending/start-in-flight reserves the single recorder across simultaneous sources', async () => {
  const c = controller(); const zoom = snapshot('z'); const discord = snapshot('d', 'active', 'discord');
  c.accept({ ...zoom, revision: ++revision, sources: [...zoom.sources, ...discord.sources] }); await flush();
  assert.equal(starts, 1); const call = takeAutomatedStart(); assert.equal(call?.source, 'zoom');
  c.accept({ ...zoom, revision: ++revision, sources: [...zoom.sources, ...discord.sources] }); await flush();
  assert.equal(starts, 1); assert.equal(markAutomatedStart({ app: 'Discord', process: 'discord', source: 'discord', sessionId: 'd' }), false); releaseAutomatedStart(call!); c.dispose();
});
test('leave while recording-state query is pending cancels delayed start', async () => {
  let resolve!: (state: RecordingQuery) => void;
  const c = controller(() => new Promise(r => { resolve = r; }));
  c.accept(snapshot('late')); c.accept(snapshot(null, 'inactive')); resolve({}); await flush();
  assert.equal(starts, 0); c.dispose();
});
test('stale snapshots, disabled detection, and unknown never cause synthetic stop', async () => {
  const c = controller(); c.accept(snapshot('one')); await flush(); finishStart();
  c.accept({ ...snapshot(null, 'inactive'), enabled: false }); await flush(); assert.equal(stops, 0);
  c.accept(snapshot(null, 'unknown')); await flush(); assert.equal(stops, 0);
  c.accept({ ...snapshot(null, 'inactive'), revision: 0 }); await flush(); assert.equal(stops, 0);
  c.dispose();
});
test('manual override during async stop query wins over old owner', async () => {
  let resolve!: (state: RecordingQuery) => void;
  let gate = false;
  const c = controller(() => gate ? new Promise(r => { resolve = r; }) : Promise.resolve({ is_recording: recording }));
  c.accept(snapshot('one')); await flush(); finishStart(); gate = true;
  c.accept(snapshot(null, 'inactive')); endAutomatedRecording(); recording = true; nativeLease = { generation: String(++generation), owner: null };
  resolve({ is_recording: true }); await flush(); assert.equal(stops, 0); c.dispose();
});

test('Unknown during pending state lookup cannot authorize a new start', async () => {
  let resolve!: (state: RecordingQuery) => void;
  const c = controller(() => new Promise(r => { resolve = r; }));
  c.accept(snapshot('permission-lost'));
  c.accept(snapshot('permission-lost', 'unknown'));
  resolve({}); await flush();
  assert.equal(starts, 0); c.dispose();
});

test('R1 disable while awaited state is pending must not route an automatic stop', async () => {
  let resolve!: (state: RecordingQuery) => void;
  let gate = false;
  const c = controller(() => gate ? new Promise(r => { resolve = r; }) : Promise.resolve({ is_recording: recording }));
  c.accept(snapshot('disable-race')); await flush(); finishStart(); gate = true;
  c.accept(snapshot(null, 'inactive'));
  enabled = false;
  resolve({ is_recording: true }); await flush();
  assert.equal(stops, 0); c.dispose();
});

test('R1 an unqualified positive candidate must not request recording', async () => {
  const c = controller();
  const observed = snapshot('unqualified');
  c.accept({ ...observed, sources: observed.sources.map(s => ({ ...s, qualified: false })) });
  await flush(); assert.equal(starts, 0); c.dispose();
});

test('R1 reload restores native lifecycle owner without a JS completion promise', async () => {
  const first = controller(); first.accept(snapshot('reload')); await flush(); finishStart(); first.dispose();
  store.clear(); // simulate lost browser owner, native take continues
  const c = controller(); c.accept(snapshot('reload')); await flush();
  assert.equal(nativeLease?.owner?.session_id, 'reload');
  assert.equal(automatedRecording(), null); // Never fabricate a JS token from a raw native snapshot.
  c.accept(snapshot(null, 'inactive')); await flush(); assert.equal(stops, 1); c.dispose();
});
