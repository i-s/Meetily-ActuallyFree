'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';

type Segment = { id: string; text: string; speaker?: string; audio_start_time?: number };
type Version = { id: string; kind: string; status: string; created_at: string; error?: string; settings_json: string; summary_json?: string };
type Versions = { selected_id: string; revision: number; versions: Version[]; audio_available: boolean };

/** The selected editable transcript is separate from immutable snapshots. A
 * candidate is never silently applied, including after reopening this page. */
export function TranscriptVersions({ meetingId, onChanged, onBeforeChange }: {
  meetingId: string; onChanged: () => Promise<void> | void;
  onBeforeChange?: () => Promise<() => void>;
}) {
  const [data, setData] = useState<Versions | null>(null);
  const [review, setReview] = useState<Version | null>(null);
  const [reviewRevision, setReviewRevision] = useState<number | null>(null);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refreshPending, setRefreshPending] = useState(false);
  const refreshProjection = async () => {
    setRefreshPending(true);
    await onChanged();
    setRefreshPending(false);
  };
  const [editing, setEditing] = useState<{ revision: number; segments: Segment[] } | null>(null);
  const [editId, setEditId] = useState('');
  const [editText, setEditText] = useState('');
  const refresh = useCallback(async () => {
    try {
      setData(await invoke<Versions>('list_transcript_versions', { meetingId }));
      setError(null);
    } catch (cause) { setError(String(cause)); }
  }, [meetingId]);
  useEffect(() => { void refresh(); }, [refresh]);

  const inspect = async (version: Version) => {
    if (!data) return;
    const revisionAtReview = data.revision;
    setBusy(true);
    try {
      const rows = await invoke<Segment[]>('get_transcript_version', { meetingId, versionId: version.id });
      setSegments(rows);
      setReview(version);
      setReviewRevision(revisionAtReview);
      setError(null);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  const select = async () => {
    if (!review || reviewRevision === null) return;
    setBusy(true);
    let release: (() => void) | undefined;
    try {
      release = await onBeforeChange?.();
      await invoke('select_transcript_version', { meetingId, versionId: review.id, expectedRevision: reviewRevision });
      setReview(null);
      await refresh();
      await refreshProjection();
    } catch (cause) { setError(String(cause)); }
    finally { release?.(); setBusy(false); }
  };
  const editCurrent = async () => {
    setBusy(true);
    try {
      const current = await invoke<{ revision: number; segments: Segment[] }>('get_selected_transcript', { meetingId });
      setEditing(current); setReview(null); setError(null);
      setEditId(current.segments[0]?.id ?? ''); setEditText(current.segments[0]?.text ?? '');
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  };
  const saveCorrection = async () => {
    if (!editing) return;
    setBusy(true);
    let release: (() => void) | undefined;
    try {
      release = await onBeforeChange?.();
      await invoke('correct_transcript_text', { meetingId, transcriptId: editId, text: editText, expectedRevision: editing.revision });
      setEditing(null); await refresh(); await refreshProjection();
    } catch (cause) { setError(String(cause)); }
    finally { release?.(); setBusy(false); }
  };
  return (
    <details className="max-h-[55vh] overflow-auto rounded-xl border border-af-border p-3" onToggle={(event) => { if (event.currentTarget.open) void refresh(); }}>
      <summary className="cursor-pointer text-sm font-medium">Transcript versions</summary>
      <p className="my-2 text-sm text-af-text-2">Live text and corrections stay separate from refined results. Review a snapshot before selecting it. No automatic merging of text or speaker corrections is performed.</p>
      {data && !data.audio_available && <p role="status" className="text-sm text-af-text-2">Saved audio is unavailable. Reprocessing is disabled; saved text versions remain readable.</p>}
      <Button variant="ghost" disabled={busy} onClick={() => void refresh()}>Refresh versions</Button>
      {refreshPending && <Button disabled={busy} onClick={async () => {
        setBusy(true);
        try { await refreshProjection(); setError(null); }
        catch (cause) { setError(String(cause)); }
        finally { setBusy(false); }
      }}>Retry selected view refresh</Button>}
      <Button variant="ghost" disabled={busy || refreshPending || !data || !!editing} onClick={() => void editCurrent()}>Correct selected text</Button>
      {editing && <section aria-label="Correct selected transcript" className="space-y-2">
        <p className="text-sm">Correction changes only this selected line's text, not its speaker, ID or timestamps. The previous text and any summary are saved as a corrections snapshot.</p>
        <select aria-label="Transcript line to correct" value={editId} onChange={(event) => {
          setEditId(event.target.value); setEditText(editing.segments.find((s) => s.id === event.target.value)?.text ?? '');
        }} className="w-full rounded border border-af-border bg-af-panel p-2">
          {editing.segments.map((s) => <option key={s.id} value={s.id}>{s.audio_start_time?.toFixed(1) ?? '—'}s · {s.speaker ?? 'Unknown'} · {s.text.slice(0, 100)}</option>)}
        </select>
        <textarea aria-label="Corrected transcript text" className="w-full rounded border border-af-border bg-af-panel p-2" rows={4} value={editText} onChange={(event) => setEditText(event.target.value)} />
        <Button disabled={busy || !editId || !editText.trim()} onClick={() => void saveCorrection()}>Save text correction</Button>
        <Button variant="ghost" onClick={() => setEditing(null)}>Discard draft correction</Button>
      </section>}
      {error && <p role="alert" className="text-sm text-af-danger">{error}</p>}
      <ul className="space-y-2">
        {data?.versions.map((version) => <li key={version.id} className="text-sm">
          <span>{version.kind} · {version.status} · {new Date(version.created_at).toLocaleString()}{data.selected_id === version.id ? ' · selected (editable)' : ''}</span>
          {version.status === 'ready' && <Button variant="ghost" disabled={busy || refreshPending || !!editing} onClick={() => void inspect(version)}>Review snapshot</Button>}
          {version.status === 'running' && <Button variant="ghost" onClick={async () => {
            try { await invoke('cancel_retranscription_command', { meetingId, versionId: version.id }); await refresh(); }
            catch (cause) { setError(String(cause)); }
          }}>Cancel refinement</Button>}
          {version.error && <p className="text-af-text-2">{version.error}</p>}
        </li>)}
      </ul>
      {review && <section aria-label="Review transcript version" className="mt-3 space-y-2">
        <h3 className="font-medium">{review.kind} snapshot</h3>
        <p className="text-sm text-af-text-2">Selecting this version replaces the displayed transcript, but first saves all current text and speaker corrections as another readable snapshot. Previous summaries are archived and cleared from the selected view. Model size alone does not establish better quality: compare these words and labels against the same audio, including RU/EN terms.</p>
        {review.summary_json && <details><summary>Archived summary (previous selected text)</summary><pre className="max-h-60 overflow-auto whitespace-pre-wrap text-xs">{review.summary_json}</pre></details>}
        <pre className="max-h-24 overflow-auto whitespace-pre-wrap text-xs">{review.settings_json}</pre>
        <div className="max-h-80 overflow-auto rounded-lg border border-af-border p-2">
          {segments.map((segment) => <p key={segment.id} className="mb-2 whitespace-pre-wrap text-sm"><strong>{segment.speaker ?? 'Unknown'}</strong> [{segment.audio_start_time?.toFixed(1) ?? '—'}s] {segment.text}</p>)}
          {!segments.length && <p>No transcript text.</p>}
        </div>
        <Button disabled={busy} onClick={() => void select()}>Save current corrections & select this version</Button>
        <Button variant="ghost" disabled={busy} onClick={() => setReview(null)}>Keep current version</Button>
      </section>}
    </details>
  );
}
