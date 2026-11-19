import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const calls: { name: string; args: any }[] = [];
const noop = () => {};
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => { calls.push({ name, args }); } }));
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
