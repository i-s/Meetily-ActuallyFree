import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, test } from 'bun:test';
let cleanup: ((event: { payload: string[] | null }) => void) | undefined;
const focus = new Set<() => void>();
let exists = true, revision = 'original', calls = 0, deferNext = false;
let resolveHeld: ((value: any) => void) | undefined;
mock.module('@tauri-apps/api/event', () => ({ listen: async (_name: string, fn: any) => { cleanup = fn; return () => { if (cleanup === fn) cleanup = undefined; }; } }));
mock.module('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }));
mock.module('@/lib/workspace-api', () => ({ getMeetingAudio: async (meeting: string) => {
  calls++;
  if (deferNext) { deferNext = false; return new Promise(resolve => { resolveHeld = resolve; }); }
  return { path: exists ? `/generated/${meeting}/audio.mp4` : null, playbackRevision: revision, micPath: null, systemPath: null };
} }));
const elements: FakeAudio[] = [];
class FakeAudio {
  src = ''; preload = ''; duration = 42; currentTime = 0; paused = true; playbackRate = 1;
  onloadedmetadata?: (() => void) | null; onplay?: (() => void) | null; onpause?: (() => void) | null;
  ontimeupdate?: (() => void) | null; onended?: (() => void) | null; onerror?: (() => void) | null;
  pause() { this.paused = true; this.onpause?.(); }
  removeAttribute() { this.src = ''; } load() {}
  play() { this.paused = false; this.onplay?.(); return Promise.resolve(); }
  constructor() { elements.push(this); }
}
Object.assign(globalThis, { Audio: FakeAudio, window: {
  addEventListener(name: string, fn: () => void) { if (name === 'focus') focus.add(fn); },
  removeEventListener(name: string, fn: () => void) { if (name === 'focus') focus.delete(fn); },
}, requestAnimationFrame: () => 1, cancelAnimationFrame() {} });
const { useMeetingAudio } = await import('../../src/hooks/useMeetingAudio');
let controls: ReturnType<typeof useMeetingAudio>;
function Harness({ meeting = 'generated' }: { meeting?: string }) { controls = useMeetingAudio(meeting); return <p>{controls.status}</p>; }
beforeEach(() => { exists = true; revision = 'original'; calls = 0; deferNext = false; resolveHeld = undefined; elements.length = 0; });
const notifyFocus = () => { for (const fn of focus) fn(); };
async function playingAt23() {
  await act(async () => { elements.at(-1)!.onloadedmetadata?.(); controls.setRate(1.5); controls.seek(23); controls.play(); });
}

test('unchanged focus preserves a playing element at 23 seconds and 1.5x; unrelated cleanup does not recheck it', async () => {
  let root: any; await act(async () => { root = create(<Harness />); }); await playingAt23();
  const original = elements[0]; const source = original.src;
  await act(async () => notifyFocus());
  expect(elements).toHaveLength(1); expect(original.src).toBe(source); expect(original.currentTime).toBe(23);
  expect(original.paused).toBe(false); expect(original.playbackRate).toBe(1.5);
  expect(controls.currentTime).toBe(23); expect(controls.playing).toBe(true); expect(controls.rate).toBe(1.5);
  const before = calls;
  await act(async () => cleanup?.({ payload: ['unrelated-meeting'] }));
  expect(calls).toBe(before); expect(elements).toHaveLength(1);
  await act(async () => root.unmount()); expect(cleanup).toBeUndefined(); expect(focus.size).toBe(0);
});

test('same-path replacement refreshes the affected player with selected rate; removed audio becomes unavailable', async () => {
  let root: any; await act(async () => { root = create(<Harness />); }); await playingAt23();
  const old = elements[0]; revision = 'replacement';
  await act(async () => cleanup?.({ payload: ['generated'] }));
  expect(elements).toHaveLength(2); expect(old.src).toBe(''); expect(old.paused).toBe(true);
  expect(elements[1].playbackRate).toBe(1.5); expect(controls.rate).toBe(1.5); expect(controls.playing).toBe(false); expect(controls.currentTime).toBe(0);
  await act(async () => elements[1].onloadedmetadata?.()); expect(controls.status).toBe('ready');
  exists = false; await act(async () => cleanup?.({ payload: ['generated'] }));
  expect(controls.status).toBe('unavailable'); expect(controls.path).toBe(null); expect(elements[1].src).toBe('');
  await act(async () => root.unmount());
});

test('late checks and old element callbacks cannot replace a newer observation or another meeting', async () => {
  let root: any; await act(async () => { root = create(<Harness meeting="a" />); }); await playingAt23();
  const oldMetadata = elements[0].onloadedmetadata!;
  deferNext = true; await act(async () => notifyFocus()); const first = resolveHeld!;
  revision = 'newer'; await act(async () => notifyFocus()); expect(elements).toHaveLength(2);
  await act(async () => first({ path: null })); expect(controls.path).toBe('/generated/a/audio.mp4');
  deferNext = true; await act(async () => notifyFocus()); const abandoned = resolveHeld!;
  await act(async () => root.update(<Harness meeting="b" />));
  const b = elements.at(-1)!; await act(async () => b.onloadedmetadata?.());
  await act(async () => { abandoned({ path: '/generated/a/audio.mp4', playbackRevision: 'obsolete' }); oldMetadata(); });
  expect(controls.path).toBe('/generated/b/audio.mp4'); expect(controls.status).toBe('ready'); expect(b.playbackRate).toBe(1.5);
  await act(async () => root.unmount()); expect(cleanup).toBeUndefined();
});
