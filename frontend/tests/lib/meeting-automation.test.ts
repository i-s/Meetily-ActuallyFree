import { beforeEach, describe, test } from 'node:test';
import assert from 'node:assert/strict';
const store = new Map<string, string>();
const announced: string[] = [];
Object.assign(globalThis, { sessionStorage: {
  getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k),
}, window: { dispatchEvent: (e: Event) => announced.push(e.type) } });
const { automatedRecording, automatedStartInProgress, beginAutomatedRecording, endAutomatedRecording,
  markAutomatedStart, takeAutomatedStart, releaseAutomatedStart, reservationCurrent } = await import('../../src/lib/meeting-automation');
const zoom = { app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'z1' };
const discord = { app: 'Discord', process: 'discord-macos:d1', source: 'discord', sessionId: 'd1' };

describe('one shared source/session/token reservation', () => {
  beforeEach(() => { store.clear(); announced.length = 0; });
  test('pending transfers the same token once to in-flight and then recording', () => {
    assert.equal(markAutomatedStart(zoom), true);
    const pending = automatedStartInProgress()!;
    assert.ok(pending.token); assert.equal(reservationCurrent(pending, 'pending'), true);
    const call = takeAutomatedStart()!;
    assert.deepEqual(call, pending); assert.equal(takeAutomatedStart(), null);
    assert.equal(reservationCurrent(call, 'inflight'), true);
    assert.equal(beginAutomatedRecording(call), true);
    assert.equal(reservationCurrent(call, 'recording'), true);
    assert.equal(automatedStartInProgress(), null);
    assert.deepEqual(automatedRecording(), call);
    endAutomatedRecording(); assert.equal(automatedRecording(), null);
    assert.equal(announced.length, 2);
  });
  test('pending Zoom and in-flight Zoom reject Discord; reciprocal order also holds', () => {
    for (const [a, b] of [[zoom, discord], [discord, zoom]]) {
      store.clear(); assert.equal(markAutomatedStart(a), true);
      assert.equal(markAutomatedStart(b), false);
      const call = takeAutomatedStart()!;
      assert.equal(markAutomatedStart(b), false);
      assert.equal(automatedStartInProgress()?.token, call.token);
    }
  });
  test('late release/promotion from A cannot clear or adopt B with same process/session', () => {
    markAutomatedStart(zoom); const a = takeAutomatedStart()!;
    endAutomatedRecording(); markAutomatedStart(zoom); const b = takeAutomatedStart()!;
    assert.notEqual(a.token, b.token);
    releaseAutomatedStart(a); assert.equal(reservationCurrent(b, 'inflight'), true);
    assert.equal(beginAutomatedRecording(a), false);
    assert.equal(beginAutomatedRecording(b), true);
    releaseAutomatedStart(a); assert.equal(reservationCurrent(b, 'recording'), true);
    endAutomatedRecording(a); assert.equal(reservationCurrent(b, 'recording'), true); // stale toast A
    assert.equal(reservationCurrent(a, 'recording'), false);
    endAutomatedRecording(b); assert.equal(automatedRecording(), null);
  });
  test('simultaneous async source claims accept exactly one shared owner', async () => {
    let ready!: () => void;
    const gate = new Promise<void>((resolve) => { ready = resolve; });
    const claims = [zoom, discord].map(async (call) => { await gate; return markAutomatedStart(call); });
    ready(); const results = await Promise.all(claims);
    assert.equal(results.filter(Boolean).length, 1);
    const winner = automatedStartInProgress()!;
    assert.ok(winner.token); assert.equal(winner.source, results[0] ? 'zoom' : 'discord');
    const wrongSource = { ...winner, source: winner.source === 'zoom' ? 'discord' : 'zoom' };
    releaseAutomatedStart(wrongSource);
    assert.equal(reservationCurrent(winner, 'pending'), true);
    assert.equal(reservationCurrent(wrongSource, 'pending'), false);
  });
  test('expired/future-dated reservation is not promoted after model loading', () => {
    markAutomatedStart(zoom); const call = takeAutomatedStart()!;
    const realNow = Date.now;
    try {
      Date.now = () => call.requestedAt! + 61_000;
      assert.equal(reservationCurrent(call, 'inflight'), false); assert.equal(beginAutomatedRecording(call), false);
      assert.equal(reservationCurrent({ ...call, requestedAt: Date.now() }, 'inflight'), false);
      Date.now = () => call.requestedAt! - 1;
      assert.equal(reservationCurrent(call, 'inflight'), false);
    } finally { Date.now = realNow; releaseAutomatedStart(call); }
  });
  test('expired pending is discarded; storage failure never dispatches or promotes', () => {
    markAutomatedStart(zoom); const pending = automatedStartInProgress()!;
    const realNow = Date.now;
    try { Date.now = () => pending.requestedAt! + 61_000; assert.equal(takeAutomatedStart(), null); }
    finally { Date.now = realNow; }
    assert.equal(automatedStartInProgress(), null);
    const original = sessionStorage.setItem;
    sessionStorage.setItem = () => { throw new Error('blocked'); };
    try { assert.equal(markAutomatedStart(zoom), false); assert.equal(takeAutomatedStart(), null); }
    finally { sessionStorage.setItem = original; }
  });
  test('legacy active display is readable but has no valid token authority', () => {
    store.set('labsAutoRecordingProcess', 'Teams.exe');
    assert.deepEqual(automatedRecording(), { app: 'Teams.exe', process: 'Teams.exe' });
    assert.equal(reservationCurrent(automatedRecording()!, 'recording'), false);
  });
  test('ending nothing announces nothing', () => { endAutomatedRecording(); assert.deepEqual(announced, []); });
});
