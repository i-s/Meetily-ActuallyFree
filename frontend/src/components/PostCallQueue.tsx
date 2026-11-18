'use client';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Job = {
  meeting_id: string; status: string; stage: string; epoch: number;
  candidate_id: string | null; error: string | null; result_json: string | null;
  result_version: string | null; result_transcript_revision: number | null;
  result_notes_revision: number | null; result_current: boolean;
};
type Artifact = {
  version_id: string; transcript_revision: number; notes_revision: number;
  result_json: string; created_at: string;
};
type Claim = {
  text: string; evidence: Array<{ source: string; row_id: string | null; quote: string }>;
  owner: { value: string } | null; deadline: { value: string } | null;
};
type Result = { summary: Claim[]; decisions: Claim[]; tasks: Claim[]; open_questions: Claim[]; uncertainty: string[] };
const labels: Record<string, string> = {
  queued: 'Queued · recording has priority', running: 'Processing', deferred: 'Deferred',
  waiting: 'Waiting for retry', cancelled: 'Cancelled · prior results kept', complete: 'Complete',
};
const meetingHref = (id: string) => `/meeting-details?id=${encodeURIComponent(id)}`;

function SummaryResult({ json, meetingId, current }: { json: string; meetingId: string; current: boolean }) {
  let result: Result;
  try { result = JSON.parse(json); } catch { return <p>Stored result could not be displayed. It has not been deleted.</p>; }
  return <div className="space-y-2 text-xs">
    {(['summary', 'decisions', 'tasks', 'open_questions'] as const).map(section => <section key={section}>
      <h4 className="font-medium capitalize">{section.replace('_', ' ')}</h4>
      {(result[section] ?? []).length === 0 && <p className="text-muted-foreground">No supported items.</p>}
      {(result[section] ?? []).map((claim, index) => <div key={index} className="my-1 border-l pl-2">
        {/* Model output is text, never executable HTML/Markdown or remote links. */}
        <p className="whitespace-pre-wrap">{claim.text}</p>
        {(claim.owner || claim.deadline) && <p>Owner: {claim.owner?.value ?? 'Not established'} · Due: {claim.deadline?.value ?? 'Not established'}</p>}
        {(claim.evidence ?? []).map((e, i) => <blockquote key={i} className="text-muted-foreground">
          {e.source === 'user_notes' ? 'Personal Notes (not speech): ' : 'Speech evidence: '}
          “{e.quote}”
          {current && e.source === 'transcript' && e.row_id && <a className="ml-1 underline" href={`${meetingHref(meetingId)}&t=${encodeURIComponent(e.row_id)}`}>Open speech</a>}
        </blockquote>)}
      </div>)}
    </section>)}
    {(result.uncertainty ?? []).map((text, i) => <p key={i} className="text-muted-foreground">Uncertain: {text}</p>)}
  </div>;
}

function QueueJob({ job, control, busy }: { job: Job; control: (id: string, action: string) => Promise<void>; busy: boolean }) {
  const [history, setHistory] = useState<Artifact[] | null>(null);
  const [localProgress, setLocalProgress] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    let disposed = false;
    let dispose: (() => void) | undefined;
    setLocalProgress(null);
    void listen<{ meeting_id: string; message: string }>('retranscription-progress', event => {
      if (!disposed && event.payload.meeting_id === job.meeting_id) setLocalProgress(event.payload.message);
    }).then(unlisten => { if (disposed) unlisten(); else dispose = unlisten; }).catch(() => {});
    return () => { disposed = true; dispose?.(); };
  }, [job.meeting_id, job.epoch]);
  const loadHistory = async () => {
    try {
      const rows = await invoke<Artifact[]>('post_call_results', { meetingId: job.meeting_id });
      if (mounted.current) { setHistory(rows); setError(null); }
    } catch (cause) { if (mounted.current) setError(String(cause)); }
  };
  return <article className="space-y-2 border-t py-3" aria-label={`Post-call job ${job.meeting_id}`}>
    <a href={meetingHref(job.meeting_id)} className="block truncate text-sm font-medium underline">Open meeting · {job.meeting_id}</a>
    <p role="status" className="text-xs">{labels[job.status] ?? job.status} · {job.stage === 'local' ? 'Local ASR → speaker refinement' : 'Codex subscription summary'}</p>
    {job.status === 'running' && job.stage === 'local' && localProgress && <p className="text-xs text-muted-foreground">{localProgress}</p>}
    {job.candidate_id && <p className="text-xs text-muted-foreground">Local candidate retained separately. Review in Transcript versions; it is not selected automatically.</p>}
    {job.error && <p role="alert" className="text-xs text-destructive whitespace-pre-wrap">{job.error}</p>}
    <div className="flex flex-wrap gap-2 text-xs">
      {['queued', 'running', 'waiting'].includes(job.status) && <button disabled={busy} className="underline" onClick={() => void control(job.meeting_id, 'defer')}>Defer</button>}
      {job.status === 'deferred' && <button disabled={busy} className="underline" onClick={() => void control(job.meeting_id, 'resume')}>Resume</button>}
      {['waiting', 'cancelled'].includes(job.status) && <button disabled={busy} className="underline" onClick={() => void control(job.meeting_id, 'retry')}>Retry</button>}
      {!['cancelled', 'complete'].includes(job.status) && <button disabled={busy} className="underline" onClick={() => void control(job.meeting_id, 'cancel')}>Cancel</button>}
      {job.status === 'complete' && <button disabled={busy} className="underline" onClick={() => void control(job.meeting_id, 'refresh')}>Summarize current selection</button>}
      {job.result_json && <button className="underline" onClick={() => void loadHistory()}>History</button>}
    </div>
    {job.result_json && <details>
      <summary className="cursor-pointer text-xs">{job.result_current ? 'Saved summary' : 'Outdated summary · selected transcript or Notes changed'}</summary>
      <p className="my-1 break-all text-xs text-muted-foreground">Version {job.result_version} · transcript revision {job.result_transcript_revision} · Notes revision {job.result_notes_revision}</p>
      <SummaryResult json={job.result_json} meetingId={job.meeting_id} current={job.result_current} />
    </details>}
    {history && <details open><summary className="text-xs">Retained summary history ({history.length})</summary>{history.map(item => <details key={`${item.version_id}:${item.transcript_revision}:${item.notes_revision}`}>
      <summary className="text-xs">{item.created_at} · transcript {item.transcript_revision} · Notes {item.notes_revision}</summary>
      <SummaryResult json={item.result_json} meetingId={job.meeting_id} current={job.result_current && item.version_id === job.result_version && item.transcript_revision === job.result_transcript_revision && item.notes_revision === job.result_notes_revision} />
    </details>)}</details>}
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
  </article>;
}

/** Observation/control only: closing/remounting this view never cancels a job. */
export function PostCallQueue() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  const mounted = useRef(true);
  const refresh = useCallback(async () => {
    const request = ++generation.current;
    try {
      const rows = await invoke<Job[]>('post_call_list');
      if (mounted.current && request === generation.current) { setJobs(rows); setError(null); }
    } catch (cause) { if (mounted.current && request === generation.current) setError(String(cause)); }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const interval = setInterval(() => void refresh(), 1500);
    let dispose: (() => void) | undefined;
    void listen('post-call-queue-changed', () => void refresh()).then(unlisten => {
      if (mounted.current) dispose = unlisten; else unlisten();
    }).catch(() => { /* Polling remains a read-only fallback for a missed event. */ });
    return () => { mounted.current = false; generation.current++; clearInterval(interval); dispose?.(); };
  }, [refresh]);
  const control = async (meetingId: string, action: string) => {
    setBusy(true);
    try { await invoke('post_call_control', { meetingId, action }); await refresh(); }
    catch (cause) { if (mounted.current) setError(String(cause)); }
    finally { if (mounted.current) setBusy(false); }
  };
  if (!jobs.length && !error) return null;
  const active = jobs.filter(j => ['queued', 'running', 'waiting'].includes(j.status)).length;
  return <details id="post-call-queue" className="fixed bottom-4 right-4 z-40 max-w-[calc(100vw-2rem)] w-96 rounded-lg border bg-background p-3 shadow-lg">
    <summary className="cursor-pointer text-sm font-medium">Post-call processing · {active} pending</summary>
    <p className="mt-2 text-xs text-muted-foreground">Runs on battery too. New recordings take priority. Notes are personal, not participant agreements. Summaries use the selected transcript; refined candidates require review.</p>
    {error && <p role="alert" className="text-xs text-destructive">{error}</p>}
    <div className="max-h-[65vh] overflow-y-auto">{jobs.map(job => <QueueJob key={job.meeting_id} job={job} control={control} busy={busy} />)}</div>
  </details>;
}

export function PostCallQueueNotice({ enabled, meetingId, onComplete }: { enabled: boolean; meetingId: string; onComplete: () => void | Promise<void> }) {
  const consumed = useRef<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const complete = useCallback(async () => {
    try { await onComplete(); consumed.current = meetingId; setError(null); }
    catch (cause) { setError(String(cause)); }
  }, [onComplete, meetingId]);
  useEffect(() => { if (enabled && consumed.current !== meetingId) { consumed.current = meetingId; void complete(); } }, [enabled, meetingId, complete]);
  if (!enabled) return null;
  return <aside className="rounded-md border p-3 text-sm">
    Saved to the native post-call queue. Local refinement and subscription Codex summary continue independently of this page.
    <a className="ml-1 underline" href="#post-call-queue" onClick={() => {
      const queue = document.getElementById('post-call-queue');
      if (queue instanceof HTMLDetailsElement) queue.open = true;
    }}>Open processing queue</a>
    {error && <p role="alert">{error} <button onClick={() => void complete()}>Retry handoff</button></p>}
  </aside>;
}
