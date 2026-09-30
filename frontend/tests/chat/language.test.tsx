import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
const { ChatThread } = await import('../../src/components/chat/ChatThread');

test('Russian chat sends the displayed preset, localizes controls, and keeps timestamp seeking', async () => {
  const questions: string[] = [];
  const sought: number[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => {
    view = create(<ChatThread historyKey="ru-preset-test" language="ru"
      suggestions={['Что было решено?']} onSeek={(seconds) => sought.push(seconds)}
      ask={async (question) => { questions.push(question); return 'Решили выпустить в пятницу [12:34].'; }} />);
  });
  expect(view!.root.findByType('textarea').props.placeholder).toBe('Задайте вопрос…');
  expect(view!.root.findByProps({ 'aria-label': 'Отправить' })).toBeDefined();
  await act(async () => { view!.root.findAllByType('button').find(node => node.children.includes('Что было решено?'))!.props.onClick(); });
  expect(questions).toEqual(['Что было решено?']);
  expect(view!.root.findByProps({ 'aria-label': 'Очистить переписку' })).toBeDefined();
  act(() => { view!.root.findAllByType('button').find(node => node.children.includes('12:34'))!.props.onClick(); });
  expect(sought).toEqual([754]);
  act(() => view!.unmount());
});

test('Russian error/retry copy is localized and a typed English question is preserved', async () => {
  const questions: string[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => {
    view = create(<ChatThread historyKey="ru-typed-test" language="ru" ask={async (question) => {
      questions.push(question); throw new Error('provider detail');
    }} />);
  });
  act(() => view!.root.findByType('textarea').props.onChange({ target: { value: 'Who owns API migration?', style: {}, scrollHeight: 32 } }));
  await act(async () => { view!.root.findByType('form').props.onSubmit({ preventDefault() {} }); });
  expect(questions).toEqual(['Who owns API migration?']);
  expect(JSON.stringify(view!.toJSON())).toContain('Не удалось ответить:');
  expect(JSON.stringify(view!.toJSON())).toContain('Повторить');
  act(() => view!.unmount());
});

test('language resolution blocks preset and typed sends until the selected language is ready', async () => {
  const questions: string[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => {
    view = create(<ChatThread historyKey="pending-language" language="ru" disabled suggestions={['Что было решено?']}
      ask={async question => { questions.push(question); return 'Ответ'; }} />);
  });
  const preset = view!.root.findAllByType('button').find(node => node.children.includes('Что было решено?'))!;
  expect(preset.props.disabled).toBe(true);
  await act(async () => preset.props.onClick());
  expect(questions).toEqual([]);
  act(() => view!.unmount());
});
