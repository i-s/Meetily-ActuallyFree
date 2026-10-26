import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let changed: (() => void) | undefined;
let convert!: (value: string) => void;
const editor = { document: [{ id: 'one', type: 'paragraph', content: [{ type: 'text', text: 'latest' }] }], onChange: (callback: () => void) => { changed = callback; return () => {}; }, blocksToMarkdownLossy: () => new Promise<string>(resolve => { convert = resolve; }) };
Object.assign(globalThis, { window: { setTimeout } });
mock.module('@blocknote/react', () => ({ useCreateBlockNote: () => editor, GridSuggestionMenuController: () => null, SuggestionMenuController: () => null }));
mock.module('@blocknote/shadcn', () => ({ BlockNoteView: () => null }));
mock.module('@/lib/app-theme', () => ({ useAppTheme: () => ['light'], themeInfo: () => ({ dark: false }) }));
mock.module('@/components/editor/blocknote-menus', () => ({ useBlockNoteLayer: () => null, BlockNoteLayerProvider: ({children}: any) => children, blockNoteMenus: {} }));
const { NotesEditor } = await import('../../src/components/editor/NotesEditor');

test('publishes exact JSON synchronously before asynchronous markdown finishes', async () => {
  const drafts: any[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<NotesEditor onDraft={(content: any, markdown: any) => drafts.push({ content, markdown })} />); await new Promise(resolve => setTimeout(resolve, 100)); });
  drafts.length = 0;
  act(() => changed!());
  expect(drafts).toHaveLength(1);
  expect(drafts[0].content.json[0].content[0].text).toBe('latest');
  convert('latest');
  expect(await drafts[0].markdown).toBe('latest');
  act(() => view!.unmount());
});
