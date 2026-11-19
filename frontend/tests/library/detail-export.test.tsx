import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const rows = new Map<string, string>();
Object.assign(globalThis, { localStorage: {
  get length() { return rows.size; }, key: (i: number) => [...rows.keys()][i] ?? null,
  getItem: (key: string) => rows.get(key) ?? null, setItem: (key: string, value: string) => rows.set(key, value), removeItem: (key: string) => rows.delete(key),
}, window: { addEventListener() {}, removeEventListener() {}, atob: globalThis.atob, btoa: globalThis.btoa } });
const docs = new Map<string, any>();
const errors: string[] = [];
const successes: string[] = [];
let writes: Array<{ path: string; text: string }> = [];
let snapshotGate: (() => Promise<void>) | null = null;
let pickerGate: (() => Promise<void>) | null = null;
let failedWrite = false;
let pickerCalls = 0;
let lastArgs: any;
const snapshot = (id: string) => ({ id, title: `Встреча ${id}`, createdAt: '2026-11-18T10:00:00Z', capturedAt: '2026-11-18T11:00:00Z', audioAvailable: false, notes: { source: 'user_notes', markdown: docs.get(`meeting:${id}`)?.markdown ?? '', revision: docs.get(`meeting:${id}`)?.revision ?? 0 }, summary: null, summaryStatus: null, transcripts: [], actions: [] });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_notes_recording_target') return null;
  if (command === 'api_read_notes_document') return structuredClone(docs.get(args.target) ?? null);
  if (command === 'api_write_notes_document') {
    const old = docs.get(args.target);
    if ((old?.revision ?? 0) !== args.expectedRevision) throw new Error('Notes conflict');
    docs.set(args.target, { markdown: args.markdown, json: args.json, revision: args.expectedRevision + 1 });
    return args.expectedRevision + 1;
  }
  if (command === 'api_export_meeting_snapshot') {
    lastArgs = args;
    const captured = snapshot(args.meetingId);
    if (snapshotGate) await snapshotGate();
    if (captured.notes.revision !== args.expectedNotesRevision) throw new Error('Notes changed');
    return captured;
  }
  throw new Error(command);
} }));
mock.module('@tauri-apps/plugin-dialog', () => ({ save: async () => { pickerCalls++; if (pickerGate) await pickerGate(); return '/synthetic/выбрано.custom'; } }));
mock.module('@tauri-apps/plugin-fs', () => ({ writeFile: async (path: string, bytes: Uint8Array) => {
  if (failedWrite) throw new Error('disk full');
  writes.push({ path, text: new TextDecoder().decode(bytes) });
} }));
mock.module('sonner', () => ({ toast: { error: (text: string, extra?: any) => errors.push(`${text}: ${extra?.description ?? ''}`), success: (text: string) => successes.push(text) } }));
const Wrap = ({ children }: any) => <div>{children}</div>;
mock.module('@/components/ui/dialog', () => ({ Dialog: Wrap, DialogContent: Wrap, DialogDescription: Wrap, DialogFooter: Wrap, DialogHeader: Wrap, DialogTitle: Wrap }));
mock.module('@/components/ui/button', () => ({ Button: ({ children, loading, ...props }: any) => <button {...props}>{children}</button> }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: (props: any) => <textarea aria-label="Notes" defaultValue={props.initialMarkdown ?? ''} onChange={(event: any) => props.onDraft({ markdown: event.target.value, json: [{ type: 'paragraph', content: event.target.value }] }, Promise.resolve(event.target.value))} /> }));
const { useCopyOperations } = await import('../../src/hooks/meeting-details/useCopyOperations');
const { MeetingExportDialog } = await import('../../src/components/MeetingDetails/MeetingExportDialog');
const { MeetingNotesEditor } = await import('../../src/components/meeting/MeetingNotesEditor');
const { flushNotes } = await import('../../src/lib/notes-store');
let operations: ReturnType<typeof useCopyOperations>;
function Harness({ id }: { id: string }) {
  operations = useCopyOperations({ meeting: { id, title: 'STALE TITLE', created_at: '2026-11-18' }, aiSummary: { markdown: 'STALE SUMMARY' } as any });
  return <><MeetingNotesEditor target={`meeting:${id}`} /><MeetingExportDialog open onOpenChange={() => {}} hasTranscript={false} hasSummary={false} onExport={operations.handleExportMeeting} /></>;
}
const text = (node: any): string => typeof node === 'string' ? node : (node?.children ?? []).map(text).join('');
const button = (root: ReactTestRenderer, label: string) => root.root.findAllByType('button').find(node => text(node) === label)!;
const deferred = () => { let resolve!: () => void; const promise = new Promise<void>(done => { resolve = done; }); return { promise, resolve }; };

test('ordinary detail dialog exports Notes without summary/audio and keeps exact chosen path', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<Harness id="notes-only" />); });
  await act(async () => { root.root.findByType('textarea').props.onChange({ target: { value: 'МыСлЬ не речь' } }); });
  await act(async () => { button(root, 'Choose format').props.onClick(); });
  await act(async () => { await button(root, 'MarkdownFormatted .md file').props.onClick(); });
  expect(writes.at(-1)?.path).toBe('/synthetic/выбрано.custom');
  expect(writes.at(-1)?.text).toContain('МыСлЬ не речь');
  expect(writes.at(-1)?.text).toContain('user_notes');
  expect(writes.at(-1)?.text).toContain('No summary');
  expect(writes.at(-1)?.text).not.toContain('STALE');
  expect(lastArgs.expectedNotesRevision).toBe(1);
  await act(async () => root.unmount());
});

test('detail navigation during native snapshot capture never opens picker or writes', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<Harness id="nav-a" />); });
  const before = writes.length, pickers = pickerCalls;
  const gate = deferred(); snapshotGate = () => gate.promise;
  let pending!: Promise<boolean>;
  await act(async () => { pending = operations.handleExportMeeting('all','markdown'); });
  await act(async () => { root.update(<Harness id="nav-b" />); gate.resolve(); await pending; });
  expect(await pending).toBe(false); expect(pickerCalls).toBe(pickers); expect(writes.length).toBe(before);
  snapshotGate = null; await act(async () => root.unmount());
});

test('detail navigation during native picker cancels writes to the old meeting destination', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<Harness id="picker-a" />); });
  const before = writes.length;
  const gate = deferred(); pickerGate = () => gate.promise;
  let pending!: Promise<boolean>;
  await act(async () => { pending = operations.handleExportMeeting('all','markdown'); });
  await act(async () => { root.update(<Harness id="picker-b" />); gate.resolve(); await pending; });
  expect(await pending).toBe(false); expect(writes.length).toBe(before);
  pickerGate = null; await act(async () => root.unmount());
});

test('production Notes edit racing snapshot is rejected, retry exports the committed revision', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<Harness id="race" />); });
  const before = writes.length;
  const gate = deferred(); snapshotGate = () => gate.promise;
  let pending!: Promise<boolean>;
  await act(async () => { pending = operations.handleExportMeeting('all','markdown'); });
  await act(async () => { root.root.findByType('textarea').props.onChange({ target: { value: 'Changed while capturing' } }); await flushNotes('meeting:race'); });
  await act(async () => { gate.resolve(); await pending; });
  expect(await pending).toBe(false); expect(writes.length).toBe(before); expect(errors.at(-1)).toContain('changed');
  snapshotGate = null;
  await act(async () => { expect(await operations.handleExportMeeting('all','markdown')).toBe(true); });
  expect(writes.at(-1)?.text).toContain('Changed while capturing');
  failedWrite = true;
  const count = successes.length;
  await act(async () => { expect(await operations.handleExportMeeting('all','markdown')).toBe(false); });
  expect(successes.length).toBe(count); expect(errors.at(-1)).toContain('disk full');
  failedWrite = false; await act(async () => root.unmount());
});
