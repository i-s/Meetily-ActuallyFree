import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, test } from 'bun:test';

const metadata = { meetingId: 'draft-42', title: 'Audio-only take', startTime: 1_700_000_000_000, lastUpdated: Date.now() - 10_000, transcriptCount: 0, savedToSQLite: false, folderPath: '/synthetic/audio-only' };
let markFails = false;
let dbFails = false;
let audioStatus = 'success';
let audioAvailable = true;
let marked = false;
const calls: Array<[string, any]> = [];
const library = new Map<string, string>();
mock.module('@/services/indexedDBService', () => ({ indexedDBService: {
  getAllMeetings: async () => marked ? [] : [{ ...metadata }],
  getMeetingMetadata: async () => ({ ...metadata }),
  getTranscripts: async () => [],
  markMeetingSaved: async () => { if (markFails) throw new Error('IndexedDB write failed'); marked = true; },
} }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push([command, args]);
  if (command === 'has_recoverable_audio') return audioAvailable;
  if (command === 'has_audio_checkpoints') return false; // Finalized take: no checkpoints remain.
  if (command === 'recover_audio_from_checkpoints') return { status: audioStatus, chunk_count: 1, estimated_duration_seconds: 3, audio_file_path: audioStatus === 'success' || audioStatus === 'partial' ? '/synthetic/audio-only/audio.mp4' : undefined, message: 'fixture' };
  if (command === 'cleanup_checkpoints') return;
  if (command === 'api_save_transcript') {
    if (dbFails) throw new Error('SQLite write failed');
    const key = args.recoveryId ?? `non-idempotent-${library.size}`;
    if (!library.has(key)) library.set(key, `meeting-${library.size}`);
    return { meeting_id: library.get(key) };
  }
  throw new Error(`unexpected ${command}`);
} }));
mock.module('@/lib/summary-language-preferences', () => ({ applyPinnedSummaryLanguageToMeeting: async () => {} }));
mock.module('@/lib/workspace-api', () => ({ announceChange: () => {} }));
mock.module('@/lib/live-speaker-edits', () => ({ editedSpeaker: (_: any, __: any, label: any) => label }));
mock.module('sonner', () => ({ toast: { warning: () => {}, error: () => {} } }));
// Keep the real dialog and hook/service; replace only DOM portal/layout boundaries.
mock.module('@/components/ui/dialog', () => Object.fromEntries(['Dialog', 'DialogContent', 'DialogDescription', 'DialogFooter', 'DialogHeader', 'DialogTitle'].map(name => [name, ({ children }: any) => <div>{children}</div>])));
mock.module('@/components/ui/scroll-area', () => ({ ScrollArea: ({ children }: any) => <div>{children}</div> }));
mock.module('@/components/ui/confirm-dialog', () => ({ ConfirmDialog: () => null }));
const { useTranscriptRecovery } = await import('../../src/hooks/useTranscriptRecovery');
const { TranscriptRecovery } = await import('../../src/components/TranscriptRecovery/TranscriptRecovery');
let current: ReturnType<typeof useTranscriptRecovery>;
function App() {
  current = useTranscriptRecovery();
  return <TranscriptRecovery isOpen onClose={() => {}} recoverableMeetings={current.recoverableMeetings} onRecover={current.recoverMeeting} onDelete={async () => {}} onLoadPreview={current.loadMeetingTranscripts} />;
}
beforeEach(() => { calls.length = 0; library.clear(); marked = false; markFails = false; dbFails = false; audioStatus = 'success'; audioAvailable = true; });

test('restart recovery dialog saves validated finalized audio with zero transcript to the library through StorageService', async () => {
  let root: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { root = create(<App />); });
    await act(async () => { await current.checkForRecoverableTranscripts(); });
    expect(current.recoverableMeetings[0].folderPath).toBe(metadata.folderPath);
    const recover = root!.root.findAllByType('button').find(b => b.children.some(c => c === 'Recover'))!;
    await act(async () => { await recover.props.onClick(); });
    expect(library.size).toBe(1);
    expect(marked).toBe(true);
    const saved = calls.find(([name]) => name === 'api_save_transcript')![1];
    expect(saved.transcripts).toEqual([]);
    expect(saved.folderPath).toBe(metadata.folderPath);
    expect(saved.recoveryId).toBe(metadata.meetingId);
  } finally { act(() => root?.unmount()); }
});

test('checkpoint-backed audio retry after database failure and restart retains refs and cannot duplicate a committed meeting', async () => {
  let root: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { root = create(<App />); });
    dbFails = true;
    await act(async () => { await expect(current.recoverMeeting(metadata.meetingId)).rejects.toThrow('SQLite'); });
    expect(marked).toBe(false);
    expect(calls.some(([name]) => name === 'cleanup_checkpoints')).toBe(false);
    dbFails = false; markFails = true;
    await act(async () => { await expect(current.recoverMeeting(metadata.meetingId)).rejects.toThrow('IndexedDB'); });
    expect(library.size).toBe(1);
    act(() => root!.unmount());
    markFails = false;
    await act(async () => { root = create(<App />); });
    await act(async () => { await current.recoverMeeting(metadata.meetingId); });
    expect(library.size).toBe(1);
    expect(marked).toBe(true);
  } finally { act(() => root?.unmount()); }
});

test('empty recovery with no verified audio cannot create a ghost meeting or erase its references', async () => {
  let root: ReturnType<typeof create> | undefined;
  try {
    audioAvailable = false; audioStatus = 'none';
    await act(async () => { root = create(<App />); });
    await act(async () => { await current.checkForRecoverableTranscripts(); });
    expect(current.recoverableMeetings[0].folderPath).toBe(metadata.folderPath);
    await act(async () => { await expect(current.recoverMeeting(metadata.meetingId)).rejects.toThrow(); });
    expect(library.size).toBe(0); expect(marked).toBe(false);
    expect(calls.some(([name]) => name === 'cleanup_checkpoints')).toBe(false);
  } finally { act(() => root?.unmount()); }
});


test('partial source recovery saves verified playback but leaves the draft and checkpoints available for retry', async () => {
  let root: ReturnType<typeof create> | undefined;
  try {
    audioStatus = 'partial';
    await act(async () => { root = create(<App />); });
    await act(async () => { await current.checkForRecoverableTranscripts(); });
    await act(async () => { await current.recoverMeeting(metadata.meetingId); });
    expect(library.size).toBe(1);
    expect(marked).toBe(false);
    expect(current.recoverableMeetings[0].folderPath).toBe(metadata.folderPath);
    expect(calls.some(([name]) => name === 'cleanup_checkpoints')).toBe(false);
  } finally { act(() => root?.unmount()); }
});
