import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
const events = new Map<string, () => void>();
Object.assign(globalThis, { localStorage: {
  get length() { return storage.size; },
  key: (index: number) => [...storage.keys()][index] ?? null,
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
}, window: {
  addEventListener: (name: string, fn: () => void) => events.set(name, fn),
  removeEventListener: (name: string) => events.delete(name),
} });
let saved: any = null;
let fail = false;
let editorMounts = 0;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'get_meeting_folder_path') return '/synthetic/ui';
  if (command === 'api_notes_recording_target') return 'recording:/synthetic/ui';
  if (command === 'api_read_notes_document') return saved;
  if (command === 'api_write_notes_document') {
    if (fail) throw new Error('disk full');
    saved = { ...args, revision: args.expectedRevision + 1 };
    return saved.revision;
  }
  throw new Error(command);
} }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: (props: any) => {
  React.useEffect(() => { editorMounts++; }, []);
  return <textarea aria-label="Notes" defaultValue={props.initialMarkdown || ''} onChange={event => {
    const markdown = event.target.value;
    props.onDraft({ markdown, json: [{ type: 'paragraph', content: markdown }] }, Promise.resolve(markdown));
  }} />;
} }));
mock.module('@/components/chat/ChatThread', () => ({ ChatThread: () => <div>Ask AI</div> }));
mock.module('@/hooks/useUserName', () => ({ useUserName: () => 'User' }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
mock.module('@/components/ui/tabs', () => {
  const Wrapper = ({ children }: any) => <div>{children}</div>;
  return { Tabs: Wrapper, TabsContent: Wrapper, TabsList: Wrapper, TabsTrigger: Wrapper };
});
const { LivePanel } = await import('../../src/components/recording/LivePanel');
const { MeetingNotesEditor } = await import('../../src/components/meeting/MeetingNotesEditor');
const { flushNotes } = await import('../../src/lib/notes-store');
const live = (lines: any[] = []) => <LivePanel tab="notes" onTabChange={() => {}} speakers={[]} lines={lines} sessionKey="session-ui" onIdentify={() => {}} onMarkMe={() => {}} onJumpTo={() => {}} />;

test('normal live Notes entrypoint survives incoming phrases and navigation, exposes failure and retry, then library edits', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(live()); });
  expect(root.root.findByProps({ 'aria-label': 'Notes' })).toBeDefined();
  const mounts = editorMounts;
  fail = true;
  await act(async () => {
    root.root.findByType('textarea').props.onChange({ target: { value: 'My idea, not speech' } });
  });
  await act(async () => { root.update(live([{ id: 'asr-1', text: 'New incoming phrase', timestamp: '00:01', speaker: 'You' }])); });
  expect(editorMounts).toBe(mounts);
  expect([...storage.values()].join('\n')).toContain('My idea, not speech');
  await act(async () => { await flushNotes('recording:/synthetic/ui'); });
  expect(JSON.stringify(root.toJSON())).toContain('disk full');
  expect(root.root.findAllByType('button').some(button => button.children.includes('Retry'))).toBe(true);
  fail = false;
  await act(async () => { await flushNotes('recording:/synthetic/ui'); });
  expect(saved.markdown).toBe('My idea, not speech');
  await act(async () => { root.unmount(); });
  await act(async () => { root = create(<MeetingNotesEditor target="meeting:ui" />); });
  expect(root.root.findByType('textarea').props.defaultValue).toBe('My idea, not speech');
  await act(async () => { root.root.findByType('textarea').props.onChange({ target: { value: 'Edited in library' } }); });
  await act(async () => { root.unmount(); });
  expect(saved.markdown).toBe('Edited in library');
});

test('library projection-only updates keep the dirty Notes editor mounted before debounce', async () => {
  // This is #12's shared-editor seam, not #14's version-selector implementation.
  // The combined branch must drive the actual selection/correction controls.
  function Projection({ version, transcript, summary }: { version: string; transcript: string; summary: string }) {
    return <div>
      <div data-version={version}>{transcript}</div>
      <div>{summary}</div>
      <MeetingNotesEditor key="same-meeting" target="meeting:projection-ui" />
    </div>;
  }
  const exact = 'Notes RU/EN: не обещание\n- API follow-up';
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<Projection version="live" transcript="old phrase" summary="old summary" />); });
  const mounts = editorMounts;
  await act(async () => { root.root.findByType('textarea').props.onChange({ target: { value: exact } }); });
  // No sleep or timer advancement: both projection replacements occur in the
  // same turn as typing, inside the debounce window rather than after a save.
  await act(async () => { root.update(<Projection version="refined" transcript="refined phrase" summary="refetched summary" />); });
  await act(async () => { root.update(<Projection version="refined" transcript="corrected phrase" summary="summary refreshed again" />); });
  expect(editorMounts).toBe(mounts);
  const journal = JSON.parse([...storage].find(([key]) => key.startsWith(`af-notes-v2:draft:${encodeURIComponent('meeting:projection-ui')}:`))![1]);
  expect(journal.content.markdown).toBe(exact);
  expect(journal.content.json).toEqual([{ type: 'paragraph', content: exact }]);
  await act(async () => { await flushNotes('meeting:projection-ui'); });
  expect(saved.markdown).toBe(exact);
  expect(saved.json).toEqual(journal.content.json);
  await act(async () => { root.unmount(); });
  await act(async () => { root = create(<Projection version="refined" transcript="corrected phrase" summary="reopened summary" />); });
  expect(root.root.findByType('textarea').props.defaultValue).toBe(exact);
  await act(async () => { root.unmount(); });
});


test('normal Notes recovery offers independent journals without silently choosing or deleting one', async () => {
  saved = null;
  storage.set('af-notes-v1:meeting:recovery-ui', JSON.stringify({ content: { markdown: 'draft A', json: [{ content: 'A exact' }] }, revision: 0, converting: true }));
  storage.set('af-notes-v1:recording:/synthetic/ui', JSON.stringify({ content: { markdown: 'draft B', json: [{ content: 'B exact' }] }, revision: 0, converting: true }));
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<MeetingNotesEditor target="meeting:recovery-ui" />); });
  expect(root.root.findAllByType('textarea')).toHaveLength(0);
  expect(JSON.stringify(root.toJSON())).toContain('Multiple Notes recovery drafts');
  expect(JSON.stringify(root.toJSON())).toContain('B exact');
  const choices = root.root.findAllByType('button').filter(button => button.children.includes('Open this draft (retain the others)'));
  expect(choices).toHaveLength(2);
  await act(async () => { choices[0].props.onClick(); });
  expect(root.root.findByType('textarea').props.defaultValue).toBe('draft A');
  expect(storage.get('af-notes-v1:recording:/synthetic/ui')).toContain('B exact');
  await act(async () => { root.unmount(); });
});
