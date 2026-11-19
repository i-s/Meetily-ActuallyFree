import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const calls: { name: string; args: any }[] = [];
const noop = () => {};
let startGate: Promise<void> | undefined;
let releaseStart: (() => void) | undefined;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => { calls.push({ name, args }); if (name === 'start_audio_level_monitoring') await startGate; } }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@/components/ui/tooltip', () => ({ Tooltip: ({ children }: any) => children, TooltipTrigger: ({ children }: any) => children, TooltipContent: () => null }));
mock.module('@/components/ui/popover', () => ({ Popover: ({ children }: any) => children, PopoverTrigger: ({ children }: any) => children, PopoverContent: ({ children }: any) => children }));
mock.module('@/components/LiveAudioVisualizer', () => ({ LiveAudioVisualizer: () => null }));
mock.module('@/components/DeviceSelection', () => ({ AudioDeviceCard: () => null }));
const { RecordingVoiceLane } = await import('../../src/components/RecordingVoiceLane');
const props = { kind: 'mic' as const, open: true, onOpenChange: noop, savedValue: null, options: [], onSelect: noop, gain: 1, onGainLive: noop, onGainCommit: noop };
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

test('microphone chevron meter never implicitly requests default computer audio', async () => {
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingVoiceLane {...props} />); await flush(); });
  try { expect(calls.find(c => c.name === 'start_audio_level_monitoring')?.args).toEqual({ deviceNames: [], deviceType: 'input' }); }
  finally { await act(async () => { view.unmount(); await flush(); }); }
});

test('disabled source does not start preview capture; live excluded icon explains its limit', async () => {
  calls.length = 0;
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingVoiceLane {...props} muted />); await flush(); });
  expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(false);
  await act(async () => { view.update(<RecordingVoiceLane {...props} live captureIncluded={false} muted />); await flush(); });
  const icon = view.root.findAllByType('button').find(node => node.props['aria-pressed'] !== undefined)!;
  expect(icon.props.disabled).toBe(true);
  expect(icon.props['aria-label']).toContain('not captured');
  await act(async () => { view.unmount(); await flush(); });
});


test('open output preview stops on chosen-app transition and never restarts for changed or empty targets', async () => {
  calls.length = 0;
  let view!: ReturnType<typeof create>;
  const render = (onlyApps: boolean, targets: string[]) => <RecordingVoiceLane {...props} kind="output" previewScope={{ loaded: true, onlyApps, targets }} />;
  await act(async () => { view = create(render(false, [])); await flush(); });
  expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(true);
  calls.length = 0;
  await act(async () => { view.update(render(true, ['Zoom'])); await flush(); });
  expect(calls.some(c => c.name === 'stop_audio_level_monitoring')).toBe(true);
  expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(false);
  await act(async () => { view.update(render(true, ['Discord'])); await flush(); view.update(render(true, [])); await flush(); });
  expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(false);
  await act(async () => { view.update(render(false, [])); await flush(); });
  expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(true);
  await act(async () => { view.unmount(); await flush(); });
});


test('chosen-app transition cancels a pending native preview before its old reply arrives', async () => {
  calls.length = 0;
  startGate = new Promise<void>(resolve => { releaseStart = resolve; });
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<RecordingVoiceLane {...props} kind="output" previewScope={{ loaded: true, onlyApps: false, targets: [] }} />); await flush(); });
  try {
    expect(calls.some(c => c.name === 'start_audio_level_monitoring')).toBe(true);
    await act(async () => { view.update(<RecordingVoiceLane {...props} kind="output" previewScope={{ loaded: true, onlyApps: true, targets: ['Zoom'] }} />); await flush(); });
    expect(calls.some(c => c.name === 'stop_audio_level_monitoring')).toBe(true);
  } finally { await act(async () => { releaseStart!(); startGate = undefined; view.unmount(); await flush(); }); }
});
