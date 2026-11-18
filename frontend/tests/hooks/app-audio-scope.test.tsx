import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
mock.module('@/hooks/usePlatform', () => ({ usePlatform: () => 'macos' }));
mock.module('@/hooks/useAppAudio', () => ({ useAppAudio: () => ({}) }));
const { AppAudioSource } = await import('../../src/components/recording/AppAudioSource');
const target = (id: string) => ({ id, name: id, executable: id });
function choice(targets: any[]) { return { loaded: true, onlyApps: true, targets, running: [], scanning: false, statusOf: () => 'closed', setOnlyApps: () => {}, removeApp: () => {}, addApp: () => {}, clearApps: () => {}, scan: async () => {}, browse: async () => {} } as any; }

test('macOS warns on persisted multi-selection before Start and never promises first-app or global fallback', async () => {
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<AppAudioSource choice={choice([target('A'), target('B')])} />); });
    const text = JSON.stringify(view!.toJSON());
    expect(text).toContain('one app');
    expect(text).not.toContain('first chosen app');
    expect(view!.root.findAllByType('button').find(b => b.children.includes('Add app'))?.props.disabled).toBe(true);
    await act(async () => { view!.update(<AppAudioSource choice={choice([])} />); });
    expect(JSON.stringify(view!.toJSON())).toContain("can't start");
    expect(JSON.stringify(view!.toJSON())).not.toContain('all computer audio is recorded');
  } finally { act(() => view?.unmount()); }
});
