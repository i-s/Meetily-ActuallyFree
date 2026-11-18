import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let writes: string[] = [];
let chosen: string | null = '/synthetic/chosen.md';
let pickerGate: Promise<void> | null = null;
let failure = false;
let closed = false;
const errors: string[] = [];
const successes: string[] = [];
mock.module('@tauri-apps/plugin-dialog', () => ({ save: async () => { if (pickerGate) await pickerGate; return chosen; } }));
mock.module('@tauri-apps/plugin-fs', () => ({ writeFile: async (_path: string, bytes: Uint8Array) => { if (failure) throw new Error('write denied'); writes.push(new TextDecoder().decode(bytes)); } }));
mock.module('@/lib/notes-store', () => ({ notesForContext: async () => ({ revision: 2, markdown: 'Моя идея', source: 'user_notes' }) }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command !== 'api_export_meeting_snapshot') throw new Error(command);
  return { id: args.meetingId, title: 'Текущая встреча', createdAt: '2026-11-18', capturedAt: '2026-11-18', audioAvailable: false, notes: { revision: 2, markdown: 'Моя идея', source: 'user_notes' }, summary: null, summaryStatus: null, transcripts: [], actions: [] };
} }));
mock.module('sonner', () => ({ toast: { error: (text: string, args?: any) => errors.push(`${text} ${args?.description}`), success: (text: string) => successes.push(text) } }));
const Wrap = ({ children }: any) => <div>{children}</div>;
mock.module('@/components/ui/dialog', () => ({ Dialog: Wrap, DialogContent: Wrap, DialogDescription: Wrap, DialogFooter: Wrap, DialogHeader: Wrap, DialogTitle: Wrap }));
mock.module('@/components/ui/surface', () => ({ Overline: Wrap }));
mock.module('@/components/ui/button', () => ({ Button: ({ children, loading, ...props }: any) => <button {...props}>{children}</button> }));
mock.module('@/components/ui/checkbox', () => ({ Checkbox: (props: any) => <input type="checkbox" {...props} /> }));
const { ExportMeetingsDialog } = await import('../../src/components/meetings/ExportMeetingsDialog');
const render = (id: string) => <ExportMeetingsDialog open onOpenChange={(open) => { closed = !open; }} meetings={[{ id, title: 'STALE TITLE' }]} />;
const exportButton = (root: ReactTestRenderer) => root.root.findAllByType('button').find(node => node.children.includes('Export'))!;

test('normal library export dialog defaults to complete Markdown for Notes-only meeting', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(render('notes')); });
  expect(root.root.findAllByType('input').every(node => node.props.checked)).toBe(true);
  await act(async () => { exportButton(root).props.onClick(); });
  expect(writes.at(-1)).toContain('## Transcript'); expect(writes.at(-1)).toContain('user_notes');
  expect(writes.at(-1)).toContain('Моя идея'); expect(writes.at(-1)).toContain('No summary');
  expect(writes.at(-1)).not.toContain('STALE TITLE'); expect(closed).toBe(true);
  await act(async () => root.unmount());
});

test('library dialog cancels stale selection writes and reports failed or cancelled saves honestly', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(render('a')); });
  const before = writes.length;
  let resolve!: () => void;
  pickerGate = new Promise(done => { resolve = done; });
  await act(async () => { exportButton(root).props.onClick(); });
  await act(async () => { root.update(render('b')); resolve(); });
  expect(writes.length).toBe(before);
  pickerGate = null;
  await act(async () => root.unmount());
  await act(async () => { root = create(render('c')); });
  chosen = null; closed = false;
  const successCount = successes.length;
  await act(async () => { exportButton(root).props.onClick(); });
  expect(writes.length).toBe(before); expect(closed).toBe(false); expect(successes.length).toBe(successCount);
  chosen = '/synthetic/chosen.md'; failure = true;
  await act(async () => { exportButton(root).props.onClick(); });
  expect(successes.length).toBe(successCount); expect(errors.at(-1)).toContain('write denied');
  failure = false; await act(async () => root.unmount());
});
