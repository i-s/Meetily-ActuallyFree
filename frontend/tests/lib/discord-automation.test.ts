import { commitAutomatedFixture as beginAutomatedRecording } from '../helpers/automated-recording';
import { beforeEach, describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createDiscordAutomation, discordMayStart, discordProcess,
  type DiscordCallStatus,
} from '../../src/lib/discord-automation';
import { automatedRecording, endAutomatedRecording, takeAutomatedStart, markAutomatedStart } from '../../src/lib/meeting-automation';

const store = new Map<string, string>();
Object.assign(globalThis, {
  sessionStorage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k) },
  window: { dispatchEvent: () => {} },
});
const active: DiscordCallStatus = { enabled: true, qualified: true, state: 'active', reason: 'test_fixture_not_mac_qualification', session_id: 'epoch:1', transition: 'start' };
const ended: DiscordCallStatus = { ...active, state: 'inactive', transition: 'end' };
function seed(call: { app: string; process: string }) {
  store.clear(); // Seed an independent externally committed native-owner fixture, not admission/retry.
  const tagged = call.process.startsWith('discord-macos:') ? { ...call, source: 'discord', sessionId: call.process.slice('discord-macos:'.length) } : { ...call, source: 'zoom', sessionId: call.process };
  assert.equal(markAutomatedStart(tagged), true);
  const reserved = takeAutomatedStart()!;
  assert.equal(beginAutomatedRecording(reserved), true);
  return reserved;
}
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

function fixture() {
  let current = { ...active };
  let recording = false;
  let allowed = true;
  let starts = 0;
  let stops = 0;
  const errors: unknown[] = [];
  const bridge = createDiscordAutomation({
    allowed: () => allowed, status: async () => current,
    recording: async () => {
      const owner = automatedRecording();
      return { is_recording: recording, recording_lease: owner ? { generation: `fixture:${owner.token}`, owner: { source: owner.source!, session_id: owner.sessionId!, app: owner.app, process: owner.process } } : null };
    },
    start: () => { starts++; }, stop: () => { stops++; }, error: (e) => errors.push(e),
  });
  return { bridge, set current(v: DiscordCallStatus) { current = v; }, set recording(v: boolean) { recording = v; }, set allowed(v: boolean) { allowed = v; }, get starts() { return starts; }, get stops() { return stops; }, errors };
}

describe('Discord production automation bridge', () => {
  beforeEach(() => store.clear());
  test('unqualified production signals, Unknown and disabled never start capture', async () => {
    for (const status of [{ ...active, qualified: false }, { ...active, state: 'unknown' as const }, { ...active, enabled: false }]) {
      const f = fixture(); f.current = status; f.bridge.update(status); await tick();
      assert.equal(f.starts, 0); assert.equal(takeAutomatedStart(), null);
      assert.equal(discordMayStart(status, discordProcess('epoch:1')), false);
    }
  });
  test('qualified fixture goes through ordinary pending recorder handoff once', async () => {
    const f = fixture(); f.bridge.update(active); await tick();
    assert.equal(f.starts, 1);
    const call = takeAutomatedStart();
    assert.equal(call?.process, discordProcess('epoch:1'));
    assert.equal(call?.source, 'discord');
    assert.equal(call?.sessionId, 'epoch:1');
    assert.ok(call?.token);
    assert.equal(store.has('discordAutomationRequest'), false);
    beginAutomatedRecording(call!); f.recording = true;
    f.bridge.update({ ...active, transition: 'none' }); await tick();
    assert.equal(f.starts, 1); assert.equal(f.stops, 0);
    // Manual Stop / Don't stop it relinquishes ownership without rearming.
    endAutomatedRecording(); f.recording = false;
    f.bridge.dispose();
    const reloaded = fixture(); reloaded.bridge.update(active); await tick();
    assert.equal(reloaded.starts, 0);
  });
  test('unknown, mute and reconnect do not stop; confirmed end stops only its owner once', async () => {
    const f = fixture();
    seed({ app: 'Discord', process: discordProcess('epoch:1') }); f.recording = true;
    f.bridge.update({ ...active, state: 'unknown', reason: 'reconnecting', transition: 'none' }); await tick();
    assert.equal(f.stops, 0);
    f.bridge.update(active); await tick(); assert.equal(f.stops, 0);
    f.current = ended; f.bridge.update(ended); await tick(); f.bridge.recordingChanged(); await tick();
    assert.equal(f.stops, 1);
    assert.equal(automatedRecording()?.app, 'Discord'); // Recorder clears ownership during actual Stop.
  });
  test('manual recordings, Zoom owner and another Discord session are never stopped', async () => {
    for (const owner of [null, { app: 'Zoom', process: 'zoom' }, { app: 'Discord', process: discordProcess('epoch:2') }]) {
      store.clear(); const f = fixture(); f.recording = true;
      if (owner) seed(owner);
      f.current = ended; f.bridge.update(ended); await tick(); assert.equal(f.stops, 0);
    }
  });
  test('end while start is loading reconciles after recorder accepts ownership', async () => {
    const f = fixture(); f.bridge.update(active); await tick();
    const call = takeAutomatedStart()!;
    f.current = ended; f.bridge.update(ended); await tick(); assert.equal(f.stops, 0);
    beginAutomatedRecording(call); f.recording = true; f.bridge.recordingChanged(); await tick();
    assert.equal(f.stops, 1);
  });
  test('fresh native snapshot rejects stale join, and disable does not stop', async () => {
    const f = fixture(); f.current = ended; f.bridge.update(active); await tick(); assert.equal(f.starts, 0);
    seed({ app: 'Discord', process: discordProcess('epoch:1') }); f.recording = true;
    f.allowed = false; f.current = ended; f.bridge.update(ended); await tick(); assert.equal(f.stops, 0);
  });
  test('a stale end cannot stop after native observation was disabled or call returned', async () => {
    const f = fixture(); f.recording = true;
    seed({ app: 'Discord', process: discordProcess('epoch:1') });
    f.current = { ...ended, enabled: false };
    f.bridge.update({ ...ended }); await tick(); assert.equal(f.stops, 0);
    f.current = active;
    f.bridge.update({ ...ended }); await tick(); assert.equal(f.stops, 0);
  });
  test('new session can start after manual Stop, but no storage fails closed', async () => {
    const f = fixture(); f.bridge.update(active); await tick();
    endAutomatedRecording();
    const next = { ...active, session_id: 'epoch:2' }; f.current = next; f.bridge.update(next); await tick();
    assert.equal(f.starts, 2);
    store.clear(); const original = sessionStorage.setItem;
    sessionStorage.setItem = () => { throw new Error('blocked storage'); };
    try { const g = fixture(); g.bridge.update(active); await tick(); assert.equal(g.starts, 0); assert.equal(g.errors.length, 0); }
    finally { sessionStorage.setItem = original; }
  });
});
