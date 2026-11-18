'use client';

import { useEffect, useState, useSyncExternalStore } from 'react';
import { NotesEditor } from '@/components/editor/NotesEditor';
import { editNotes, flushNotes, getNotesSnapshot, loadNotes, readNotesDocument, recoverNotesDraft, saveRetainedNotes, cancelNotesRecovery, resumeNotesRecovery, retryNotesBackup, saveReviewedNotes, subscribeNotes, type NotesDocument } from '@/lib/notes-store';

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
  const copyRecovery = async (value: string) => {
    setReviewError(null);
    try { await navigator.clipboard.writeText(value); }
    catch (error) { setReviewError(`Copy failed; select the exact JSON/text above: ${String(error)}`); }
  };
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
        {snapshot.state === 'error' && !snapshot.backupBlocked && (
          <button type="button" className="ml-2 underline" onClick={() => {
            if (!snapshot.loaded) void loadNotes(target);
            else { void flushNotes(target); setRetry(value => value + 1); }
          }}>Retry</button>
        )}
      </div>
      {snapshot.warning && <p role="status" className="mb-3 text-xs text-af-text-3">{snapshot.warning}</p>}
      {snapshot.backupBlocked && <div className="mb-3 space-y-2 rounded border border-af-border p-3 text-xs">
        <p>Recovery is read-only. Copy the exact blocks or text, or explicitly save to the meeting without adding browser storage.</p>
        <label>Recovery JSON (exact blocks)
          <textarea aria-label="Recovery JSON (exact blocks)" readOnly value={JSON.stringify(snapshot.content.json, null, 2)} className="block max-h-40 w-full" />
        </label>
        <button type="button" className="underline" onClick={() => {
          void copyRecovery(JSON.stringify(snapshot.content.json, null, 2));
        }}>Copy recovery JSON</button>
        <label>{snapshot.converting ? 'Recovery text (last complete conversion; conversion pending)' : 'Recovery text'}
          <textarea aria-label="Recovery text" readOnly value={snapshot.content.markdown} className="block max-h-40 w-full" />
        </label>
        <button type="button" className="underline" onClick={() => {
          void copyRecovery(snapshot.content.markdown);
        }}>Copy recovery text</button>
        {!snapshot.recoveryPaused && <>
          <button type="button" className="mr-3 underline" disabled={snapshot.converting || snapshot.state === 'saving'} onClick={() => {
            setReviewError(null);
            void saveRetainedNotes(target).catch(error => setReviewError(String(error)));
          }}>Save retained Notes to meeting</button>
          <button type="button" className="mr-3 underline" disabled={snapshot.state === 'saving'} onClick={() => {
            try { cancelNotesRecovery(target); } catch (error) { setReviewError(String(error)); }
          }}>Cancel recovery (keep original)</button>
        </>}
        {snapshot.recoveryPaused && <button type="button" className="mr-3 underline" onClick={() => {
          resumeNotesRecovery(target); setRetry(value => value + 1);
        }}>Resume recovery</button>}
        <button type="button" className="underline" disabled={snapshot.state === 'saving'} onClick={() => {
          void retryNotesBackup(target).then(() => setRetry(value => value + 1)).catch(error => setReviewError(String(error)));
        }}>Retry recovery backup</button>
      </div>}
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
      {snapshot.recoveryDrafts?.map(draft => <div key={draft.key} className="mb-3 rounded border border-af-border p-3 text-xs">
        <p>Retained recovery draft</p>
        <pre className="my-2 max-h-40 overflow-auto whitespace-pre-wrap">{draft.converting ? JSON.stringify(draft.content.json, null, 2) : draft.content.markdown}</pre>
        <button type="button" className="underline" onClick={() => { void recoverNotesDraft(target, draft.key); }}>Open this draft (retain the others)</button>
      </div>)}
      {snapshot.loaded && <NotesEditor
        key={`${target}:${retry}`}
        editable={!snapshot.backupBlocked && !snapshot.recoveryPaused}
        initialBlocks={snapshot.content.json}
        initialMarkdown={snapshot.content.markdown}
        placeholder="Your own Notes — saved locally, separately from speech and AI summaries."
        onDraft={(content, markdown) => editNotes(target, content, markdown)}
      />}
    </div>
  );
}
