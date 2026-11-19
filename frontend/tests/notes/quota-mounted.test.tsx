import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, test } from 'bun:test';

const rows = new Map<string, string>();
const json = [{ id: 'retained', type: 'paragraph', props: {}, content: [{ type: 'text', text: 'Точная rich idea', styles: { bold: true } }], children: [] }];
const markdown = '**Точная rich idea**';
let writes = 0;
let setAttempts = 0;
let native: any = null;
let failNative = false;
let copied = '';
let convert: ((value: string) => void) | null = null;
let holdConversion = false;
const converted: unknown[] = [];
const ready: Array<() => void> = [];
Object.assign(globalThis, {
  localStorage: {
    get length() { return rows.size; }, key: (index: number) => [...rows.keys()][index] ?? null,
    getItem: (key: string) => rows.get(key) ?? null,
    setItem: () => { setAttempts++; throw new DOMException('No additional storage', 'QuotaExceededError'); },
    removeItem: (key: string) => rows.delete(key),
  },
  window: { setTimeout: (fn: () => void) => { ready.push(fn); return 0; }, addEventListener: () => {}, removeEventListener: () => {} },
  navigator: { clipboard: { writeText: async (value: string) => { copied = value; } } },
});
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_notes_recording_target') return 'recording:/synthetic/quota-mounted';
  if (command === 'api_read_notes_document') return native;
  if (command === 'api_write_notes_document') {
    writes++;
    if (failNative) throw new Error('SQLite unavailable');
    if ((native?.revision ?? 0) !== args.expectedRevision) throw new Error('Notes conflict');
    native = { ...args, revision: args.expectedRevision + 1 }; return native.revision;
  }
  throw new Error(command);
} }));
const editor = {
  document: json, onChange: () => () => {},
  blocksToMarkdownLossy: async (blocks: typeof json) => {
    converted.push(structuredClone(blocks));
    if (holdConversion) return new Promise<string>(resolve => { convert = resolve; });
    // The production NotesEditor passes the actual rich blocks to conversion.
    const text = blocks[0].content[0];
    return text.styles.bold ? `**${text.text}**` : text.text;
  },
};
mock.module('@blocknote/react', () => ({ useCreateBlockNote: () => editor, GridSuggestionMenuController: () => null, SuggestionMenuController: () => null }));
mock.module('@blocknote/shadcn', () => ({ BlockNoteView: ({ editable }: any) => <div data-notes-editor="mounted" data-editable={editable} /> }));
mock.module('@/lib/app-theme', () => ({ useAppTheme: () => ['light'], themeInfo: () => ({ dark: false }) }));
mock.module('@/components/editor/blocknote-menus', () => ({ useBlockNoteLayer: () => null, BlockNoteLayerProvider: ({ children }: any) => children, blockNoteMenus: {} }));
const { MeetingNotesEditor } = await import('../../src/components/meeting/MeetingNotesEditor');
const { getNotesSnapshot } = await import('../../src/lib/notes-store');
const button = (root: ReturnType<typeof create>, name: string) => root.root.findAllByType('button').find(button => button.children.includes(name))!;
const prepareEditor = async () => { await act(async () => { for (const callback of ready.splice(0)) callback(); }); };
const seed = (target: string, converting: boolean, alias = false) => {
  const source = alias ? 'recording:/synthetic/quota-mounted' : target;
  const key = `af-notes-v2:draft:${encodeURIComponent(source)}:original`;
  const raw = JSON.stringify({ target: source, content: { markdown: converting ? 'STALE TEXT' : markdown, json }, revision: 0, converting });
  rows.set(key, raw); return { key, raw };
};
beforeEach(() => {
  rows.clear(); ready.splice(0); converted.splice(0); writes = 0; setAttempts = 0;
  native = null; failNative = false; copied = ''; holdConversion = false; convert = null;
});

for (const pending of [false, true]) {
  test(`actual mounted editor recovers exact rich Notes with no added Storage capacity (pending=${pending})`, async () => {
    const target = `meeting:quota-mounted-${pending}`;
    const source = seed(target, pending, pending);
    let root!: ReturnType<typeof create>;
    await act(async () => { root = create(<MeetingNotesEditor target={target} />); });
    expect(root.root.findAllByProps({ 'data-notes-editor': 'mounted' })).toHaveLength(1);
    expect(root.root.findByProps({ 'data-notes-editor': 'mounted' }).props['data-editable']).toBe(false);
    expect(getNotesSnapshot(target).loaded).toBe(true);
    expect(writes).toBe(0); expect(rows.get(source.key)).toBe(source.raw);
    expect(root.root.findByProps({ 'aria-label': 'Recovery JSON (exact blocks)' }).props.value).toBe(JSON.stringify(json, null, 2));
    await act(async () => { button(root, 'Copy recovery JSON').props.onClick(); });
    expect(copied).toBe(JSON.stringify(json, null, 2));
    await prepareEditor();
    expect(converted[0]).toEqual(json);
    expect(root.root.findByProps({ 'aria-label': 'Recovery text' }).props.value).toBe(markdown);
    await act(async () => { button(root, 'Copy recovery text').props.onClick(); });
    expect(copied).toBe(markdown);
    expect(setAttempts).toBe(1); // Failed copy only; reconversion allocates no journal.
    await act(async () => { button(root, 'Save retained Notes to meeting').props.onClick(); });
    expect(writes).toBe(1); expect(native.markdown).toBe(markdown); expect(native.json).toEqual(json);
    expect(rows.has(source.key)).toBe(false);
    expect(getNotesSnapshot(target).state).toBe('saved');
    expect(root.root.findByProps({ 'data-notes-editor': 'mounted' }).props['data-editable']).toBe(false);
    await act(async () => { root.unmount(); });
    await act(async () => { root = create(<MeetingNotesEditor target={target} />); });
    await prepareEditor();
    expect(getNotesSnapshot(target).content.json).toEqual(json);
    expect(getNotesSnapshot(target).content.markdown).toBe(markdown);
    expect(writes).toBe(1);
    await act(async () => { root.unmount(); });
  });
}

test('actual editor keeps failed recovery readable on reopen and explicit Retry succeeds', async () => {
  const target = 'meeting:quota-native-error'; const source = seed(target, false);
  let root!: ReturnType<typeof create>;
  await act(async () => { root = create(<MeetingNotesEditor target={target} />); }); await prepareEditor();
  failNative = true;
  await act(async () => { button(root, 'Save retained Notes to meeting').props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('SQLite unavailable'); expect(rows.get(source.key)).toBe(source.raw);
  await act(async () => { root.unmount(); });
  await act(async () => { root = create(<MeetingNotesEditor target={target} />); }); await prepareEditor();
  expect(root.root.findByProps({ 'aria-label': 'Recovery JSON (exact blocks)' }).props.value).toBe(JSON.stringify(json, null, 2));
  failNative = false;
  await act(async () => { button(root, 'Save retained Notes to meeting').props.onClick(); });
  expect(native.markdown).toBe(markdown); expect(rows.has(source.key)).toBe(false);
  await act(async () => { root.unmount(); });
});

test('cancel during actual JSON reconversion never saves, and Resume remounts read-only for explicit save', async () => {
  const target = 'meeting:quota-cancel'; const source = seed(target, true);
  holdConversion = true;
  let root!: ReturnType<typeof create>;
  await act(async () => { root = create(<MeetingNotesEditor target={target} />); }); await prepareEditor();
  expect(button(root, 'Save retained Notes to meeting').props.disabled).toBe(true);
  await act(async () => { button(root, 'Cancel recovery (keep original)').props.onClick(); });
  await act(async () => { convert!(markdown); });
  expect(writes).toBe(0); expect(rows.get(source.key)).toBe(source.raw);
  expect(getNotesSnapshot(target).recoveryPaused).toBe(true);
  holdConversion = false;
  await act(async () => { button(root, 'Resume recovery').props.onClick(); }); await prepareEditor();
  expect(root.root.findByProps({ 'data-notes-editor': 'mounted' }).props['data-editable']).toBe(false);
  await act(async () => { button(root, 'Save retained Notes to meeting').props.onClick(); });
  expect(native.markdown).toBe(markdown); expect(native.json).toEqual(json);
  await act(async () => { root.unmount(); });
});

test('quota recovery choice and native conflict review retain the other writer through the actual editor', async () => {
  const target = 'meeting:quota-multiple'; const source = seed(target, false);
  const otherKey = `af-notes-v2:draft:${encodeURIComponent(target)}:other`;
  const otherRaw = JSON.stringify({ target, content: { markdown: 'Independent draft', json: [] }, revision: 0, converting: false });
  rows.set(otherKey, otherRaw);
  let root!: ReturnType<typeof create>;
  await act(async () => { root = create(<MeetingNotesEditor target={target} />); });
  expect(root.root.findAllByProps({ 'data-notes-editor': 'mounted' })).toHaveLength(0);
  const choices = root.root.findAllByType('button').filter(button => button.children.includes('Open this draft (retain the others)'));
  expect(choices).toHaveLength(2);
  await act(async () => { choices[0].props.onClick(); }); await prepareEditor();
  expect(root.root.findByProps({ 'data-notes-editor': 'mounted' }).props['data-editable']).toBe(false);
  native = { markdown: 'Another saved version', json: [], revision: 1 };
  await act(async () => { button(root, 'Save retained Notes to meeting').props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('Notes conflict');
  expect(rows.get(source.key)).toBe(source.raw); expect(rows.get(otherKey)).toBe(otherRaw);
  await act(async () => { button(root, 'Review saved version').props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('Another saved version');
  await act(async () => { button(root, 'Replace reviewed version with my draft').props.onClick(); });
  expect(native.markdown).toBe(markdown); expect(native.json).toEqual(json);
  expect(rows.has(source.key)).toBe(false); expect(rows.get(otherKey)).toBe(otherRaw);
  await act(async () => { root.unmount(); });
});
