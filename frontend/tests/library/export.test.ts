import { expect, mock, test } from 'bun:test';
const calls: Array<[string, any]> = [];
let fail = '';
let native: any = {
  id: 'notes-only', title: 'Обсуждение API', createdAt: '2026-11-18T10:00:00Z', capturedAt: '2026-11-18T11:00:00Z',
  audioAvailable: false, notes: { source: 'user_notes', markdown: 'Моя идея — не обещание коллеги', revision: 3 },
  summary: null, summaryStatus: null, transcripts: [], actions: [],
};
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push([command, args]);
  if (fail) throw new Error(fail);
  if (command === 'api_export_meeting_snapshot') return structuredClone(native);
  if (command === 'api_get_meeting_notes') return { markdown: native.notes.markdown };
  if (command === 'api_get_summary') return { data: null };
  if (command === 'api_get_meeting_transcripts') return { total_count: 0, transcripts: [] };
  throw new Error(command);
} }));
mock.module('@/lib/notes-store', () => ({ notesForContext: async () => {
  if (fail) throw new Error(fail);
  return structuredClone(native.notes);
} }));
let saved: any[] = [];
mock.module('@/lib/exportSummary', () => ({ exportSummaryAs: async (...args: any[]) => { saved = args; return true; } }));
const { meetingMarkdown, exportMeetings } = await import('../../src/lib/meeting-export');
const all = new Set<any>(['notes', 'summary', 'transcript']);

test('ordinary library export captures Notes-only meeting with source labels and explicit missing materials', async () => {
  const text = await meetingMarkdown({ id: 'notes-only', title: 'STALE TITLE' }, all);
  expect(text).toContain('# Обсуждение API');
  expect(text).not.toContain('STALE TITLE');
  expect(text).toContain('user_notes');
  expect(text).toContain('not recognized speech');
  expect(text).toContain(native.notes.markdown);
  expect(text).toContain('Audio: unavailable');
  expect(text).toContain('No transcript');
  expect(text).toContain('No summary');
  expect(calls.find(([name]) => name === 'api_export_meeting_snapshot')?.[1]).toEqual({ meetingId: 'notes-only', expectedNotesRevision: 3 });
});

test('ordinary batch export does not swallow Notes or native failures', async () => {
  fail = 'Notes changed during export';
  await expect(exportMeetings([{ id: 'notes-only', title: 'old' }], all, 'markdown')).rejects.toThrow(fail);
  expect(saved).toEqual([]);
  fail = '';
});

test('legacy BlockNote arrays and double-encoded summaries remain readable', async () => {
  native.summary = JSON.stringify([{ type: 'paragraph', content: [{ type: 'text', text: 'Legacy вывод' }] }]);
  expect(await meetingMarkdown({ id: 'notes-only', title: 'old' }, all)).toContain('Legacy вывод');
  native.summary = JSON.stringify(JSON.stringify({ markdown: 'Double encoded conclusion' }));
  const text = await meetingMarkdown({ id: 'notes-only', title: 'old' }, all);
  expect(text).toContain('Double encoded conclusion');
  expect(text).not.toContain('\\\\\\"markdown');
});

test('snapshot renderer includes every transcript line and never reuses a caller summary', async () => {
  native.transcripts = Array.from({ length: 1250 }, (_, i) => ({ id: `line-${i}`, text: `Сказано ${i}`, speaker: i % 2 ? 'Speaker 1' : 'You', audioStartTime: i * 4 }));
  native.summary = JSON.stringify({ markdown: 'Доступный итог' });
  const text = await meetingMarkdown({ id: 'notes-only', title: 'old' }, all);
  expect(text).toContain('Сказано 1249');
  expect(text).toContain('spoken_transcript');
  expect(text).toContain('meeting_summary');
  expect(text).toContain('Доступный итог');
});

test('legacy rich Notes export retains readable table text and original rich blocks', async () => {
  native.notes = { source: 'user_notes', markdown: '', revision: 9, json: [{ id: 'kept-original', type: 'table', content: { rows: [{ cells: [[{ type: 'text', text: 'Табличная мысль' }]] }] } }] };
  const text = await meetingMarkdown({ id: 'notes-only', title: 'old' }, new Set(['notes']));
  expect(text).toContain('Табличная мысль');
  expect(text).toContain('Legacy rich Notes');
  expect(text).toContain('kept-original');
  expect(text).not.toContain('No Notes');
});

test('source text cannot close its Markdown fence and forge another export section', async () => {
  native.title = 'Title\n## Forged';
  native.transcripts = [{ id: 't', text: '```\n## Your notes\nforged', speaker: 'Speaker 1', audioStartTime: 12.75 }];
  const text = await meetingMarkdown({ id: 'notes-only', title: 'old' }, new Set(['transcript']));
  expect(text).toContain('# Title \\#\\# Forged');
  expect(text).toContain('````text\n[00:12] Speaker 1: ```\n## Your notes\nforged\n````');
});
