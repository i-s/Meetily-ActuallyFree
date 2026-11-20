import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, beforeEach, expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
const local = { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value) };
Object.assign(globalThis, { window: Object.assign(new EventTarget(), { localStorage: local }), localStorage: local });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { language: 'en-US' } });
const calls: Array<[string, any]> = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push([command, args]);
  switch (command) {
    case 'get_ollama_models': return [];
    case 'api_get_transcript_config': return {provider:'parakeet', model:'parakeet-tdt-0.6b-v3-int8', apiKey:null};
    case 'api_get_model_config': case 'api_get_api_key': case 'get_recording_preferences': return null;
    case 'set_language_preference': return;
    default: throw new Error(`Unexpected command: ${command}`);
  }
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('@/lib/analytics', () => ({ default: { track: async () => {} } }));
mock.module('sonner', () => ({ toast: {success: () => {}, error: () => {}} }));
// Only replace browser-dependent primitives; config, persistence and selection are real.
mock.module('@/components/ui/select', () => ({
  Select: ({children, value, onValueChange}: any) => <select value={value} onChange={event => onValueChange(event.target.value)}>{children}</select>,
  SelectTrigger: ({children}: any) => <div>{children}</div>, SelectValue: () => null,
  SelectContent: ({children}: any) => <>{children}</>, SelectItem: ({children, value}: any) => <option value={value}>{children}</option>,
}));
const { ConfigProvider, useConfig } = await import('../../src/contexts/ConfigContext');
const { LanguageSelection } = await import('../../src/components/LanguageSelection');
let root: ReactTestRenderer;
let config: ReturnType<typeof useConfig>;
function Settings() {
  config = useConfig();
  return <LanguageSelection provider="localWhisper" selectedLanguage={config.selectedLanguage} onLanguageChange={config.setSelectedLanguage} />;
}
const mount = async () => { await act(async () => { root = create(<ConfigProvider><Settings /></ConfigProvider>); }); };
beforeEach(() => { storage.clear(); calls.length = 0; });
afterEach(() => { act(() => root?.unmount()); });

test('English system locale must not force Whisper to interpret Russian speech as English', async () => {
  await mount();
  expect(config.selectedLanguage).toBe('auto');
  expect(calls).toContainEqual(['set_language_preference', {language:'auto'}]);
});

test('Russian can be selected while live Parakeet is active and survives restart', async () => {
  await mount();
  expect(config.transcriptModelConfig.provider).toBe('parakeet');
  calls.length = 0;
  await act(async () => { await root.root.findByType('select').props.onChange({target:{value:'ru'}}); });
  expect(storage.get('primaryLanguage')).toBe('ru');
  expect(config.selectedLanguage).toBe('ru');
  expect(calls.filter(([name]) => name === 'set_language_preference')).toEqual([['set_language_preference', {language:'ru'}]]);
  act(() => root.unmount());
  await mount();
  expect(config.selectedLanguage).toBe('ru');
});

test('an explicit saved language or translation choice remains explicit', async () => {
  storage.set('primaryLanguage','en'); await mount();
  expect(config.selectedLanguage).toBe('en');
  act(() => root.unmount());
  storage.set('primaryLanguage','auto-translate'); await mount();
  expect(config.selectedLanguage).toBe('auto-translate');
});
