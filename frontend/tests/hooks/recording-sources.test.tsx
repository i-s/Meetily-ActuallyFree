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
const calls: any[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push({ name, args });
  if (name === 'get_recording_preferences') { const captured = { ...prefs }; await readGate; if (readFailure) throw new Error('Temporary read failure'); return captured; }
  if (name === 'set_recording_source_enabled') {
    await gate;
    if (fail) throw new Error('Synthetic disk write failure');
    prefs = { ...prefs, [args.source === 'system' ? 'system_audio_enabled' : 'microphone_enabled']: args.enabled };
    return { ...prefs };
  }
  throw new Error(name);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
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
    await act(async () => { bus.dispatchEvent(new CustomEvent('meetily-recording-sources-changed', { detail: { microphone_enabled: true, system_audio_enabled: false } })); });
    await act(async () => { readRelease!(); await flush(); });
    expect(current!.sources?.system_audio_enabled).toBe(false);
    expect(current!.error).toBeNull();
  } finally { readFailure = false; readGate = undefined; act(() => view.unmount()); }
});
