/** Shared library/detail export. All required reads fail visibly; absence is data,
 * not a catch-all for disk/SQLite errors. Each meeting is a separate read snapshot. */
import { invoke } from '@tauri-apps/api/core';
import { exportSummaryAs, type ExportFormat } from '@/lib/exportSummary';
import { blocksToMarkdown, completeSummaryMarkdown, parseSummaryData } from '@/lib/summary-markdown';
import { notesForContext } from '@/lib/notes-store';
import { displayTitle } from '@/lib/meeting-titles';
import type { MeetingExportSnapshot } from '@/lib/library-api';

export type ExportPart = 'notes' | 'actions' | 'summary' | 'transcript';
export const EXPORT_PARTS: Array<{ id: ExportPart; label: string }> = [
  { id: 'notes', label: 'Your notes' },
  { id: 'actions', label: 'Action items' },
  { id: 'summary', label: 'AI summary' },
  { id: 'transcript', label: 'Transcript' },
];
export interface ExportableMeeting {
  id: string;
  title: string;
  created_at?: string;
  duration_seconds?: number;
  groupName?: string | null;
}

export async function captureMeetingExport(meetingId: string): Promise<MeetingExportSnapshot> {
  const notes = await notesForContext(`meeting:${meetingId}`);
  const snapshot = await invoke<MeetingExportSnapshot>('api_export_meeting_snapshot', { meetingId, expectedNotesRevision: notes.revision });
  // The native transaction rejects cross-WebView revision races. This second
  // flush/read also rejects an edit/conversion in this WebView during capture.
  const after = await notesForContext(`meeting:${meetingId}`);
  if (snapshot.id !== meetingId || snapshot.notes.revision !== notes.revision || after.revision !== notes.revision || after.markdown !== snapshot.notes.markdown) {
    throw new Error('Notes or meeting changed while export was loading. Retry export.');
  }
  return snapshot;
}

function clock(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const h = Math.floor(total / 3600);
  const m = String(Math.floor(total % 3600 / 60)).padStart(2, '0');
  const s = String(total % 60).padStart(2, '0');
  return h ? `${h}:${m}:${s}` : `${m}:${s}`;
}
function literal(text: string): string {
  return text.replace(/\\/g, '\\\\').replace(/([`*_{}\[\]<>#+!|])/g, '\\$1').replace(/\r?\n/g, ' ');
}
/** A raw-text fence preserves every character and prevents transcript/Notes
 * text from forging another top-level source section, even with backticks. */
function verbatim(text: string): string {
  const runs = text.match(/`+/g) ?? [];
  const fence = '`'.repeat(runs.reduce((longest, run) => Math.max(longest, run.length + 1), 3));
  return `${fence}text\n${text}\n${fence}`;
}
function savedSummary(raw: string | null): string {
  let value: unknown = raw;
  for (let depth = 0; depth < 4 && typeof value === 'string'; depth++) {
    try { value = JSON.parse(value); } catch { break; }
  }
  return completeSummaryMarkdown(Array.isArray(value) ? { summary_json: value } : parseSummaryData(value));
}
export function renderMeetingMarkdown(snapshot: MeetingExportSnapshot, parts: Set<ExportPart>): string {
  const sections = [
    `# ${literal(displayTitle(snapshot.title, snapshot.createdAt))}`,
    `Meeting ID: ${literal(snapshot.id)}\n\nRecorded: ${snapshot.createdAt}\n\nSnapshot captured: ${snapshot.capturedAt}`,
    `Audio: ${snapshot.audioAvailable ? 'available locally at capture time (not embedded)' : 'unavailable — file absent or no saved recording'}`,
    '_Local saved snapshot. Later edits are not included. Notes are the user’s own thoughts, not recognized speech or evidence of an agreement._',
  ];
  if (parts.has('notes')) {
    const rich = snapshot.notes.json;
    const legacy = !snapshot.notes.markdown.trim() && Array.isArray(rich) && rich.length > 0;
    const text = legacy ? blocksToMarkdown(rich) : snapshot.notes.markdown;
    const body = text.trim() ? verbatim(text) : '_No Notes text saved._';
    const original = legacy ? `\n\nLegacy rich Notes (original blocks retained below; no data was rewritten):\n\n${verbatim(JSON.stringify(rich, null, 2))}` : '';
    sections.push(`## Your notes\n\nSource: user_notes (user-authored; not recognized speech or agreed conclusions). Revision: ${snapshot.notes.revision}.\n\n${body}${original}`);
  }
  if (parts.has('summary')) {
    const summary = savedSummary(snapshot.summary);
    sections.push(`## Meeting summary\n\nSource: meeting_summary (generated or user-edited conclusions; not verbatim speech). Status: ${literal(snapshot.summaryStatus ?? 'not generated')}.\n\n${summary.trim() ? verbatim(summary) : '_No summary available._'}`);
  }
  if (parts.has('transcript')) {
    const lines = snapshot.transcripts.map(row => `${row.audioStartTime != null ? `[${clock(row.audioStartTime)}] ` : ''}${row.speaker?.trim() ? `${row.speaker}: ` : ''}${row.text}`);
    sections.push(`## Transcript\n\nSource: spoken_transcript (selected editable transcript; speaker labels are not identity proof).\n\n${lines.length ? verbatim(lines.join('\n\n')) : '_No transcript available._'}`);
  }
  if (parts.has('actions')) {
    const lines = snapshot.actions.map(item => `- [${item.done ? 'x' : ' '}] ${item.text}${item.owner ? ` (${item.owner})` : ''}${item.dueText ? ` — ${item.dueText}` : ''} [source: ${item.source}]`);
    sections.push(`## Action items\n\nSource: saved_action_items (AI-extracted or user-authored; verify against the meeting).\n\n${lines.length ? verbatim(lines.join('\n')) : '_No action items saved._'}`);
  }
  return sections.join('\n\n');
}
export async function meetingMarkdown(meeting: ExportableMeeting, parts: Set<ExportPart>): Promise<string> {
  return renderMeetingMarkdown(await captureMeetingExport(meeting.id), parts);
}
export async function exportMeetings(meetings: ExportableMeeting[], parts: Set<ExportPart>, format: ExportFormat, onProgress?: (done: number, total: number) => void, isCurrent: () => boolean = () => true): Promise<boolean> {
  if (!meetings.length || !parts.size) throw new Error('Choose at least one meeting and material to export.');
  const snapshots: MeetingExportSnapshot[] = [];
  const requested = meetings.map(meeting => ({ ...meeting }));
  for (const [index, meeting] of requested.entries()) {
    if (!isCurrent()) return false;
    snapshots.push(await captureMeetingExport(meeting.id));
    if (!isCurrent()) return false;
    onProgress?.(index + 1, meetings.length);
  }
  const markdown = snapshots.map(snapshot => renderMeetingMarkdown(snapshot, parts)).join('\n\n---\n\n');
  const baseName = snapshots.length === 1 ? displayTitle(snapshots[0].title, snapshots[0].createdAt) : `Meetings (${snapshots.length}) ${new Date().toISOString().slice(0, 10)}`;
  return exportSummaryAs(format, markdown, baseName, isCurrent);
}
