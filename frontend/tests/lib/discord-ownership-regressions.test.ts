import { beforeEach, test } from 'node:test';
import assert from 'node:assert/strict';
import { createDiscordAutomation, type DiscordCallStatus } from '../../src/lib/discord-automation';
import { markAutomatedStart, takeAutomatedStart, beginAutomatedRecording, endAutomatedRecording, automatedRecording } from '../../src/lib/meeting-automation';
const store = new Map<string, string>();
Object.assign(globalThis, {
  sessionStorage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k) },
  window: { dispatchEvent: () => {} },
});
const active: DiscordCallStatus = { enabled: true, qualified: true, state: 'active', reason: 'synthetic', session_id: 'one', transition: 'start' };
const call = { app: 'Discord', process: 'discord-macos:one', source: 'discord', sessionId: 'one' };
const zoom = { app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'z1' };
const tick = () => new Promise((r) => setTimeout(r, 0));
function seed() {
  endAutomatedRecording(); markAutomatedStart(call);
  const owner = takeAutomatedStart()!; beginAutomatedRecording(owner); return owner;
}
beforeEach(() => store.clear());
function fixture() {
  let recording = false, starts = 0, stops = 0;
  let status = active;
  const bridge = createDiscordAutomation({ allowed: () => true, status: async () => status,
    recording: async () => ({ is_recording: recording }), start: () => { starts++; }, stop: () => { stops++; }, error: () => {} });
  return { bridge, get starts() { return starts; }, get stops() { return stops; }, set recording(v: boolean) { recording = v; }, set status(v: DiscordCallStatus) { status = v; } };
}
test('R3 busy manual take consumes Discord session before await and stays suppressed after Stop/reload', async () => {
  const f = fixture(); f.recording = true; f.bridge.update(active); await tick();
  f.recording = false; f.bridge.update({ ...active }); await tick(); assert.equal(f.starts, 0);
  f.bridge.dispose(); const reloaded = fixture(); reloaded.bridge.update(active); await tick(); assert.equal(reloaded.starts, 0);
  const next = { ...active, session_id: 'two' }; reloaded.status = next; reloaded.bridge.update(next); await tick(); assert.equal(reloaded.starts, 1);
});
test('R4 pending Zoom rejects Discord dispatch and private override', async () => {
  markAutomatedStart(zoom);
  const f = fixture(); f.bridge.update(active); await tick();
  assert.equal(f.starts, 0); assert.equal(takeAutomatedStart()?.process, zoom.process);
  assert.equal(store.has('discordAutomationRequest'), false);
});
test('R4 Discord accepted first cannot be overwritten by pending Zoom', async () => {
  const f = fixture(); f.bridge.update(active); await tick();
  assert.equal(f.starts, 1);
  const accepted = markAutomatedStart(zoom);
  assert.equal(takeAutomatedStart()?.process, call.process);
  assert.equal(accepted, false);
});
test('R4 after asynchronous admission replacement Zoom reservation wins without stale Discord dispatch', async () => {
  let resolve!: (value: { is_recording: boolean }) => void;
  let starts = 0;
  const bridge = createDiscordAutomation({ allowed: () => true, status: async () => active,
    recording: () => new Promise((r) => { resolve = r; }), start: () => { starts++; }, stop: () => {}, error: () => {} });
  bridge.update(active); await tick(); endAutomatedRecording(); markAutomatedStart(zoom); resolve({ is_recording: false }); await tick();
  assert.equal(starts, 0); assert.equal(takeAutomatedStart()?.process, zoom.process);
});
test('R4 same-process replacement while stop state await is pending cannot stop replacement take', async () => {
  seed(); let resolve!: (value: any) => void; let stops = 0;
  const ended = { ...active, state: 'inactive' as const, transition: 'end' as const };
  const bridge = createDiscordAutomation({ allowed: () => true, status: async () => ended,
    recording: () => new Promise((r) => { resolve = r; }), start: () => {}, stop: () => { stops++; }, error: () => {} });
  bridge.update(ended); await tick(); const b = seed();
  resolve({ is_recording: true, recording_lease: { generation: 'A', owner: { source: 'discord', session_id: 'one', app: 'Discord', process: call.process } } });
  await tick(); assert.equal(stops, 0); assert.equal(automatedRecording()?.token, b.token);
});
test('R4 native generation replacement is rechecked even when local owner has not refreshed', async () => {
  seed(); let reads = 0, stops = 0;
  const ended = { ...active, state: 'inactive' as const, transition: 'end' as const };
  const lease = { generation: 'A', owner: { source: 'discord', session_id: 'one', app: 'Discord', process: call.process } };
  const bridge = createDiscordAutomation({ allowed: () => true, status: async () => ended,
    recording: async () => ({ is_recording: true, recording_lease: ++reads === 1 ? lease : { ...lease, generation: 'B', owner: null } }),
    start: () => {}, stop: () => { stops++; }, error: () => {} });
  bridge.update(ended); await tick(); assert.equal(reads, 2); assert.equal(stops, 0);
});
test('R4 disable during final status await leaves owner and native recording untouched', async () => {
  const owner = seed(); let allowed = true, stops = 0;
  let resolve!: (value: DiscordCallStatus) => void;
  const ended = { ...active, state: 'inactive' as const, transition: 'end' as const };
  const lease = { generation: 'A', owner: { source: 'discord', session_id: 'one', app: 'Discord', process: call.process } };
  const bridge = createDiscordAutomation({ allowed: () => allowed, status: () => new Promise((r) => { resolve = r; }),
    recording: async () => ({ is_recording: true, recording_lease: lease }),
    start: () => {}, stop: () => { stops++; }, error: () => {} });
  bridge.update(ended); await tick(); allowed = false; resolve(ended); await tick();
  assert.equal(stops, 0); assert.equal(automatedRecording()?.token, owner.token);
});
test('R4 missing native generation fails closed even with a locally matching owner', async () => {
  seed(); const f = fixture(); f.recording = true;
  const ended = { ...active, state: 'inactive' as const, transition: 'end' as const }; f.status = ended; f.bridge.update(ended); await tick();
  assert.equal(f.stops, 0);
});
