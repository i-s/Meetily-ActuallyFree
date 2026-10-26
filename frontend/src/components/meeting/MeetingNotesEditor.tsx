'use client';
import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { NotesEditor } from '@/components/editor/NotesEditor';
import { getMeetingNotesSnapshot, loadMeetingNotes, subscribeMeetingNotes, updateMeetingNotes } from '@/lib/meeting-notes-store';

export function useMeetingNotes(meetingId: string) {
  const snapshot = useSyncExternalStore(
    useCallback(listener => subscribeMeetingNotes(meetingId, listener), [meetingId]),
    useCallback(() => getMeetingNotesSnapshot(meetingId), [meetingId]),
    useCallback(() => getMeetingNotesSnapshot(meetingId), [meetingId]),
  );
  useEffect(() => { void loadMeetingNotes(meetingId).catch(() => {}); }, [meetingId]);
  return snapshot;
}
export function MeetingNotesEditor({ meetingId }: { meetingId: string }) {
  const notes = useMeetingNotes(meetingId);
  const [retryRevision, setRetryRevision] = useState(0);
  return <div className="h-full overflow-y-auto px-5 py-4 pl-11">
    <div className="mb-3 flex items-center justify-between text-xs text-af-text-3">
      <span>Meeting notes</span>
      <span role="status">{notes.state === 'saved' ? 'Saved' : notes.state === 'saving' || notes.state === 'pending' ? 'Saving…' : ''}</span>
    </div>
    {notes.error && <div role="alert" className="mb-3 text-xs text-af-danger">
      <p>Notes could not be saved: {notes.error}</p>
      <button type="button" className="mt-1 underline" onClick={() => {
        if (notes.loaded) setRetryRevision(value => value + 1);
        else void loadMeetingNotes(meetingId).catch(() => {});
      }}>Retry</button>
    </div>}
    {notes.loaded ? <NotesEditor key={`${meetingId}:${retryRevision}`} initialBlocks={notes.content.json} initialMarkdown={notes.content.markdown}
      placeholder="Write your notes. They are saved automatically to this meeting."
      onDraft={(content, markdown) => updateMeetingNotes(meetingId, content, markdown)} /> : <p className="text-xs text-af-text-3">Loading Meeting notes…</p>}
  </div>;
}
