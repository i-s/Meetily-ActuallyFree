import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const bus = new EventTarget();
Object.assign(globalThis, { window: bus });
let prefs = { microphone_enabled: true, system_audio_enabled: false };
let release: (() => void) | undefined;
let gate: Promise<void> | undefined;
let fail = false;
let readFailure = false;
let readGate: Promise<void> | undefined;
let readRelease: (() => void) | undefined;
let replyGate: Promise<void> | undefined;
type Selection = typeof prefs;
const subscribers = new Set<(event: { payload: Selection }) => void>();
function publish(value: Selection) {
  subscribers.forEach(handler => handler({ payload: value }));
}
const calls: any[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push({ name, args });
  if (name === 'get_recording_preferences') { const captured = { ...prefs }; await readGate; if (readFailure) throw new Error('Temporary read failure'); return captured; }
  if (name === 'set_recording_source_enabled') {
    await gate;
    if (fail) throw new Error('Synthetic disk write failure');
    prefs = { ...prefs, [args.source === 'system' ? 'system_audio_enabled' : 'microphone_enabled']: args.enabled };
    const result = { ...prefs };
    await replyGate;
    return result;
  }
  throw new Error(name);
} }));
mock.module('@tauri-apps/api/event', () => ({
  listen: async (_event: string, handler: (event: { payload: Selection }) => void) => {
    subscribers.add(handler);
    return () => subscribers.delete(handler);
  },
}));
mock.module('sonner', () => ({ toast: { error: () => {} } }));
const { useRecordingSources } = await import('../../src/hooks/useRecordingSources');
const { flushRecordingSources } = await import('../../src/lib/recording-sources');
let current: ReturnType<typeof useRecordingSources>;
function Probe() { current = useRecordingSources(); return <span>{current.sources?.system_audio_enabled ? 'on' : 'off'}</span>; }
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

test('upgrade source stays off across mount, explicit click persists before Start, then survives remount', async () => {
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<Probe />); await flush(); });
  expect(current!.sources?.system_audio_enabled).toBe(false);
  gate = new Promise<void>(r => { release = r; });
  let startReady = false;
  await act(async () => { current!.setEnabled('system', true); await flush(); });
  const pendingStart = flushRecordingSources().then(() => { startReady = true; });
  await flush(); expect(startReady).toBe(false);
  expect(current!.saving).toBe(true);
  await act(async () => { release!(); await pendingStart; await flush(); });
  expect(current!.sources?.system_audio_enabled).toBe(true);
  expect(prefs.microphone_enabled).toBe(true);
  expect(calls.some(c => c.name === 'set_recording_preferences')).toBe(false);
  act(() => view.unmount());
  gate = undefined;
  await act(async () => { view = create(<Probe />); await flush(); });
  expect(current!.sources?.system_audio_enabled).toBe(true);
  act(() => view.unmount());
});

test('failed write leaves confirmed source off and fences immediate Start until a successful retry', async () => {
  prefs = { microphone_enabled: true, system_audio_enabled: false }; fail = true;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<Probe />); await flush(); });
  await act(async () => { current!.setEnabled('system', true); await flush(); });
  expect(current!.sources?.system_audio_enabled).toBe(false);
  await expect(flushRecordingSources()).rejects.toThrow('Synthetic disk');
  fail = false;
  await act(async () => { current!.setEnabled('system', true); await flushRecordingSources(); await flush(); });
  expect(current!.sources?.system_audio_enabled).toBe(true);
  act(() => view.unmount());
});


test('temporary initial read error can be retried in-place and late read cannot replace fresh event', async () => {
  readFailure = true;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<Probe />); await flush(); });
  try {
    expect(current!.sources).toBeNull();
    expect(current!.error).toContain('Temporary read');
    readFailure = false;
    await act(async () => { await current!.retry(); await flush(); });
    expect(current!.sources?.system_audio_enabled).toBe(true);
    expect(current!.error).toBeNull();
    readGate = new Promise<void>(resolve => { readRelease = resolve; });
    await act(async () => { void current!.retry(); await flush(); });
    readGate = undefined; // Only the first read is held.
    prefs = { microphone_enabled: true, system_audio_enabled: false };
    await act(async () => { publish(prefs); await flush(); });
    await act(async () => { readRelease!(); await flush(); });
    expect(current!.sources?.system_audio_enabled).toBe(false);
    expect(current!.error).toBeNull();
  } finally { readFailure = false; readGate = undefined; act(() => view.unmount()); }
});

test.each([true, false])('a delayed reply or event cannot reverse the other window’s system audio choice (%s)', async enabled => {
  prefs = { microphone_enabled: true, system_audio_enabled: enabled };
  let view!: ReturnType<typeof create>;
  let acknowledge!: () => void;
  replyGate = new Promise<void>(resolve => { acknowledge = resolve; });
  await act(async () => { view = create(<Probe />); await flush(); });
  try {
    await act(async () => { current.setEnabled('system', !enabled); await flush(); });
    const firstCommit = { ...prefs };
    expect(firstCommit.system_audio_enabled).toBe(!enabled);
    await act(async () => { publish(firstCommit); await flush(); });
    // B commits after A. A's setter reply is still in transit.
    prefs = { ...prefs, system_audio_enabled: enabled };
    await act(async () => { publish(prefs); await flush(); });
    expect(current.sources).toEqual(prefs);
    await act(async () => { acknowledge(); await flushRecordingSources(); await flush(); });
    expect(current.sources).toEqual(prefs);
    await act(async () => { publish(firstCommit); await flush(); });
    expect(current.sources).toEqual(prefs);
  } finally { acknowledge(); replyGate = undefined; act(() => view.unmount()); }
});
