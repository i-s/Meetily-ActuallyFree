'use client';

import { useEffect, useState, useSyncExternalStore } from 'react';
import { NotesEditor } from '@/components/editor/NotesEditor';
import { editNotes, flushNotes, getNotesSnapshot, loadNotes, readNotesDocument, saveReviewedNotes, subscribeNotes, type NotesDocument } from '@/lib/notes-store';

/** Shared by the normal live panel and library detail view. Keep ASR updates
 * outside this document's ownership: they must never remount its editor. */
export function MeetingNotesEditor({ target }: { target: string }) {
  const snapshot = useSyncExternalStore(
    listener => subscribeNotes(target, listener),
    () => getNotesSnapshot(target),
    () => getNotesSnapshot(target),
  );
  const [retry, setRetry] = useState(0);
  const [review, setReview] = useState<NotesDocument | null>(null);
  const [reviewError, setReviewError] = useState<string | null>(null);
  useEffect(() => {
    void loadNotes(target, true);
    const flush = () => { void flushNotes(target); };
    window.addEventListener('pagehide', flush);
    return () => { window.removeEventListener('pagehide', flush); flush(); };
  }, [target]);
  return (
    <div>
      <div role={snapshot.error ? 'alert' : 'status'} aria-live="polite" className={`mb-3 text-xs ${snapshot.error ? 'text-af-danger' : 'text-af-text-3'}`}>
        {snapshot.error || ({ loading: 'Loading Notes…', pending: 'Unsaved changes…', saving: 'Saving…', saved: 'Saved locally', error: 'Notes not saved' }[snapshot.state])}
        {snapshot.state === 'error' && (
          <button type="button" className="ml-2 underline" onClick={() => {
            if (!snapshot.loaded) void loadNotes(target);
            else { void flushNotes(target); setRetry(value => value + 1); }
          }}>Retry</button>
        )}
      </div>
      {snapshot.error?.includes('Notes conflict') && <button type="button" className="mb-3 text-xs underline" onClick={() => {
        void readNotesDocument(target).then(stored => {
          setReview(stored ?? { markdown: '', json: null, revision: 0 });
          setReviewError(null);
        }).catch(error => setReviewError(String(error)));
      }}>Review saved version</button>}
      {reviewError && <p role="alert">{reviewError}</p>}
      {review && <div className="mb-3 rounded border border-af-border p-3 text-xs">
        <p>Saved version (revision {review.revision}). Your recovery draft remains in the editor below.</p>
        <pre className="my-2 max-h-40 overflow-auto whitespace-pre-wrap">{review.markdown}</pre>
        <button type="button" className="underline" onClick={() => {
          void saveReviewedNotes(target, review.revision).then(() => setReview(null)).catch(error => setReviewError(String(error)));
        }}>Replace reviewed version with my draft</button>
      </div>}
      {snapshot.loaded && <NotesEditor
        key={`${target}:${retry}`}
        initialBlocks={snapshot.content.json}
        initialMarkdown={snapshot.content.markdown}
        placeholder="Your own Notes — saved locally, separately from speech and AI summaries."
        onDraft={(content, markdown) => editNotes(target, content, markdown)}
      />}
    </div>
  );
}
