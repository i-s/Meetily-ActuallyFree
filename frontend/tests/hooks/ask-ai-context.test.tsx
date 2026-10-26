import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const target = new EventTarget();
Object.assign(globalThis, { window: target });
const pending: {id: string, resolve: (value: unknown) => void}[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: (_: string, args: any) => new Promise(resolve => pending.push({id: args.meetingId, resolve})) }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
const { useAskAiContext } = await import('../../src/hooks/useAskAiContext');
let state: ReturnType<typeof useAskAiContext>;
function View({id}: {id: string}) { state = useAskAiContext(id, '', false); return null; }

test('late preview for the previous meeting cannot replace current context', async () => {
  let view: ReturnType<typeof create>;
  act(() => { view = create(<View id="a" />); });
  act(() => { view!.update(<View id="b" />); });
  expect(pending.map(p => p.id)).toEqual(['a', 'b']);
  await act(async () => { pending[1].resolve({ startTranscriptId: 'b-row' }); });
  await act(async () => { pending[0].resolve({ startTranscriptId: 'a-row' }); });
  expect(state!.range?.startTranscriptId).toBe('b-row');
  act(() => { window.dispatchEvent(new Event('af-ask-ai-context-settings-changed')); });
  expect(pending.at(-1)?.id).toBe('b');
  act(() => view!.unmount());
});
