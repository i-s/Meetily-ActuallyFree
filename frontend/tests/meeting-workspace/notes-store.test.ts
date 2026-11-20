import { expect, mock, test } from 'bun:test';
const rows = new Map<string, any>();
const drafts = new Map<string, string>();
let rejectSave = false;
let releaseSave: (() => void) | undefined;
let holdSave = false;
Object.assign(globalThis, { localStorage: { getItem: (key: string) => drafts.get(key) ?? null, setItem: (key: string, value: string) => drafts.set(key, value), removeItem: (key: string) => drafts.delete(key) } });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_get_meeting_notes') return rows.get(args.meetingId) ?? null;
  if (command === 'api_save_meeting_notes') {
    if (rejectSave) throw new Error('disk full');
    if (holdSave) { holdSave = false; await new Promise<void>(resolve => { releaseSave = resolve; }); }
    rows.set(args.meetingId, { markdown: args.markdown, json: args.json }); return;
  }
  throw new Error(command);
} }));
const store = await import('../../src/lib/meeting-notes-store');

test('typing is immediately durable per ID and flush waits for markdown conversion', async () => {
  await store.loadMeetingNotes('silent');
  let converted!: (value: string) => void;
  const markdown = new Promise<string>(resolve => { converted = resolve; });
  store.updateMeetingNotes('silent', { json: [{ text: 'Only notes' }], markdown: '' }, markdown);
  expect([...drafts.values()].some(value => value.includes('Only notes'))).toBe(true);
  const flush = store.flushMeetingNotes('silent');
  expect(rows.has('silent')).toBe(false);
  converted('Only notes'); await flush;
  expect(rows.get('silent').markdown).toBe('Only notes');
});

test('pending save retains its original ID and flush includes edits made during save', async () => {
  await store.loadMeetingNotes('first'); await store.loadMeetingNotes('next');
  store.updateMeetingNotes('first', { json: [], markdown: 'old' });
  holdSave = true;
  const saving = store.flushMeetingNotes('first');
  await new Promise(resolve => setTimeout(resolve, 0));
  store.updateMeetingNotes('first', { json: [], markdown: 'latest' });
  store.updateMeetingNotes('next', { json: [], markdown: 'separate' });
  releaseSave!(); await saving; await store.flushMeetingNotes('next');
  expect(rows.get('first').markdown).toBe('latest');
  expect(rows.get('next').markdown).toBe('separate');
});

test('save failure retains draft and surfaces error until retry succeeds', async () => {
  await store.loadMeetingNotes('retry');
  store.updateMeetingNotes('retry', { json: [], markdown: 'keep me' });
  rejectSave = true;
  await expect(store.flushMeetingNotes('retry')).rejects.toThrow('disk full');
  expect(store.getMeetingNotesSnapshot('retry').error).toContain('disk full');
  expect([...drafts.values()].some(value => value.includes('keep me'))).toBe(true);
  rejectSave = false; await store.flushMeetingNotes('retry');
  expect(store.getMeetingNotesSnapshot('retry').state).toBe('saved');
});

test('deleting a meeting removes its draft and blocks late editor conversion or edits', async () => {
  await store.loadMeetingNotes('deleted');
  let converted!: (value: string) => void;
  store.updateMeetingNotes('deleted', { json: [{ text: 'private draft' }], markdown: '' }, new Promise(resolve => { converted = resolve; }));
  store.forgetMeetingNotes('deleted');
  converted('private draft'); await Promise.resolve();
  store.updateMeetingNotes('deleted', { json: [], markdown: 'late edit' });
  expect(drafts.has('af-meeting-notes-draft:deleted')).toBe(false);
  await expect(store.flushMeetingNotes('deleted')).rejects.toThrow('deleted');
  expect(rows.has('deleted')).toBe(false);
});
