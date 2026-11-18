import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, spyOn, test } from 'bun:test';

const calls: string[] = [];
const saved: unknown[][] = [];
const memory = new Map<string, string>();
const session = { getItem: (key: string) => memory.get(key) ?? null, setItem: (key: string, value: string) => memory.set(key, value), removeItem: (key: string) => memory.delete(key) };
Object.assign(globalThis, { sessionStorage: session, window: {} });
const noop = () => {};
mock.module('next/navigation', () => ({ useRouter: () => ({ push: noop }) }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string) => { calls.push(name); } }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ transcriptsRef: { current: [] }, flushBuffer: noop, clearTranscripts: noop, meetingTitle: 'Short meeting', markMeetingAsSaved: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ refetchMeetings: noop, setCurrentMeeting: noop, setMeetings: noop, meetings: [], setIsMeetingActive: noop }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { IDLE: 'idle', STOPPING: 'stopping', PROCESSING_TRANSCRIPTS: 'processing', SAVING: 'saving', ERROR: 'error' }, useRecordingState: () => ({ recordingDuration: 3, setStatus: noop }) }));
mock.module('@/services/storageService', () => ({ storageService: { saveMeeting: async (...args: unknown[]) => { saved.push(args); throw new Error('synthetic save failure; retain recovery'); } } }));
mock.module('@/services/transcriptService', () => ({ transcriptService: { getTranscriptionStatus: async () => ({ is_processing: false, chunks_in_queue: 0 }) } }));
mock.module('@/lib/analytics', () => ({ default: { track: async () => {}, trackButtonClick: noop } }));
mock.module('@/lib/summary-language-preferences', () => ({ applyPinnedSummaryLanguageToMeeting: noop, detectAndCacheSummaryLanguage: noop }));
mock.module('@/lib/live-session', () => ({ endLiveSession: noop, finalLiveTitle: (title: string) => title, readLiveNotes: () => null, writeLiveTitle: noop }));
mock.module('@/lib/workspace-api', () => ({ announceChange: noop, saveMeetingNotes: noop }));
mock.module('@/lib/meeting-automation', () => ({ endAutomatedRecording: noop }));
mock.module('sonner', () => ({ toast: { error: noop, warning: noop, message: noop } }));
const { useRecordingStop } = await import('../../src/hooks/useRecordingStop');
let stop: (save: boolean) => Promise<void>;
function Probe() { stop = useRecordingStop(noop, noop).handleRecordingStop; return null; }

test('manual stop keeps even a short, empty-transcript recording and retains recovery on save failure', async () => {
  memory.set('last_recording_folder_path', '/synthetic/short-meeting');
  const timer = spyOn(globalThis, 'setTimeout').mockImplementation(((callback: () => void) => { queueMicrotask(callback); return 1; }) as any);
  let root: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { root = create(<Probe />); });
    await act(async () => { await stop(true); });
    expect(calls).not.toContain('discard_recording_folder');
    expect(saved).toHaveLength(1);
    expect(saved[0][2]).toBe('/synthetic/short-meeting');
    expect(memory.get('last_recording_folder_path')).toBe('/synthetic/short-meeting');
  } finally {
    act(() => root?.unmount());
    timer.mockRestore();
  }
});
