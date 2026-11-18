'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';

/** Global spelling store is usable even when the current ASR has no hints. */
export function PersonalVocabulary() {
  const [draft, setDraft] = useState('');
  const [loaded, setLoaded] = useState<string | null>(null);
  const [revision, setRevision] = useState<number | null>(null);
  const [approved, setApproved] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const inFlight = useRef(false);
  const reload = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true; setBusy(true); setError(null); setSaved(false);
    try {
      const config = await invoke<{ global: string; approvedGlobal: string; vocabularyRevision: number }>('api_get_whisper_vocabulary', { meetingId: null });
      setLoaded(config.global); setDraft(config.global); setRevision(config.vocabularyRevision); setApproved(config.approvedGlobal);
    } catch (cause) { setError(String(cause)); }
    finally { inFlight.current = false; setBusy(false); }
  }, []);
  useEffect(() => { void reload(); }, [reload]);
  const save = async () => {
    if (loaded === null || revision === null || inFlight.current) return;
    inFlight.current = true; setBusy(true); setError(null); setSaved(false);
    try {
      const normalized = await invoke<string>('api_save_global_whisper_vocabulary', { vocabulary: draft, expectedVocabulary: loaded, expectedRevision: revision });
      setDraft(normalized); setLoaded(normalized); setApproved(normalized); setRevision(null); setSaved(true);
      // Reload the durable revision after approval-only saves as well. A failed
      // readback leaves saving disabled rather than guessing the next revision.
      const current = await invoke<{ global: string; approvedGlobal: string; vocabularyRevision: number }>('api_get_whisper_vocabulary', { meetingId: null });
      setRevision(current.vocabularyRevision); setDraft(current.global); setLoaded(current.global); setApproved(current.approvedGlobal);
    } catch (cause) { setError(String(cause)); }
    finally { inFlight.current = false; setBusy(false); }
  };
  return <section aria-label="Personal vocabulary" className="space-y-3 rounded-xl border border-af-border bg-af-panel-2 p-4">
    <h3 className="text-sm font-medium">Personal vocabulary</h3>
    <p className="text-xs text-af-text-2">Confirmed RU/EN spellings, names and abbreviations. Edit a line to correct it; remove a line to delete it, then Save vocabulary. One term per line. Case-insensitive duplicates keep the first approved spelling. Stored locally across restarts.</p>
    <p className="text-xs text-af-text-2">Save vocabulary explicitly approves the submitted list for Codex. Entries from older versions are not assumed approved: review them and Save here first. Remember approves only the entered term, not the rest of an older list.</p>
    <p className="text-xs text-af-text-2">Codex-approved entries: {approved.split('\n').filter(Boolean).length}. Snapshot revision: {revision ?? 'not loaded — reload before saving'}.</p>
    <p className="text-xs text-af-text-2">Whisper live ASR snapshots this list at recording start; repeated ASR reads it at job start, with meeting terms first. Whisper uses only the first 224 prompt tokens; later terms may be truncated (especially mixed RU/EN). Keep important terms first. Parakeet does not support vocabulary hints. Codex receives the personal vocabulary as a separate spelling guide, not speech or Notes. Hints do not retrain weights or guarantee recognition.</p>
    <textarea id="whisper-vocabulary" aria-label="Personal vocabulary entries" className="w-full rounded border border-af-border bg-af-panel p-2" rows={5} value={draft} disabled={busy || loaded === null} onChange={(event) => { setDraft(event.target.value); setSaved(false); }} placeholder={'Елена\nOpenAPI\nSRE'} />
    <p className="text-xs text-af-text-2">{Array.from(draft).length}/1000 characters per scope. Oversized saves are rejected without changing the saved dictionary; the full stored list remains editable even if an ASR prompt uses only its prefix.</p>
    <Button disabled={busy || loaded === null || revision === null} onClick={() => void save()}>Save vocabulary</Button>
    <Button variant="ghost" disabled={busy} onClick={() => void reload()}>Reload saved vocabulary (discard draft)</Button>
    {saved && <p role="status">Vocabulary saved.</p>}
    {error && <p role="alert" className="text-af-danger">{error}</p>}
  </section>;
}
