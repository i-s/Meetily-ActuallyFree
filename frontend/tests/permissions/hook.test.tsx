import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
Object.defineProperty(globalThis, 'window', { value: { sessionStorage: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
} }, configurable: true });
let probe: unknown = { detected: false, conclusive: false, reason: 'no_audio_detected' };
let devicesError = false;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string) => {
  if (command === 'get_audio_devices') {
    if (devicesError) throw new Error('device enumeration failed');
    return [{ name: 'Mic', device_type: 'Input' }, { name: 'Speakers', device_type: 'Output' }];
  }
  if (command === 'trigger_system_audio_permission_command') {
    if (probe instanceof Error) throw probe;
    return probe;
  }
  return true;
} }));
mock.module('../../src/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
const { usePermissionCheck, MACOS_SYSTEM_AUDIO_VERIFIED_KEY } = await import('../../src/hooks/usePermissionCheck');
let status: ReturnType<typeof usePermissionCheck>;
function Harness() { status = usePermissionCheck(); return null; }

test('silence, errors, legacy cache, verification and explicit denial remain distinct', async () => {
  storage.set('macos_system_audio_verified', 'false');
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<Harness />); });
  try {
    expect(status!.systemAudio).toBe('unknown');
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('unknown');
    expect(storage.has(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(false);

    probe = new Error('probe unavailable');
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('unknown');
    expect(storage.has(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(false);

    probe = { detected: true, conclusive: true, reason: 'audio_detected' };
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('verified');
    const verifiedCache = storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY);
    expect(verifiedCache).toBeDefined();

    probe = { detected: false, conclusive: false, reason: 'no_audio_detected' };
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('verified');
    expect(storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(verifiedCache);
    probe = new Error('probe unavailable');
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('verified');
    devicesError = true;
    await act(async () => { await status!.checkPermissions(); });
    expect(status!.systemAudio).toBe('verified');
    devicesError = false;

    probe = { detected: false, conclusive: true, reason: 'permission_denied' };
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('denied');
    probe = { detected: false, conclusive: false, reason: 'no_audio_detected' };
    await act(async () => { await status!.requestPermissions(); });
    expect(status!.systemAudio).toBe('denied');
  } finally { act(() => view!.unmount()); }
}, 15000);
