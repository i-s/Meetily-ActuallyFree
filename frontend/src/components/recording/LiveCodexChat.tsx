'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChatThread, type ChatResponse } from '@/components/chat/ChatThread';
import { currentRecordingNotesTarget, getNotesSnapshot, notesForContext } from '@/lib/notes-store';
import { snapshotLiveQuestion, type LiveAnswer, type QuestionLine } from '@/lib/live-codex';
import { stamp } from '@/lib/live-context';

export function LiveCodexChat({ lines, onJumpTo }: { lines: QuestionLine[]; onJumpTo: (id: string) => void }) {
  const [target, setTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef({ lines });
  latest.current = { lines };
  const load = useCallback(() => {
    let valid = true;
    setError(null);
    void currentRecordingNotesTarget().then(value => { if (valid) setTarget(value); }).catch(e => { if (valid) setError(String(e)); });
    return () => { valid = false; };
  }, []);
  useEffect(load, [load]);

  const ask = useCallback(async (question: string, _history: unknown, signal: AbortSignal): Promise<ChatResponse> => {
    if (!target) throw new Error('Current recording identity is loading. Retry shortly.');
    // Capture the UI's already-delivered turns before ANY await. A
    // later ASR event, even with an earlier speech time, belongs to a later ask.
    const captured = structuredClone(latest.current);
    const notes = getNotesSnapshot(target);
    if (!notes.loaded || notes.state !== 'saved' || notes.converting || notes.recoveryPaused || notes.error) throw new Error('Notes are loading, pending or not saved. Open Notes, resolve any error, and retry. Nothing was sent.');
    const requestId = crypto.randomUUID();
    let finished = false;
    let rejectAbort!: (error: Error) => void;
    const aborted = new Promise<never>((_resolve, reject) => { rejectAbort = reject; });
    const cancel = () => {
      if (finished) return;
      finished = true;
      // Native remembers early cancellation IDs: cancel may beat ask IPC.
      void invoke('codex_live_cancel', { requestId }).catch(() => {});
      rejectAbort(new Error('Request cancelled. Recording, transcription and Notes continue.'));
    };
    signal.addEventListener('abort', cancel, { once: true });
    const timer = setTimeout(() => {
      if (finished) return;
      finished = true;
      void invoke('codex_live_cancel', { requestId }).catch(() => {});
      rejectAbort(new Error('Codex request timed out after 60 seconds. Recording, transcription and Notes continue. Retry when ready.'));
    }, 60_000);
    try {
      if (signal.aborted) cancel();
      const work = async (): Promise<ChatResponse> => {
        if (finished) throw new Error('Request cancelled');
        const cutoff = await invoke<{ target: string; asOfSeconds: number }>('codex_live_cutoff');
        if (cutoff.target !== target) throw new Error('Recording changed while the request was accepted. Nothing was sent.');
        if (finished || signal.aborted) throw new Error('Request cancelled');
        const committed = await notesForContext(target);
        // notesForContext enforces committed/revisioned ownership. The editor
        // must also remain the same snapshot captured at the user's click.
        if (getNotesSnapshot(target).content !== notes.content || committed.markdown !== notes.content.markdown) throw new Error('Notes changed while context was prepared. Retry to authorize the newer Notes. Nothing was sent.');
        if (finished || signal.aborted) throw new Error('Request cancelled');
        const mode = question === 'Что я пропустил?' ? 'missed' : 'question';
        const request = snapshotLiveQuestion(target, captured.lines, cutoff.asOfSeconds, mode, question, committed);
        const answer = await invoke<LiveAnswer>('codex_live_ask', { requestId, request });
        if (finished || signal.aborted) throw new Error('Request cancelled');
        return {
          answer: answer.answer,
          citations: answer.evidence,
          sourceStatus: [
            `Available speech ${stamp(answer.fromSeconds)}–${stamp(answer.asOfSeconds)}; ASR may lag. Whole turns only.`,
            answer.uncertain ? 'Insufficient/uncertain evidence.' : '',
            answer.vocabularyRevision != null ? `Personal vocabulary · revision ${answer.vocabularyRevision} (approved spellings only, not speech evidence).` : '',
            answer.notesRevision !== null ? `Personal Notes · revision ${answer.notesRevision} (whole-meeting Notes, not window-timed speech or proof of agreement; no speech timestamp).` : 'Personal Notes not cited.',
          ].filter(Boolean).join(' '),
        };
      };
      return await Promise.race([work(), aborted]);
    } finally {
      finished = true;
      clearTimeout(timer);
      signal.removeEventListener('abort', cancel);
    }
  }, [target]);

  if (!target) return <div className="p-4 text-xs">{error ? <p role="alert">{error} <button type="button" onClick={load}>Retry</button></p> : 'Loading current recording…'}</div>;
  return <ChatThread
    historyKey={`live-codex:${target}`}
    ask={ask}
    compact cancellable strictCitations
    onCite={(id, sequenceId) => {
      // A WebView reload changes UI IDs; native sequence ownership remains the
      // citation anchor within this same recording target.
      const line = latest.current.lines.find(line => line.id === id || (sequenceId !== undefined && line.sequenceId === sequenceId));
      if (line) onJumpTo(line.id);
    }}
    quickActions={['Что я пропустил?']}
    emptyTitle="Ask about this call"
    emptyHint="Only this recording's available text and separately committed personal Notes are sent. The five-minute action uses the recording clock, excluding pauses."
    suggestions={['What has been decided?', 'Which action items came up?']}
    placeholder="Ask about the call…"
    pendingLabel="Waiting for subscription Codex · up to 60 seconds…"
    footnote="Sends text to OpenAI via your installed local Codex CLI and ChatGPT subscription only. No audio, other meetings, API keys or provider fallback. Configure Settings → Live questions."
  />;
}
