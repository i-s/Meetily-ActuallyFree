import { expect, mock, test } from 'bun:test';
let chosen: string | null = '/synthetic/user-chosen.custom';
let options: any;
let written: any[] = [];
let failure = false;
mock.module('@tauri-apps/plugin-dialog', () => ({ save: async (value: any) => { options = value; return chosen; } }));
mock.module('@tauri-apps/plugin-fs', () => ({ writeFile: async (...args: any[]) => { if (failure) throw new Error('disk full'); written = args; } }));
const { exportSummaryAs } = await import('../../src/lib/exportSummary');
test('writes only the exact picker-granted destination and retains safe Cyrillic filename', async () => {
  expect(await exportSummaryAs('markdown', 'Текст', '../Обсуждение / API\u0000')).toBe(true);
  expect(written[0]).toBe('/synthetic/user-chosen.custom');
  expect(options.defaultPath).toContain('Обсуждение');
  expect(options.defaultPath).not.toContain('/');
  expect(options.defaultPath).not.toContain('\u0000');
  expect(new TextDecoder().decode(written[1])).toBe('Текст');
});
test('cancel and failed writes never report successful export', async () => {
  written = [];
  chosen = null;
  expect(await exportSummaryAs('markdown', 'text', 'name')).toBe(false);
  expect(written).toEqual([]);
  chosen = '/synthetic/name.md';
  failure = true;
  await expect(exportSummaryAs('markdown', 'text', 'name')).rejects.toThrow('disk full');
});
