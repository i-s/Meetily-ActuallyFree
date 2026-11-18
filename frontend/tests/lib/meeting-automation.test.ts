import { beforeEach, describe, test } from 'node:test';
import assert from 'node:assert/strict';
import * as automation from '../../src/lib/meeting-automation';
import type { RecordingLease, StartRequest, StartResolution } from '../../src/lib/recording-start-authority';
const store = new Map<string, string>();
const announced: string[] = [];
Object.assign(globalThis, { sessionStorage: {
  getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k),
}, window: { dispatchEvent: (e: Event) => announced.push(e.type) } });
const { automatedRecording, automatedStartInProgress, beginAutomatedRecording, endAutomatedRecording,
  markAutomatedStart, takeAutomatedStart, takeAutomatedStartResult, releaseAutomatedStart, reservationCurrent,
  automatedStartState, freezeAutomatedStart, applyAutomatedStartResolution, automationUnavailableMessage } = automation;
const zoom = { app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'z1' };
const discord = { app: 'Discord', process: 'discord-macos:d1', source: 'discord', sessionId: 'd1' };
const capability = { version: 1 as const, epoch: 'native-epoch', next_sequence: '1' };
function owner(call: automation.AutomatedCall): RecordingLease {
  return { generation: `native:${call.token}`, owner: { source: call.source!, session_id: call.sessionId!, app: call.app, process: call.process } };
}
function receipt(expected: StartRequest, overrides: Partial<StartResolution> = {}): StartResolution {
  return { version: 1, native_epoch: expected.epoch, expected, terminal: true, no_future_commit: true,
    outcome: 'cancelled', committed_lease: null, recording_lease: null, is_recording: false, is_stopping: false, ...overrides };
}
let now = 1000;
const realNow = Date.now;
// Clock is deterministic and restored after this file's tests, with no sleeps.
import { after } from 'node:test';
after(() => { Date.now = realNow; });
describe('shared reservation expiry and native authority', () => {
  beforeEach(() => { store.clear(); announced.length = 0; now = 1000; Date.now = () => now; });
  for (const [a, b] of [[zoom, discord], [discord, zoom]]) {
    test(`${a.source} pending expiry admits fresh ${b.source}, never consumed A, including reload`, async () => {
      assert.equal(markAutomatedStart(a), true); const pending = automatedStartInProgress()!;
      now += 60_001;
      const reloaded = await import(`../../src/lib/meeting-automation.ts?pending=${a.source}`);
      assert.equal(reloaded.automatedStartInProgress(), null);
      assert.equal(reloaded.markAutomatedStart(a), false);
      assert.equal(reloaded.markAutomatedStart(b), true);
      const next = reloaded.takeAutomatedStart()!;
      releaseAutomatedStart(pending);
      assert.equal(reservationCurrent(next, 'inflight'), true);
      assert.equal(beginAutomatedRecording(pending, owner(pending)), false);
    });
    test(`${a.source} preparation expiry is locally cancellable and stale async work cannot dispatch`, () => {
      markAutomatedStart(a); const old = takeAutomatedStart()!; now += 60_001;
      assert.equal(automatedStartInProgress(), null);
      assert.equal(markAutomatedStart(b), true); const next = takeAutomatedStart()!;
      assert.equal(freezeAutomatedStart(old, capability), null);
      releaseAutomatedStart(old); assert.equal(reservationCurrent(next, 'inflight'), true);
      const expected = freezeAutomatedStart(next, capability)!;
      assert.ok(Object.isFrozen(expected));
      assert.equal(beginAutomatedRecording(next, owner(next)), true);
      endAutomatedRecording(old); assert.equal(reservationCurrent(next, 'recording'), true);
    });
    test(`${a.source} held native start survives expiry, release, reload, idle snapshot; terminal cancel fences late A before B`, async () => {
      markAutomatedStart(a); const old = takeAutomatedStart()!;
      const expected = freezeAutomatedStart(old, capability)!;
      // Held dispatch owns this immutable tuple; a late reply never fetches B.
      let deliver!: (lease: RecordingLease) => void;
      const lateReply = new Promise<RecordingLease>((resolve) => { deliver = resolve; });
      const completion = lateReply.then((lease) => beginAutomatedRecording(old, lease));
      now += 60_001;
      assert.equal(automatedStartInProgress(), null);
      releaseAutomatedStart(old); endAutomatedRecording();
      const reloaded = await import(`../../src/lib/meeting-automation.ts?native=${a.source}`);
      const state = reloaded.automatedStartState(); assert.equal(state.kind, 'native_unresolved');
      assert.deepEqual(state.kind === 'native_unresolved' && state.expected, expected);
      assert.equal(reloaded.markAutomatedStart(b), false); // B consumed, not queued.
      assert.equal(applyAutomatedStartResolution(old, expected, { is_recording: false }), 'unresolved');
      assert.equal(automatedStartState().kind, 'native_unresolved');
      assert.equal(reloaded.applyAutomatedStartResolution(old, expected, receipt(expected)), 'settled_idle');
      assert.equal(reloaded.markAutomatedStart(a), false); assert.equal(reloaded.markAutomatedStart(b), false);
      const fresh = { ...b, sessionId: `${b.sessionId}:new` };
      assert.equal(reloaded.markAutomatedStart(fresh), true); const next = reloaded.takeAutomatedStart()!;
      const nextExpected = reloaded.freezeAutomatedStart(next, { ...capability, next_sequence: '2' })!;
      deliver(owner(old)); assert.equal(await completion, false);
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected)), 'unresolved');
      releaseAutomatedStart(old); endAutomatedRecording(old);
      assert.equal(reservationCurrent(next, 'inflight'), true);
      assert.equal(beginAutomatedRecording(next, owner(next)), true);
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected)), 'unresolved');
      assert.equal(reservationCurrent(next, 'recording'), true);
      assert.equal(applyAutomatedStartResolution(next, nextExpected, receipt(nextExpected)), 'settled_idle');
    });
    test(`${a.source} held native setup settles busy before fresh ${b.source}; no timeout-to-release`, async () => {
      markAutomatedStart(a); const old = takeAutomatedStart()!;
      const expected = freezeAutomatedStart(old, capability)!;
      let finishSetup!: () => void;
      const held = new Promise<void>((resolve) => { finishSetup = resolve; });
      let nativeLease: RecordingLease | null = null;
      // Synthetic native boundary models #10's serialized lock: a barrier
      // cannot resolve while admitted setup owns it. No capture/device runs.
      const nativeStart = held.then(() => { nativeLease = owner(old); return nativeLease; });
      let barrierReturned = false;
      const barrier = nativeStart.then((lease) => {
        barrierReturned = true;
        return receipt(expected, { outcome: 'committed', committed_lease: lease, recording_lease: lease, is_recording: true });
      });
      now += 60_001; endAutomatedRecording();
      await Promise.resolve(); assert.equal(barrierReturned, false);
      assert.equal(markAutomatedStart(b), false);
      assert.equal(automatedStartState().kind, 'native_unresolved');
      finishSetup();
      assert.equal(applyAutomatedStartResolution(old, expected, await barrier), 'settled_busy');
      assert.equal(beginAutomatedRecording(old, await nativeStart), false); // Expired/revoked late reply.
      assert.ok(nativeLease); // Cancel did not pretend to stop a committed take.
      nativeLease = null; // Explicit native manual Stop fixture.
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected, { outcome: 'retired' })), 'settled_idle');
      assert.equal(markAutomatedStart({ ...b, sessionId: 'new-after-held-setup' }), true);
    });
    test(`${a.source} cancel barrier before delayed IPC permanently fences A while ${b.source} starts`, async () => {
      markAutomatedStart(a); const old = takeAutomatedStart()!;
      const expected = freezeAutomatedStart(old, capability)!;
      let dispatch!: () => void;
      const delayed = new Promise<void>((resolve) => { dispatch = resolve; });
      let watermark = 1, captures = 0;
      const nativeStart = delayed.then(() => {
        if (Number(expected.sequence) < watermark) return 'rejected';
        captures++; return 'committed';
      });
      watermark = 2; // Synthetic cancel wins native lock and consumes sequence1.
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected)), 'settled_idle');
      assert.equal(markAutomatedStart(b), true); const next = takeAutomatedStart()!;
      freezeAutomatedStart(next, { ...capability, next_sequence: '2' });
      assert.equal(beginAutomatedRecording(next, owner(next)), true);
      dispatch(); assert.equal(await nativeStart, 'rejected'); assert.equal(captures, 0);
      assert.equal(reservationCurrent(next, 'recording'), true);
    });
    test(`${a.source} admitted/committed A and manual/native owner exclude B until real idle barrier`, () => {
      markAutomatedStart(a); const old = takeAutomatedStart()!;
      const expected = freezeAutomatedStart(old, capability)!;
      assert.equal(beginAutomatedRecording(old, owner(old)), true);
      assert.equal(automatedStartInProgress(), null); assert.equal(automatedRecording()?.token, old.token);
      const busy = receipt(expected, { outcome: 'committed', committed_lease: owner(old), recording_lease: owner(old), is_recording: true });
      assert.equal(applyAutomatedStartResolution(old, expected, busy), 'settled_busy');
      endAutomatedRecording(); assert.equal(automatedRecording(), null);
      assert.equal(markAutomatedStart(b), false);
      const manual: RecordingLease = { generation: 'manual-B', owner: null };
      assert.equal(applyAutomatedStartResolution(old, expected, { ...busy, recording_lease: manual }), 'settled_busy');
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected, { is_stopping: true })), 'settled_busy');
      assert.equal(applyAutomatedStartResolution(old, expected, receipt(expected, { outcome: 'retired' })), 'settled_idle');
      assert.equal(markAutomatedStart(b), false);
      assert.equal(markAutomatedStart({ ...b, sessionId: 'new-after-manual' }), true);
    });
  }
  test('absent vs malformed/expired automatic payloads, including unstaged legacy inflight', () => {
    assert.deepEqual(takeAutomatedStartResult(), { kind: 'absent' });
    assert.deepEqual(takeAutomatedStartResult({ ...zoom, token: 'expired-routed-A', requestedAt: now }), { kind: 'rejected', reason: 'automated_reservation_missing' });
    for (const raw of ['{}', 'null', '[]', '', '{', 'true']) {
      store.set('labsAutoStartPending', raw);
      assert.equal(takeAutomatedStartResult().kind, 'rejected');
      assert.equal(markAutomatedStart({ ...zoom, sessionId: raw || 'empty' }), false);
    }
    store.clear(); markAutomatedStart(zoom); now += 60_001;
    assert.deepEqual(takeAutomatedStartResult(), { kind: 'rejected', reason: 'expired_pending' });
    store.clear(); markAutomatedStart(zoom); const old = takeAutomatedStart()!;
    store.set('labsAutoStartInFlight', JSON.stringify({ ...old, startStage: undefined }));
    now += 60_001; endAutomatedRecording(); releaseAutomatedStart(old);
    assert.equal(automatedStartState().kind, 'native_unresolved');
    assert.equal(markAutomatedStart(discord), false);
  });
  test('expired/future-dated preparation never authorizes dispatch; request timestamp cannot be forged by caller', () => {
    markAutomatedStart(zoom); const old = takeAutomatedStart()!; now += 60_001;
    assert.equal(reservationCurrent({ ...old, requestedAt: now }, 'inflight'), false);
    assert.equal(freezeAutomatedStart(old, capability), null);
    now = old.requestedAt! - 1;
    assert.equal(reservationCurrent(old, 'inflight'), false);
  });
  test('stale routed A cannot take B; source/session/token/app all bind scope', () => {
    markAutomatedStart(zoom); const a = automatedStartInProgress()!; releaseAutomatedStart(a);
    markAutomatedStart(discord); const b = automatedStartInProgress()!;
    assert.equal(takeAutomatedStartResult(a).kind, 'rejected');
    releaseAutomatedStart({ ...b, source: 'other' });
    assert.equal(reservationCurrent(b, 'pending'), true);
    assert.equal(takeAutomatedStartResult(b).kind, 'accepted');
  });
  test('no authority, missing/mismatched lease and unverified receipt cannot promote/release', () => {
    markAutomatedStart(zoom); const call = takeAutomatedStart()!;
    assert.equal(beginAutomatedRecording(call, owner(call)), false);
    assert.equal(freezeAutomatedStart(call, {} as any), null);
    const expected = freezeAutomatedStart(call, capability)!;
    assert.equal(beginAutomatedRecording(call), false);
    assert.equal(beginAutomatedRecording(call, { generation: 'manual', owner: null }), false);
    for (const bad of [undefined, {}, { ...receipt(expected), no_future_commit: false },
      receipt({ ...expected, token: 'B' }), receipt(expected, { native_epoch: 'different' }),
      receipt(expected, { recording_lease: owner(call) })]) {
      assert.equal(applyAutomatedStartResolution(call, expected, bad), 'unresolved');
    }
    assert.equal(automatedStartState().kind, 'native_unresolved');
    assert.equal(applyAutomatedStartResolution(call, expected, receipt(expected, { outcome: 'epoch_mismatch', native_epoch: 'new-process' })), 'settled_idle');
  });
  test('future/out-of-order error leaves fence; advancing capability is not proof; later real receipt releases it', () => {
    markAutomatedStart(zoom); const call = takeAutomatedStart()!;
    const expected = freezeAutomatedStart(call, { ...capability, next_sequence: '9' })!;
    assert.equal(applyAutomatedStartResolution(call, expected, new Error('Future sequence')), 'unresolved');
    assert.equal(freezeAutomatedStart(call, { ...capability, next_sequence: '10' }), null);
    assert.equal(markAutomatedStart(discord), false);
    assert.equal(applyAutomatedStartResolution(call, expected, receipt(expected)), 'settled_idle');
    assert.equal(markAutomatedStart({ ...discord, sessionId: 'later-new' }), true);
  });
  test('256-session cap is visible, retains every tombstone, never clears unresolved authority to recover', () => {
    for (let i = 0; i < 256; i++) {
      assert.equal(markAutomatedStart({ ...zoom, sessionId: `session-${i}` }), true);
      releaseAutomatedStart(automatedStartInProgress()!);
    }
    assert.equal(automatedStartState().kind, 'rejected');
    assert.match(automationUnavailableMessage()!, /256-session safety history is full/);
    assert.match(automationUnavailableMessage()!, /Manual controls remain available/);
    endAutomatedRecording();
    assert.equal(markAutomatedStart({ ...zoom, sessionId: 'session-0' }), false);
    assert.equal(markAutomatedStart({ ...zoom, sessionId: 'genuinely-new' }), false);
    assert.equal(JSON.parse(store.get('labsAutoStartConsumed')!).length, 256);
    // Only a clean session after stopping native/end-all-calls is documented as
    // recovery; no public reset API accepts an idle snapshot or evicts history.
  });
  test('active owner exclusion and storage failure remain fail closed', () => {
    store.set('labsAutoRecordingProcess', 'legacy-manual-display');
    assert.equal(markAutomatedStart(zoom), false); assert.ok(automatedRecording());
    endAutomatedRecording(); store.clear();
    const original = sessionStorage.setItem; sessionStorage.setItem = () => { throw new Error('blocked'); };
    try { assert.equal(markAutomatedStart(zoom), false); assert.equal(beginAutomatedRecording(zoom), false); }
    finally { sessionStorage.setItem = original; }
  });
});
