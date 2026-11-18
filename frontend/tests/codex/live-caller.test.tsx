import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const stored = new Map<string, string>();
const storage = { get length() { return stored.size; }, key: (i: number) => [...stored.keys()][i] ?? null, getItem: (k: string) => stored.get(k) ?? null, setItem: (k: string, v: string) => stored.set(k, v), removeItem: (k: string) => stored.delete(k) };
Object.assign(globalThis, { localStorage: storage, sessionStorage: storage, window: { localStorage: storage, sessionStorage: storage, addEventListener() {}, removeEventListener() {} } });
let folder = '/owned/A';
let saved = { source: 'user_notes', markdown: 'Personal: maybe 900, not agreed', json: null, revision: 3 };
const calls: Array<{ command: string; args: any }> = [];
let resolveAnswer: ((v: any) => void) | null = null;
let hold = false;
let holdCutoff = false;
let resolveCutoff: ((v: any) => void) | null = null;
let failure: string | null = null;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push({ command, args });
  if (command === 'get_meeting_folder_path') return folder;
  if (command === 'api_read_notes_document') return structuredClone(saved);
  if (command === 'api_write_notes_document') { saved = { ...saved, markdown: args.markdown, json: args.json, revision: args.expectedRevision + 1 }; return saved.revision; }
  if (command === 'api_notes_recording_target') return null;
  if (command === 'codex_live_cancel') return;
  if (command === 'codex_live_cutoff') return holdCutoff ? new Promise(resolve => { resolveCutoff = resolve; }) : { target: `recording:${folder}`, asOfSeconds: 600 };
  if (command === 'codex_live_ask') {
    if (failure) throw new Error(failure);
    const result = { answer: 'Speech says 42; Notes are only a personal thought. Fake [99:59] is not a citation. [external](https://invalid.invalid) ![remote](https://invalid.invalid/image.png)', evidence: args.request.lines.slice(0, 1).map((l: any) => ({ id: l.id, sequenceId: l.sequenceId, time: l.time, text: l.text })), uncertain: false, notesRevision: args.request.userNotes.revision, asOfSeconds: args.request.asOfSeconds, fromSeconds: args.request.fromSeconds };
    return hold ? new Promise(resolve => { resolveAnswer = resolve; }) : result;
  }
  throw new Error(`Forbidden fallback or unexpected command: ${command}`);
} }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: (props: any) => <textarea aria-label="Live Notes" defaultValue={props.initialMarkdown} onChange={e => props.onDraft({ markdown: e.target.value, json: null }, Promise.resolve(e.target.value))} /> }));
mock.module('@/hooks/useUserName', () => ({ useUserName: () => 'User' }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
mock.module('@/components/ui/tabs', () => { const W = ({ children }: any) => <div>{children}</div>; return { Tabs: W, TabsList: W, TabsContent: W, TabsTrigger: W }; });
const { LivePanel } = await import('../../src/components/recording/LivePanel');
const { loadNotes, editNotes, flushNotes } = await import('../../src/lib/notes-store');
const lines = [
  { id: 'early', sequenceId: 1, time: 12, endTime: 15, text: 'Budget 42', speaker: 'Speaker 1' },
  { id: 'recent', sequenceId: 2, time: 599, endTime: 600, text: 'Recent speech' },
];
let jumped = '';
const panel = (key = 'session-A', currentLines = lines) => <LivePanel {...({ tab: 'ask', onTabChange() {}, speakers: [], lines: currentLines, sessionKey: key, asOfSeconds: 600, onIdentify() {}, onMarkMe() {}, onJumpTo: (id: string) => { jumped = id; } } as any)} />;
const button = (r: ReactTestRenderer, label: string) => r.root.findAllByType('button').find(b => b.children.includes(label))!;

test('normal live caller sends only subscription snapshot, returns verified clickable speech evidence and distinct revisioned Notes', async () => {
  await loadNotes('recording:/owned/A');
  let root!: ReactTestRenderer;
  await act(async () => { root = create(panel()); });
  const input = root.root.findAllByType('textarea').find(t => t.props.placeholder === 'Ask about the call…')!;
  await act(async () => { input.props.onChange({ target: { value: 'What budget was discussed?', style: {}, scrollHeight: 20 } }); });
  await act(async () => { root.root.findByType('form').props.onSubmit({ preventDefault() {} }); });
  const request = calls.find(c => c.command === 'codex_live_ask')!.args.request;
  expect(request.target).toBe('recording:/owned/A');
  expect(request.lines.map((l: any) => l.id)).toEqual(['early', 'recent']);
  expect(request.userNotes).toEqual({ source: 'user_notes', markdown: saved.markdown, revision: 3 });
  expect(calls.some(c => c.command === 'ask_live_assistant')).toBe(false);
  expect(JSON.stringify(root.toJSON())).toContain('Personal Notes · revision 3');
  const evidence = root.root.findByProps({ 'aria-label': 'Jump to speech at 0:12' });
  act(() => evidence.props.onClick()); expect(jumped).toBe('early');
  await act(async () => { root.update(panel('session-A', lines.map(line => ({ ...line, id: `resynced-${line.id}` })))); });
  act(() => root.root.findByProps({ 'aria-label': 'Jump to speech at 0:12' }).props.onClick());
  expect(jumped).toBe('resynced-early');
  expect(root.root.findAllByProps({ 'aria-label': 'Jump to speech at 99:59' })).toHaveLength(0);
  expect(root.root.findAllByType('a')).toHaveLength(0);
  expect(root.root.findAllByType('img')).toHaveLength(0);
  await act(async () => { root.unmount(); });
});
test('dedicated five-minute action uses immutable click cutoff and cancels late response on meeting switch', async () => {
  calls.length = 0; hold = true;
  let root!: ReactTestRenderer;
  await act(async () => { root = create(panel()); });
  await act(async () => { button(root, 'Что я пропустил?').props.onClick(); });
  const call = calls.find(c => c.command === 'codex_live_ask')!;
  expect(call.args.request.lines.map((l: any) => l.id)).toEqual(['recent']);
  expect(call.args.request.fromSeconds).toBe(300);
  await act(async () => { root.update(panel('session-A', [...lines, { id: 'delayed', sequenceId: 3, time: 400, endTime: 401, text: 'Delayed after click' }])); });
  expect(call.args.request.lines).toHaveLength(1);
  await act(async () => { root.root.findByProps({ 'aria-label': 'Live Notes' }).props.onChange({ target: { value: 'Notes still editable while CLI waits' } }); });
  await act(async () => { await flushNotes('recording:/owned/A'); });
  expect(saved.markdown).toBe('Notes still editable while CLI waits');
  expect(call.args.request.userNotes.markdown).toBe('Personal: maybe 900, not agreed');
  folder = '/owned/B';
  await act(async () => { root.update(panel('session-B', [])); });
  expect(calls.some(c => c.command === 'codex_live_cancel' && c.args.requestId === call.args.requestId)).toBe(true);
  await act(async () => { resolveAnswer?.({ answer: 'SECRET A LATE ANSWER', evidence: [], notesRevision: null, uncertain: true, asOfSeconds: 600, fromSeconds: 300 }); });
  expect(JSON.stringify(root.toJSON())).not.toContain('SECRET A LATE ANSWER');
  await act(async () => { root.unmount(); }); hold = false; folder = '/owned/A';
});

test('cutoff await does not admit delayed ASR and manual cancellation rejects a late result', async () => {
  calls.length = 0; hold = true; holdCutoff = true;
  let root!: ReactTestRenderer;
  await act(async () => { root = create(panel('cutoff-await')); });
  await act(async () => { button(root, 'Что я пропустил?').props.onClick(); });
  expect(calls.some(c => c.command === 'codex_live_ask')).toBe(false);
  await act(async () => { root.update(panel('cutoff-await', [...lines, { id: 'late-during-cutoff', sequenceId: 4, time: 450, endTime: 451, text: 'Late ASR while cutoff IPC awaits' }])); });
  await act(async () => { resolveCutoff?.({ target: 'recording:/owned/A', asOfSeconds: 600 }); });
  const request = calls.find(c => c.command === 'codex_live_ask')!;
  expect(request.args.request.lines.map((l: any) => l.id)).toEqual(['recent']);
  await act(async () => { button(root, 'Cancel request').props.onClick(); });
  expect(calls.some(c => c.command === 'codex_live_cancel' && c.args.requestId === request.args.requestId)).toBe(true);
  expect(JSON.stringify(root.toJSON())).toContain('Request cancelled');
  await act(async () => { resolveAnswer?.({ answer: 'LATE AFTER CANCEL', evidence: [], notesRevision: null, uncertain: true, asOfSeconds: 600, fromSeconds: 300 }); });
  expect(JSON.stringify(root.toJSON())).not.toContain('LATE AFTER CANCEL');
  await act(async () => { root.unmount(); }); hold = false; holdCutoff = false;
});
test('sixty-second UI deadline cancels the native ID and network errors remain visible without provider fallback', async () => {
  calls.length = 0; hold = true;
  const original = globalThis.setTimeout;
  let expire!: () => void;
  globalThis.setTimeout = ((fn: (...args: any[]) => void, delay?: number, ...args: any[]) => {
    if (delay === 60_000) expire = () => fn(...args);
    return original(fn, delay, ...args);
  }) as typeof setTimeout;
  let root!: ReactTestRenderer;
  try {
    await act(async () => { root = create(panel('deadline')); });
    await act(async () => { button(root, 'Что я пропустил?').props.onClick(); });
    await act(async () => { expire(); });
    expect(JSON.stringify(root.toJSON())).toContain('timed out after 60 seconds');
    expect(calls.some(c => c.command === 'codex_live_cancel')).toBe(true);
    await act(async () => { root.unmount(); });
  } finally { globalThis.setTimeout = original; hold = false; }
  failure = 'Codex network connection failed. Recording, local ASR and Notes continue.';
  await act(async () => { root = create(panel('offline')); });
  await act(async () => { button(root, 'Что я пропустил?').props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('network connection failed');
  expect(calls.some(c => c.command === 'ask_live_assistant' || c.command === 'stop_recording')).toBe(false);
  await act(async () => { root.unmount(); }); failure = null;
});
test('pending Notes are reported instead of sending stale Markdown to CLI', async () => {
  calls.length = 0;
  let root!: ReactTestRenderer;
  await act(async () => { root = create(panel('pending-session')); });
  act(() => editNotes('recording:/owned/A', { markdown: 'old complete', json: [{ type: 'paragraph', content: 'pending exact rich text' }] }, new Promise<string>(() => {})));
  await act(async () => { button(root, 'Что я пропустил?').props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('Notes');
  expect(JSON.stringify(root.toJSON())).toContain('pending');
  expect(calls.some(c => c.command === 'codex_live_ask')).toBe(false);
  // Leave the intentionally pending conversion uncommitted; no background native write.
  await act(async () => { root.unmount(); });
});
