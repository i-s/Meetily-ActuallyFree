import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, expect, mock, test } from 'bun:test';
let stored = 'Ёлка\nAPI', failRead = false, revision=2;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  if (name === 'api_get_whisper_vocabulary') { if(failRead) throw new Error('read unavailable'); return {global:stored,meeting:'',approvedGlobal:'',vocabularyRevision:revision}; }
  if (name === 'api_save_global_whisper_vocabulary') {
    if (args.expectedVocabulary !== stored || args.expectedRevision !== revision) throw new Error('Vocabulary changed in another window');
    stored = args.vocabulary; revision++; return stored;
  }
  throw new Error(name);
}}));
mock.module('@/components/ui/button', () => ({ Button: ({children,...props}:any) => <button {...props}>{children}</button> }));
const { PersonalVocabulary } = await import('../../src/components/PersonalVocabulary');
let root: ReactTestRenderer;
const button = (text: string) => root.root.findAllByType('button').find(n => String(n.props.children).includes(text))!;
afterEach(async () => { await act(async () => root?.unmount()); stored='Ёлка\nAPI'; failRead=false; revision=2; });
test('stale settings cannot erase concurrent confirmation and explicit reload permits correction/deletion', async () => {
  await act(async () => {root=create(<PersonalVocabulary />);});
  expect(root.root.findByType('textarea').props.disabled).toBe(false);
  await act(async () => root.root.findByType('textarea').props.onChange({target:{value:'OpenAPI'}}));
  stored='Ёлка\nAPI\nSRE';
  await act(async () => button('Save vocabulary').props.onClick());
  expect(stored).toBe('Ёлка\nAPI\nSRE');
  expect(JSON.stringify(root.toJSON())).toContain('Vocabulary changed');
  expect(root.root.findByType('textarea').props.value).toBe('OpenAPI');
  await act(async () => button('Reload saved vocabulary').props.onClick());
  expect(root.root.findByType('textarea').props.value).toBe('Ёлка\nAPI\nSRE');
  await act(async () => root.root.findByType('textarea').props.onChange({target:{value:'OpenAPI\nSRE'}}));
  await act(async () => button('Save vocabulary').props.onClick());
  expect(stored).toBe('OpenAPI\nSRE');
});
test('failed initial read blocks destructive save, with visible retry and engine limits', async () => {
  failRead=true;
  await act(async () => {root=create(<PersonalVocabulary />);});
  expect(button('Save vocabulary').props.disabled).toBe(true);
  expect(JSON.stringify(root.toJSON())).toContain('read unavailable');
  expect(JSON.stringify(root.toJSON())).toContain('224');
  expect(JSON.stringify(root.toJSON())).toContain('Parakeet');
  failRead=false;
  await act(async () => button('Reload saved vocabulary').props.onClick());
  expect(button('Save vocabulary').props.disabled).toBe(false);
});
