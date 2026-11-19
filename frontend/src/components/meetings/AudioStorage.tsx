'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { announceChange } from '@/lib/workspace-api';

type Recording = { meetingId: string; title: string; createdAt: string; bytes: number; files: { name: string; bytes: number }[]; blocked: string | null };
type Inventory = { totalBytes: number; usageComplete: boolean; recordings: Recording[] };
type Consent = { token: string; bytes: number; recordings: Recording[] };
type Outcome = { removedBytes: number; files: { meetingId: string; name: string; status: string; removedBytes: number; error: string | null }[] };

function size(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
}
function oldRecording(recording: Recording) {
  const date = Date.parse(recording.createdAt);
  return Number.isFinite(date) && date < Date.now() - 30 * 86400000;
}

/** Native consent owns exact file scope. Closing/reloading cannot authorize an
 * operation; every retry starts with a fresh unchecked inventory and review. */
export function AudioStorage() {
  const [inventory, setInventory] = useState<Inventory | null>(null);
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [includeNewer, setIncludeNewer] = useState(false);
  const [consent, setConsent] = useState<Consent | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submitting = useRef(false);
  const generation = useRef(0);
  const actionGeneration = useRef(0);
  const mounted = useRef(true);
  const refresh = useCallback(async () => {
    const run = ++generation.current;
    setConsent(null); setSelected(new Set());
    try {
      const result = await invoke<Inventory>('api_audio_storage');
      if (mounted.current && run === generation.current) { setInventory(result); }
    } catch (cause) {
      if (mounted.current && run === generation.current) setError(`Cannot read audio storage: ${String(cause)}`);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const changed = () => { void refresh(); };
    window.addEventListener('focus', changed);
    let stop: (() => void) | undefined;
    void listen('audio-cleanup-finished', () => {
      announceChange('meetings'); changed();
    }).then(unlisten => { if (mounted.current) stop = unlisten; else unlisten(); }).catch(cause => {
      if (mounted.current) setError(`Audio change notifications unavailable: ${String(cause)}. Refresh before reviewing.`);
    });
    return () => { mounted.current = false; ++generation.current; ++actionGeneration.current; stop?.(); window.removeEventListener('focus', changed); };
  }, [refresh]);

  const review = async () => {
    if (submitting.current || !selected.size) return;
    submitting.current = true; setBusy(true); setError(null); setOutcome(null);
    const run = ++actionGeneration.current;
    try {
      const plan = await invoke<Consent>('api_prepare_audio_cleanup', { meetingIds: [...selected] });
      if (mounted.current && run === actionGeneration.current) setConsent(plan);
    } catch (cause) {
      if (mounted.current && run === actionGeneration.current) { setError(String(cause)); await refresh(); }
    } finally { submitting.current = false; if (mounted.current) setBusy(false); }
  };
  const confirm = async () => {
    if (submitting.current || !consent) return;
    submitting.current = true; setBusy(true); setError(null);
    const token = consent.token;
    setConsent(null);
    const run = ++actionGeneration.current;
    try {
      const result = await invoke<Outcome>('api_confirm_audio_cleanup', { token });
      if (mounted.current && run === actionGeneration.current) {
        setOutcome(result); announceChange('meetings');
      }
    } catch (cause) {
      if (mounted.current && run === actionGeneration.current) setError(`Cleanup result could not be confirmed: ${String(cause)}. Refresh and review remaining files; no freed-space total is assumed.`);
    } finally {
      submitting.current = false;
      if (mounted.current && run === actionGeneration.current) { setBusy(false); await refresh(); }
    }
  };
  const changeOpen = (next: boolean) => {
    if (submitting.current) return;
    ++actionGeneration.current; setOpen(next); setConsent(null); setOutcome(null); setError(null);
    setSelected(new Set()); setIncludeNewer(false);
    if (next) void refresh();
  };
  const rows = inventory?.recordings.filter(row => includeNewer || oldRecording(row)) ?? [];
  const chosenBytes = inventory?.recordings.filter(row => selected.has(row.meetingId)).reduce((sum, row) => sum + row.bytes, 0) ?? 0;

  return <section className="mt-4 rounded-xl border border-af-border p-4 text-sm" aria-label="Audio storage">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <p>Audio storage: {inventory ? `${size(inventory.totalBytes)} observed in saved recording folders` : 'Checking…'}{inventory && !inventory.usageComplete && ' — incomplete measurement; some folders could not be safely read'}</p>
      <Button variant="secondary" size="sm" onClick={() => changeOpen(true)}>Review audio cleanup</Button>
    </div>
    {!open && error && <p role="alert">{error} <button onClick={() => { setError(null); void refresh(); }}>Refresh audio storage</button></p>}
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent showCloseButton={!busy} className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{consent ? 'Confirm audio-only deletion' : 'Audio storage and cleanup'}</DialogTitle>
          <DialogDescription>Only explicitly selected final audio files are deleted. Nothing is automatically selected or deleted.</DialogDescription>
        </DialogHeader>
        <p>App suggestion: review recordings older than 30 days. This is a convenience heuristic, not a storage limit agreed by you. You may keep every recording.</p>
        <p>Retained: transcripts, transcript versions and manual corrections, Notes, summaries, tasks, queue artifacts and recording metadata. Search and Markdown export remain available. Playback and repeat ASR of deleted audio will be unavailable. Deletion cannot be undone.</p>
        {error && <p role="alert">{error}</p>}
        {outcome && <div role="status" className="rounded-lg border border-af-border p-3">
          <p>Removed file bytes: {size(outcome.removedBytes)} — not a measurement of freed disk space. Open players, filesystem snapshots or other OS storage may retain blocks.</p>
          <ul>{outcome.files.map(file => <li key={`${file.meetingId}/${file.name}`}>{inventory?.recordings.find(row => row.meetingId === file.meetingId)?.title ?? file.meetingId} / {file.name}: {file.status}{file.error ? ` — ${file.error}` : ''}</li>)}</ul>
          <p>Any blocked or failed audio was not counted as removed. To retry, refresh, select remaining recordings and review again.</p>
        </div>}
        {consent ? <>
          <p>Delete only these listed files ({size(consent.bytes)})? This review expires in 15 minutes. Native ownership and job state are checked again at deletion.</p>
          <ul>{consent.recordings.map(row => <li key={row.meetingId} className="mb-3"><strong>{row.title}</strong> · {row.createdAt}<ul>{row.files.map(file => <li key={file.name}>{file.name} · {size(file.bytes)}</li>)}</ul></li>)}</ul>
          <DialogFooter><Button variant="secondary" disabled={busy} onClick={() => setConsent(null)}>Back to selection</Button><Button variant="destructive" disabled={busy} onClick={confirm}>Delete listed audio</Button></DialogFooter>
        </> : <>
          <label className="flex items-center gap-2"><input type="checkbox" checked={includeNewer} disabled={busy} onChange={event => { setIncludeNewer(event.target.checked); setSelected(new Set()); }} />Show newer or undated recordings too</label>
          <p>{inventory ? `${size(inventory.totalBytes)} observed total${inventory.usageComplete ? '' : ' — incomplete measurement'} · ${selected.size} selected · ${size(chosenBytes)} selected audio` : 'Checking audio storage…'}</p>
          <ul className="max-h-72 space-y-3 overflow-y-auto">{rows.map(row => <li key={row.meetingId}><label className="flex items-start gap-2"><input type="checkbox" aria-label={`Select audio for ${row.title}`} checked={selected.has(row.meetingId)} disabled={busy || !!row.blocked || (!selected.has(row.meetingId) && selected.size >= 100)} onChange={event => { const checked = event.target.checked; setSelected(current => { const next = new Set(current); if (checked) next.add(row.meetingId); else next.delete(row.meetingId); return next; }); }} /><span><strong>{row.title}</strong> · {row.createdAt} · {size(row.bytes)}<br />{row.blocked ?? row.files.map(file => file.name).join(', ')}</span></label></li>)}</ul>
          {inventory && !rows.length && <p>No older saved recordings to review. Nothing will be deleted.</p>}
          <DialogFooter><Button variant="secondary" disabled={busy} onClick={() => { setError(null); void refresh(); }}>Refresh audio storage</Button><Button disabled={busy || !selected.size} onClick={review}>Review selected audio</Button></DialogFooter>
        </>}
      </DialogContent>
    </Dialog>
  </section>;
}
