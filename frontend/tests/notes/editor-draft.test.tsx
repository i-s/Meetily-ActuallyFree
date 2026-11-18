import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let changed: (() => void) | undefined;
let convert!: (value: string) => void;
const editor = { document: [{ id: 'one', type: 'paragraph', content: [{ type: 'text', text: 'latest' }] }], onChange: (callback: () => void) => { changed = callback; return () => {}; }, blocksToMarkdownLossy: () => new Promise<string>(resolve => { convert = resolve; }) };
let ready!: () => void;
Object.assign(globalThis, { window: { setTimeout: (callback: () => void) => { ready = callback; return 0; } } });
mock.module('@blocknote/react', () => ({ useCreateBlockNote: () => editor, GridSuggestionMenuController: () => null, SuggestionMenuController: () => null }));
mock.module('@blocknote/shadcn', () => ({ BlockNoteView: () => null }));
mock.module('@/lib/app-theme', () => ({ useAppTheme: () => ['light'], themeInfo: () => ({ dark: false }) }));
mock.module('@/components/editor/blocknote-menus', () => ({ useBlockNoteLayer: () => null, BlockNoteLayerProvider: ({children}: any) => children, blockNoteMenus: {} }));
const { NotesEditor } = await import('../../src/components/editor/NotesEditor');

test('publishes exact JSON synchronously before asynchronous markdown finishes', async () => {
  const drafts: any[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<NotesEditor onDraft={(content: any, markdown: any) => drafts.push({ content, markdown })} />); });
  await act(async () => ready());
  drafts.length = 0;
  act(() => changed!());
  expect(drafts).toHaveLength(1);
  expect(drafts[0].content.json[0].content[0].text).toBe('latest');
  convert('latest');
  expect(await drafts[0].markdown).toBe('latest');
  act(() => view!.unmount());
});

test('flush waits for the newest conversion across editable transitions and ignores older completion', async () => {
  const pending: Array<(value: string) => void> = [];
  const previous = editor.blocksToMarkdownLossy;
  editor.blocksToMarkdownLossy = () => new Promise<string>(resolve => pending.push(resolve));
  const updates: string[] = [];
  let flush!: () => Promise<void>;
  const register = (value: () => Promise<void>) => { flush = value; return () => {}; };
  const onChange = (content: { markdown: string }) => updates.push(content.markdown);
  let root!: ReturnType<typeof create>;
  try {
    await act(async () => { root = create(<NotesEditor onChange={onChange} registerFlush={register} />); });
    await act(async () => ready());
    act(() => changed!());
    await act(async () => root.update(<NotesEditor editable={false} onChange={onChange} registerFlush={register} />));
    await act(async () => root.update(<NotesEditor editable onChange={onChange} registerFlush={register} />));
    act(() => changed!());
    let finished = false;
    const barrier = flush().then(() => { finished = true; });
    await Promise.resolve(); expect(finished).toBe(false);
    await act(async () => { pending[1]('new RU/EN formatting'); await barrier; });
    expect(updates).toEqual(['new RU/EN formatting']);
    await act(async () => { pending[0]('old conversion'); });
    expect(updates).toEqual(['new RU/EN formatting']);
  } finally { act(() => root?.unmount()); editor.blocksToMarkdownLossy = previous; }
});
