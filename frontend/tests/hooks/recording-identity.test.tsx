import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, spyOn, test } from 'bun:test';

const values = new Map<string, string>();
const storage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) };
Object.assign(globalThis, { sessionStorage: storage, window: { sessionStorage: storage } });
let currentId = 'meeting-first';
let flush: (id: string) => Promise<void> = async () => {};
const calls: Array<[string, any]> = [];
const routes: string[] = [];
const saved: string[] = [];
const deleted: string[] = [];
const noop = () => {};
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push([name, args]);
  if (name === 'api_delete_meeting') deleted.push(`native:${args.meetingId}`);
  if (name === 'api_save_transcript') return { meeting_id: args.meetingId };
  if (name === 'api_get_meeting') return { id: args.meetingId, title: 'Saved' };
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => noop }));
mock.module('@tauri-apps/plugin-store', () => ({ Store: { load: async () => ({ get: async () => 2 }) } }));
mock.module('next/navigation', () => ({ useRouter: () => ({ push: (path: string) => routes.push(path) }) }));
mock.module('sonner', () => ({ toast: { error: noop, warning: noop, message: noop } }));
mock.module('@/contexts/TranscriptContext', () => ({ useTranscripts: () => ({ transcriptsRef: { current: [] }, flushBuffer: noop, clearTranscripts: noop, meetingTitle: 'Silent', markMeetingAsSaved: async () => { saved.push(currentId); } }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ RecordingStatus: { IDLE: 'idle', STOPPING: 'stopping', PROCESSING_TRANSCRIPTS: 'processing', SAVING: 'saving', COMPLETED: 'completed', ERROR: 'error' }, useRecordingState: () => ({ meetingId: currentId, recordingDuration: 3, setStatus: noop }) }));
mock.module('@/components/Sidebar/SidebarProvider', () => ({ useSidebar: () => ({ refetchMeetings: async () => {}, setCurrentMeeting: noop, setMeetings: noop, meetings: [], setIsMeetingActive: noop }) }));
mock.module('@/services/transcriptService', () => ({ transcriptService: { getTranscriptionStatus: async () => ({ is_processing: false, chunks_in_queue: 0 }) } }));
mock.module('@/lib/meeting-notes-store', () => ({ flushMeetingNotes: (id: string) => flush(id), forgetMeetingNotes: (id: string) => { deleted.push(`draft:${id}`); } }));
mock.module('@/lib/summary-language-preferences', () => ({ applyPinnedSummaryLanguageToMeeting: async () => true, detectAndCacheSummaryLanguage: async () => {} }));
mock.module('@/lib/live-session', () => ({ endLiveSession: noop, readLiveNotes: () => null, writeLiveTitle: noop, finalLiveTitle: (fallback: string) => fallback }));
mock.module('@/lib/workspace-api', () => ({ announceChange: noop, saveMeetingNotes: async () => {} }));
mock.module('@/lib/meeting-automation', () => ({ endAutomatedRecording: noop }));
mock.module('@/lib/groups', () => ({ readPendingGroup: () => null }));
mock.module('@/lib/analytics', () => ({ default: { trackPageView: noop, trackMeetingCompleted: async () => {}, getMeetingsCountToday: async () => 1, updateMeetingCount: async () => {} } }));
mock.module('@/services/indexedDBService', () => ({ indexedDBService: {
  deleteMeeting: async (id: string) => { deleted.push(`recovery:${id}`); },
  getMeetingMetadata: async (id: string) => ({ meetingId: id, title: 'Recovered silent call', startTime: 1, transcriptCount: 0 }),
  getTranscripts: async () => [], markMeetingSaved: async (id: string) => { saved.push(id); },
} }));
mock.module('@/lib/live-speaker-edits', () => ({ editedSpeaker: (_id: string, _seq: number, speaker: string) => speaker }));
const { useRecordingStop } = await import('../../src/hooks/useRecordingStop');
const { useTranscriptRecovery } = await import('../../src/hooks/useTranscriptRecovery');
let stop: ReturnType<typeof useRecordingStop>;
let recovery: ReturnType<typeof useTranscriptRecovery>;
function Probe() { stop = useRecordingStop(noop, noop); recovery = useTranscriptRecovery(); return null; }
beforeEach(() => { calls.length = 0; saved.length = 0; deleted.length = 0; routes.length = 0; values.clear(); currentId = 'meeting-first'; flush = async () => {}; });

test('silent three-second stop waits for pending notes and finalizes the native meeting', async () => {
  let release!: () => void;
  const flushed: string[] = [];
  flush = async id => { flushed.push(id); await new Promise<void>(resolve => { release = resolve; }); };
  const timer = spyOn(globalThis, 'setTimeout').mockImplementation(((callback: () => void) => { queueMicrotask(callback); return 1; }) as any);
  let view: ReturnType<typeof create>;
  try {
    await act(async () => { view = create(<Probe />); });
    let pending!: Promise<void>;
    await act(async () => { pending = stop.handleRecordingStop(true); });
    expect(flushed).toEqual(['meeting-first']);
    expect(calls.some(([name]) => name === 'api_save_transcript')).toBe(false);
    await act(async () => { release(); await pending; });
    const request = calls.find(([name]) => name === 'api_save_transcript')?.[1];
    expect(request.meetingId).toBe('meeting-first');
    expect(request.transcripts).toEqual([]);
    expect(saved).toEqual(['meeting-first']);
    expect(routes).toEqual(['/meeting-details?id=meeting-first&source=recording']);
  } finally { act(() => view!.unmount()); timer.mockRestore(); }
});

test('silent crash recovery keeps its ID and cannot borrow the next recording folder', async () => {
  currentId = 'meeting-second';
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<Probe />); });
  try {
    let result: any;
    await act(async () => { result = await recovery.recoverMeeting('meeting-first'); });
    expect(result.meetingId).toBe('meeting-first');
    const request = calls.find(([name]) => name === 'api_save_transcript')?.[1];
    expect(request.meetingId).toBe('meeting-first');
    expect(request.folderPath).toBeNull();
    expect(calls.some(([name]) => name === 'get_meeting_folder_path')).toBe(false);
    expect(saved).toEqual(['meeting-first']);
  } finally { act(() => view.unmount()); }
});

test('deleting a recovered recording removes the native meeting before local recovery and draft data', async () => {
  let view!: ReturnType<typeof create>;
  await act(async () => { view = create(<Probe />); });
  try {
    await act(async () => { await recovery.deleteRecoverableMeeting('meeting-first'); });
    expect(deleted).toEqual(['native:meeting-first', 'draft:meeting-first', 'recovery:meeting-first']);
  } finally { act(() => view.unmount()); }
});
