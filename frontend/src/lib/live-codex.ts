import type { LiveLine } from '@/lib/live-context';
import type { NotesDocument } from '@/lib/notes-store';

export type LiveQuestionMode = 'question' | 'missed';
export interface QuestionLine extends LiveLine { sequenceId?: number; endTime?: number }
export interface LiveQuestionSnapshot {
  target: string;
  asOfSeconds: number;
  fromSeconds: number;
  mode: LiveQuestionMode;
  question: string;
  lines: QuestionLine[];
  userNotes: { source: 'user_notes'; markdown: string; revision: number };
}
export interface LiveAnswer {
  answer: string;
  uncertain: boolean;
  evidence: Array<{ id: string; sequenceId: number; time: number; text: string }>;
  notesRevision: number | null;
  vocabularyRevision?: number | null;
  asOfSeconds: number;
  fromSeconds: number;
}

/** Seconds are the recording's pause-excluding audio clock, not wall time and
 * not the most recently transcribed turn. No retrieval truncation: a full long
 * meeting is sent or a visible size error asks the user to narrow the request. */
export function snapshotLiveQuestion(target: string, lines: QuestionLine[], asOfSeconds: number, mode: LiveQuestionMode, question: string, notes: NotesDocument & { source: 'user_notes' }): LiveQuestionSnapshot {
  if (!target.startsWith('recording:') || !target.slice(10) || !Number.isFinite(asOfSeconds) || asOfSeconds < 0) throw new Error('Current recording identity or clock is unavailable. Retry from the live recording.');
  if (mode === 'question' && (!question.trim() || question.length > 4000)) throw new Error('Enter a question of at most 4000 characters.');
  const fromSeconds = mode === 'missed' ? Math.max(0, asOfSeconds - 300) : 0;
  const selected = lines.filter(line => {
    if (!Number.isFinite(line.time) || !Number.isFinite(line.endTime) || line.time < 0 || line.endTime! < line.time || !Number.isSafeInteger(line.sequenceId)) throw new Error('Some transcript timing is unavailable. Retry when the live transcript is synchronized.');
    // Whole turns only. A turn crossing the boundary cannot be word-trimmed
    // without word timestamps; do not attach earlier speech to a later time.
    return line.time >= fromSeconds && line.endTime! <= asOfSeconds;
  });
  const snapshot: LiveQuestionSnapshot = {
    target, asOfSeconds, fromSeconds, mode,
    question: mode === 'missed' ? 'Что я пропустил? Summarize only the requested last five minutes, in the meeting language.' : question.trim(),
    lines: structuredClone(selected),
    userNotes: { source: 'user_notes', markdown: notes.markdown, revision: notes.revision },
  };
  if (new TextEncoder().encode(JSON.stringify(snapshot)).length > 512 * 1024) throw new Error('Meeting context exceeds the safe 512 KiB limit. Nothing was sent. Use the five-minute lookback or a shorter meeting.');
  return snapshot;
}
