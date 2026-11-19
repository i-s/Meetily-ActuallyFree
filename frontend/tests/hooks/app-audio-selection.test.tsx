import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

let prefs: any;
const writes: any[] = [];
const errors: string[] = [];
Object.assign(globalThis, { window: new EventTarget() });
mock.module('@/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
mock.module('sonner', () => ({ toast: { error: (message: string) => errors.push(message) } }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  if (name === 'get_recording_preferences') return prefs;
  if (name === 'get_recordable_apps') return [];
  if (name === 'set_recording_preferences') { writes.push(args.preferences); prefs = args.preferences; return; }
  throw new Error(name);
} }));
const { useAppAudio } = await import('../../src/hooks/useAppAudio');
let choice: ReturnType<typeof useAppAudio>;
function Probe() { choice = useAppAudio(); return null; }
const app = (name: string) => ({ id: name, name, executable: name });

test('macOS picker rejects adding a second app and can repair an unsupported persisted list without losing other preferences', async () => {
  prefs = { save_folder: '/synthetic', auto_save: true, in_person: false, per_app_recording_enabled: true,
    per_app_target_app: 'A', per_app_target_name: 'A', per_app_targets: [app('A')] };
  let root: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { root = create(<Probe />); });
    await act(async () => { choice.addApp(app('B')); });
    expect(writes).toHaveLength(0);
    expect(errors[0]).toContain('one app');
    act(() => root!.unmount());
    prefs = { ...prefs, per_app_targets: [app('A'), app('B'), app('C')] };
    await act(async () => { root = create(<Probe />); });
    await act(async () => { choice.clearApps(); });
    expect(writes.at(-1).per_app_targets).toEqual([]);
    expect(writes.at(-1).per_app_target_app).toBeNull();
    await act(async () => { choice.addApp(app('D')); });
    expect(writes.at(-1).per_app_targets.map((a: any) => a.name)).toEqual(['D']);
    expect(writes.at(-1).auto_save).toBe(true);
    expect(writes.at(-1).in_person).toBe(false);
    expect(writes.at(-1).save_folder).toBe('/synthetic');
  } finally { act(() => root?.unmount()); }
});
