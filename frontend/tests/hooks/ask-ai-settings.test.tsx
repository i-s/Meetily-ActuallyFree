import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
Object.assign(globalThis, { window: new EventTarget() });
const saved: any[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'get_ask_ai_context_settings') return { mode: 'recent', maxChars: 10000 };
  saved.push(args.settings); return args.settings;
} }));
const { AskAiContextSettings } = await import('../../src/components/AskAiContextSettings');

test('settings default to 10000 and save whole-transcript mode without a transcript limit', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<AskAiContextSettings />); });
  expect(view!.root.findByType('input').props.value).toBe('10000');
  act(() => view!.root.findByType('select').props.onChange({ target: { value: 'all' } }));
  expect(view!.root.findByType('input').props.disabled).toBe(true);
  await act(async () => { await view!.root.findByType('form').props.onSubmit({ preventDefault() {} }); });
  expect(saved).toEqual([{ mode: 'all', maxChars: 10000 }]);
  expect(JSON.stringify(view!.toJSON())).toContain('Meeting notes всегда передаются целиком');
  act(() => view!.unmount());
});
