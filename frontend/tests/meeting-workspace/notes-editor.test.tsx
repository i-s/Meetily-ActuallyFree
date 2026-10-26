import React, { useEffect } from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const storage = new Map<string, string>();
Object.assign(globalThis, { localStorage: { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) } });
let attempts = 0;
let saved = '';
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_get_meeting_notes') return { json: [{ text: 'keep' }], markdown: 'keep' };
  if (command === 'api_save_meeting_notes') { saved = args.markdown; return; }
  throw new Error(command);
} }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: ({ onDraft }: any) => {
  useEffect(() => { attempts++; onDraft({ json: [{ text: 'keep' }], markdown: '' }, attempts === 1 ? Promise.reject(new Error('Conversion failed')) : Promise.resolve('keep')); }, []);
  return <div>Editor</div>;
} }));
const { MeetingNotesEditor } = await import('../../src/components/meeting/MeetingNotesEditor');
const { flushMeetingNotes } = await import('../../src/lib/meeting-notes-store');
test('retry reconverts the retained JSON after markdown conversion failure', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<MeetingNotesEditor meetingId="retry-conversion" />); });
  expect(JSON.stringify(view!.toJSON())).toContain('Conversion failed');
  await act(async () => { view!.root.findByType('button').props.onClick(); });
  await act(async () => { try { await flushMeetingNotes('retry-conversion'); } catch {} });
  expect(saved).toBe('keep');
  expect(JSON.stringify(view!.toJSON())).toContain('Saved');
  act(() => view!.unmount());
});
