import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, spyOn, test } from 'bun:test';

const handlers = new Map<string, (payload: any) => void>();
let backend = { is_recording: true, is_paused: false, is_microphone_muted: false, is_system_audio_muted: false, is_active: true, recording_duration: 37 as number | null, active_duration: 35 as number | null };
const subscribe = (name: string) => async (callback: (payload: any) => void) => {
  handlers.set(name, callback);
  return () => { handlers.delete(name); };
};
mock.module('../../src/services/recordingService', () => ({ recordingService: {
  getRecordingState: async () => backend,
  onRecordingStarted: subscribe('started'), onRecordingStopped: subscribe('stopped'),
  onRecordingPaused: subscribe('paused'), onRecordingResumed: subscribe('resumed'),
  onMicrophoneMuteChanged: subscribe('mic'), onSystemAudioMuteChanged: subscribe('system'),
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async (name: string, callback: (event: any) => void) => {
  handlers.set(name, callback);
  return () => { handlers.delete(name); };
} }));
const { RecordingStateProvider, useRecordingState } = await import('../../src/contexts/RecordingStateContext');

function Status() {
  const state = useRecordingState();
  return <output>{JSON.stringify({ duration: state.activeDuration, message: state.statusMessage, stopping: state.isStopping, recording: state.isRecording })}</output>;
}

test('shutdown stage stays visible and duration survives detached native manager until completion', async () => {
  let poll: (() => Promise<void>) | undefined;
  const timer = spyOn(globalThis, 'setInterval').mockImplementation(((callback: () => Promise<void>) => { poll = callback; return 1; }) as any);
  const clear = spyOn(globalThis, 'clearInterval').mockImplementation(() => {});
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<RecordingStateProvider><Status /></RecordingStateProvider>); });
    const state = () => JSON.parse(view!.root.findByType('output').children.join(''));
    await act(async () => { handlers.get('started')!({}); });
    await act(async () => { handlers.get('recording-shutdown-progress')?.({ payload: { stage: 'finalizing', message: 'Saving recording audio...' } }); });
    backend = { ...backend, is_active: false, recording_duration: null, active_duration: null };
    await act(async () => { await poll!(); });
    expect(state()).toEqual({ duration: 35, message: 'Saving recording audio...', stopping: true, recording: true });
    await act(async () => { handlers.get('stopped')!({}); });
    expect(state().recording).toBe(false);
    act(() => { view!.unmount(); });
    expect(handlers.has('recording-shutdown-progress')).toBe(false);
  } finally {
    if (view) act(() => view!.unmount());
    timer.mockRestore(); clear.mockRestore();
  }
});
