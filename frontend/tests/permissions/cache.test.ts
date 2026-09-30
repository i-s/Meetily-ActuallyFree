import { beforeEach, expect, test } from 'bun:test';
import { applySystemAudioProbe, readSystemAudioPermission, MACOS_SYSTEM_AUDIO_VERIFIED_KEY } from '../../src/lib/system-audio-permission';
const storage = new Map<string, string>();
Object.defineProperty(globalThis, 'window', { configurable: true, value: { sessionStorage: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
} } });
beforeEach(() => storage.clear());

test('legacy booleans and malformed cache never prove denial', () => {
  for (const value of ['false', 'true', '{', '{"version":1,"status":"denied"}', '{"version":2,"status":"unknown"}']) {
    storage.set('macos_system_audio_verified', value);
    storage.set(MACOS_SYSTEM_AUDIO_VERIFIED_KEY, value);
    expect(readSystemAudioPermission()).toBe('unknown');
  }
});

test('unknown IPC evidence is not cached and never downgrades verification', () => {
  expect(applySystemAudioProbe({ detected: false, conclusive: false, reason: 'no_audio_detected' })).toBe('unknown');
  expect(storage.size).toBe(0);
  applySystemAudioProbe({ detected: true, conclusive: true, reason: 'audio_detected' });
  const verified = storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY);
  for (const result of [false, true, null, {}, { detected: true, conclusive: false }, { detected: false, conclusive: false }]) {
    expect(applySystemAudioProbe(result as any)).toBe('verified');
    expect(storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(verified);
  }
});

test('only conclusive denial replaces verified evidence', () => {
  applySystemAudioProbe({ detected: true, conclusive: true, reason: 'audio_detected' });
  expect(applySystemAudioProbe({ detected: false, conclusive: true, reason: 'permission_denied' })).toBe('denied');
  expect(readSystemAudioPermission()).toBe('denied');
});
