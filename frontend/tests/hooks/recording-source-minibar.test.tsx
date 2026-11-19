import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const noop = () => {};
Object.assign(globalThis, { document: { documentElement: { classList: { add: noop, remove: noop } } }, window: { setInterval: () => 1, clearInterval: noop } });
let state: any;
let resolveSnapshot: ((value: any) => void) | undefined;
let snapshot: Promise<any> | undefined;
const changes = new Map<string, (value: any) => void>();
const commands: string[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async () => {} }));
mock.module('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ startDragging: async () => {} }) }));
mock.module('@/lib/app-theme', () => ({ useAppTheme: noop }));
mock.module('@/components/LiveAudioVisualizer', () => ({ LiveAudioVisualizer: (props: any) => <span data-source={props.source} data-active={props.active} /> }));
mock.module('@/services/recordingService', () => ({ recordingService: {
  getRecordingState: async () => snapshot ?? state,
  onMicrophoneMuteChanged: async (handler: any) => { changes.set('mic', handler); return noop; },
  onSystemAudioMuteChanged: async (handler: any) => { changes.set('system', handler); return noop; },
  setMicrophoneMuted: async (muted: boolean) => { commands.push('mic'); changes.get('mic')!({ muted }); },
  setSystemAudioMuted: async (muted: boolean) => { commands.push('system'); changes.get('system')!({ muted }); },
} }));
const { default: MiniBar } = await import('../../src/app/minibar/page');
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function native(mic: boolean, system: boolean) { return { microphone_captured: mic, system_audio_captured: system, is_paused: false, is_recording: true, is_microphone_muted: !mic, is_system_audio_muted: !system, active_duration: 3 }; }
function lane(view: ReturnType<typeof create>, source: string) { return view.root.findByProps({ 'data-source': source }).parent!.parent!; }

test('minibar excludes both source directions until first snapshot, then explains excluded instead of unmuting it', async () => {
  for (const [mic, system] of [[true, false], [false, true]]) {
    snapshot = new Promise(resolve => { resolveSnapshot = resolve; });
    let view!: ReturnType<typeof create>;
    await act(async () => { view = create(<MiniBar />); await flush(); });
    try {
      expect(view.root.findByProps({ 'data-source': 'mic' }).props['data-active']).toBe(false);
      expect(view.root.findByProps({ 'data-source': 'system' }).props['data-active']).toBe(false);
      await act(async () => { resolveSnapshot!(native(mic!, system!)); await flush(); });
      const excluded = mic ? 'system' : 'mic';
      const button = lane(view, excluded);
      expect(button.props.disabled).toBe(true);
      expect(button.props['aria-label']).toContain('not captured');
      expect(button.props.title).toContain('Stop');
      expect(view.root.findByProps({ 'data-source': excluded }).props['data-active']).toBe(false);
      commands.length = 0;
      await act(async () => { button.props.onClick(); await flush(); });
      expect(commands).toHaveLength(0);
    } finally { snapshot = undefined; act(() => view.unmount()); }
  }
});

test('included minibar lanes retain normal mute/unmute and meter behavior after reload', async () => {
  state = native(true, true);snapshot = undefined;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<MiniBar />); await flush(); });
  try { for (const source of ['mic', 'system']) {
    expect(view.root.findByProps({ 'data-source': source }).props['data-active']).toBe(true);
    await act(async () => { lane(view, source).props.onClick(); await flush(); });
    expect(view.root.findByProps({ 'data-source': source }).props['data-active']).toBe(false);
    await act(async () => { lane(view, source).props.onClick(); await flush(); });
    expect(view.root.findByProps({ 'data-source': source }).props['data-active']).toBe(true);
  }} finally { act(() => view.unmount()); }
});
