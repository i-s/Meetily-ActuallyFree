import { expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
function draftFor(target: string) {
  const prefix = `af-notes-v2:draft:${encodeURIComponent(target)}:`;
  const raw = [...storage].find(([key]) => key.startsWith(prefix))?.[1];
  return raw ? JSON.parse(raw) : null;
}
Object.assign(globalThis, { localStorage: {
  get length() { return storage.size; },
  key: (index: number) => [...storage.keys()][index] ?? null,
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
} });
const documents = new Map<string, any>();
let fail = false;
let hold: Promise<void> | null = null;
let writes = 0;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'get_meeting_folder_path') return '/synthetic/current';
  if (command === 'api_notes_recording_target') return `recording:/synthetic/${args.meetingId}`;
  if (command === 'api_read_notes_document') return documents.get(args.target) ?? null;
  if (command === 'api_write_notes_document') {
    writes++;
    if (hold) await hold;
    if (fail) throw new Error('disk unavailable');
    const previous = documents.get(args.target);
    if ((previous?.revision ?? 0) !== args.expectedRevision) throw new Error('Notes conflict');
    const revision = args.expectedRevision + 1;
    documents.set(args.target, { markdown: args.markdown, json: args.json, revision });
    return revision;
  }
  throw new Error(command);
} }));
const store = await import('../../src/lib/notes-store');
const content = (markdown: string) => ({ markdown, json: [{ type: 'paragraph', content: markdown }] });
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };

test('recording Notes use the native folder identity', async () => {
  expect(await store.currentRecordingNotesTarget()).toBe('recording:/synthetic/current');
});

test('rapid conversions and an in-flight save cannot overwrite the newest edit', async () => {
  const target = 'recording:/synthetic/rapid';
  await store.loadNotes(target);
  let finishOld!: (value: string) => void;
  store.editNotes(target, content('old'), new Promise(resolve => { finishOld = resolve; }));
  expect(draftFor(target).content.markdown).toBe('old');
  store.editNotes(target, content('new'), Promise.resolve('new'));
  await settle();
  finishOld('old');
  await settle();
  let release!: () => void;
  hold = new Promise(resolve => { release = resolve; });
  const saving = store.flushNotes(target);
  store.editNotes(target, content('newest'), Promise.resolve('newest'));
  await settle();
  hold = null; release(); await saving;
  expect(documents.get(target).markdown).toBe('newest');
  expect(store.getNotesSnapshot(target).state).toBe('saved');
  expect(draftFor(target)).toBeNull();
});

test('failed writes retain draft and retry without needing another edit', async () => {
  const target = 'meeting:retry';
  await store.loadNotes(target);
  fail = true;
  store.editNotes(target, content('keep this'), Promise.resolve('keep this')); await settle();
  await store.flushNotes(target);
  expect(store.getNotesSnapshot(target).state).toBe('error');
  expect(draftFor(target)).not.toBeNull();
  await expect(store.notesForContext(target)).rejects.toThrow('disk unavailable');
  fail = false; await store.flushNotes(target);
  expect((await store.notesForContext(target)).markdown).toBe('keep this');
});

test('a fresh store entry recovers last keystrokes through meeting-folder alias after restart', async () => {
  storage.set('af-notes-v1:recording:/synthetic/recovered', JSON.stringify({ content: content('recovered'), revision: 0, converting: false }));
  await store.loadNotes('meeting:recovered');
  await store.flushNotes('meeting:recovered');
  expect(documents.get('meeting:recovered').markdown).toBe('recovered');
  // Mutable v1 data is retained; an immutable exact-value receipt suppresses replay.
  expect(storage.has('af-notes-v1:recording:/synthetic/recovered')).toBe(true);
});

test('interrupted conversion recovers JSON and blocks stale context until reconverted', async () => {
  const target = 'meeting:conversion';
  storage.set(`af-notes-v1:${target}`, JSON.stringify({ content: content('exact JSON'), revision: 0, converting: true }));
  await store.loadNotes(target);
  expect(store.getNotesSnapshot(target).content.json).toEqual(content('exact JSON').json);
  await expect(store.notesForContext(target)).rejects.toThrow('still being saved');
  store.editNotes(target, content('exact JSON'), Promise.resolve('converted')); await settle();
  await store.flushNotes(target);
  expect((await store.notesForContext(target)).markdown).toBe('converted');
});

test('new recordings and existing meetings remain isolated; clearing Notes persists', async () => {
  documents.set('meeting:old', { ...content('legacy'), revision: 0 });
  await store.loadNotes('meeting:old');
  await store.loadNotes('recording:/synthetic/new');
  expect(store.getNotesSnapshot('meeting:old').content.markdown).toBe('legacy');
  expect(store.getNotesSnapshot('recording:/synthetic/new').content.markdown).toBe('');
  store.editNotes('meeting:old', content(''), Promise.resolve('')); await settle();
  await store.flushNotes('meeting:old');
  expect(documents.get('meeting:old').markdown).toBe('');
});

test('read and conversion errors are visible and never produce an empty save', async () => {
  const target = 'meeting:bad-journal';
  storage.set(`af-notes-v1:${target}`, '{invalid');
  const before = writes;
  await store.loadNotes(target);
  expect(store.getNotesSnapshot(target).state).toBe('error');
  expect(store.getNotesSnapshot(target).loaded).toBe(false);
  expect(writes).toBe(before);
  storage.delete(`af-notes-v1:${target}`);
  await store.loadNotes(target);
  store.editNotes(target, content('preserved'), Promise.reject(new Error('conversion failed'))); await settle();
  expect(store.getNotesSnapshot(target).error).toBe('conversion failed');
  await store.flushNotes(target);
  expect(writes).toBe(before);
});

test('a conflict retains both versions until the reviewed revision is explicitly chosen', async () => {
  const target = 'meeting:conflict';
  await store.loadNotes(target);
  documents.set(target, { ...content('other editor'), revision: 1 });
  store.editNotes(target, content('my draft'), Promise.resolve('my draft')); await settle();
  await store.flushNotes(target);
  expect(store.getNotesSnapshot(target).error).toContain('Notes conflict');
  expect(documents.get(target).markdown).toBe('other editor');
  await store.saveReviewedNotes(target, 1);
  expect(documents.get(target).markdown).toBe('my draft');
  expect((await store.notesForContext(target)).source).toBe('user_notes');
});

test('a clean library reentry refreshes native edits and a stale context is rejected', async () => {
  const target = 'meeting:refresh';
  documents.set(target, { ...content('first'), revision: 1 });
  await store.loadNotes(target);
  documents.set(target, { ...content('changed elsewhere'), revision: 2 });
  await expect(store.notesForContext(target)).rejects.toThrow('changed in another editor');
  await store.loadNotes(target, true);
  expect(store.getNotesSnapshot(target).content.markdown).toBe('changed elsewhere');
});

test('interrupted alias-journal migration can resume without discarding a newer edit', async () => {
  const alias = 'recording:/synthetic/alias-copy';
  const older = JSON.stringify({ content: content('older'), revision: 0, converting: false });
  storage.set(`af-notes-v1:${alias}`, older);
  storage.set('af-notes-v1:meeting:alias-copy', JSON.stringify({ content: content('latest'), revision: 0, converting: false, inheritedAlias: { target: alias, raw: older } }));
  await store.loadNotes('meeting:alias-copy');
  await store.flushNotes('meeting:alias-copy');
  expect(documents.get('meeting:alias-copy').markdown).toBe('latest');
  expect(storage.has(`af-notes-v1:${alias}`)).toBe(true);
});

test('projection refetch cannot replace dirty Notes during conversion, debounce or a native save', async () => {
  const target = 'meeting:projection-refetch';
  const original = content('Previously saved Notes');
  const edited = content('Моя мысль, not speech — exact\n- follow up API');
  documents.set(target, { ...original, revision: 1 });
  await store.loadNotes(target);
  let converted!: (markdown: string) => void;
  store.editNotes(target, edited, new Promise(resolve => { converted = resolve; }));
  // A correction/version switch refetches projections immediately, before
  // conversion or the 400ms Notes debounce can complete.
  await store.loadNotes(target, true);
  expect(store.getNotesSnapshot(target).content).toEqual(edited);
  expect(documents.get(target).markdown).toBe(original.markdown);
  expect(draftFor(target).content).toEqual(edited);
  converted(edited.markdown); await settle();
  await store.loadNotes(target, true);
  expect(store.getNotesSnapshot(target).content).toEqual(edited);
  expect(store.getNotesSnapshot(target).state).toBe('pending');
  let release!: () => void;
  hold = new Promise(resolve => { release = resolve; });
  const saving = store.flushNotes(target);
  await store.loadNotes(target, true);
  expect(store.getNotesSnapshot(target).content).toEqual(edited);
  expect(store.getNotesSnapshot(target).state).toBe('saving');
  hold = null; release(); await saving;
  expect(documents.get(target).markdown).toBe(edited.markdown);
  expect(documents.get(target).json).toEqual(edited.json);
  expect(store.getNotesSnapshot(target).state).toBe('saved');
  await store.loadNotes(target, true);
  expect(store.getNotesSnapshot(target).content).toEqual(edited);
});


test('committed revision notifications advance only on native read/ack, not pending or failed writes', async () => {
  const target = 'meeting:committed-revision';
  documents.set(target, { ...content('Original'), revision: 4 });
  const revisions: Array<number | undefined> = [];
  const stop = store.subscribeNotes(target, () => revisions.push(store.getNotesSnapshot(target).committedRevision));
  try {
    await store.loadNotes(target);
    expect(store.getNotesSnapshot(target).committedRevision).toBe(4);
    const before = revisions.length;
    store.editNotes(target, content('Updated'), Promise.resolve('Updated'));
    await settle();
    let release!: () => void;
    hold = new Promise(resolve => { release = resolve; });
    const saving = store.flushNotes(target);
    expect(store.getNotesSnapshot(target).committedRevision).toBe(4);
    fail = true; hold = null; release(); await saving;
    expect(store.getNotesSnapshot(target).state).toBe('error');
    expect(revisions.slice(before).every(revision => revision === 4)).toBe(true);
    fail = false; await store.flushNotes(target);
    expect(store.getNotesSnapshot(target).committedRevision).toBe(5);
    expect(revisions.at(-1)).toBe(5);
  } finally { fail = false; hold = null; stop(); }
});

test('R2 superseded native acknowledgement publishes its revision while newer rich draft fails and retries', async () => {
  const target = 'meeting:superseded-ack';
  const native = new Map<string, any>([[target, { ...content('Original'), revision: 4 }]]);
  const local = new Map<string, string>();
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  let rejectNewer = true;
  const isolated = store.createNotesStore({ storage: {
    get length() { return local.size; }, key: (i: number) => [...local.keys()][i] ?? null,
    getItem: (k: string) => local.get(k) ?? null, setItem: (k: string, v: string) => { local.set(k, v); }, removeItem: (k: string) => { local.delete(k); },
  } as Storage, invoke: (async (command: string, args: any) => {
    if (command === 'api_notes_recording_target') return null;
    if (command === 'api_read_notes_document') return native.get(args.target) ?? null;
    if (command === 'api_write_notes_document') {
      if (args.markdown === 'A') await gate;
      else if (rejectNewer) throw new Error('B failed');
      expect(args.expectedRevision).toBe(native.get(args.target).revision);
      const revision = args.expectedRevision + 1;
      native.set(args.target, { markdown: args.markdown, json: args.json, revision }); return revision;
    }
    throw new Error(command);
  }) as any });
  await isolated.loadNotes(target);
  await isolated.loadNotes('meeting:unrelated');
  isolated.editNotes(target, content('A'), Promise.resolve('A')); await settle();
  const saving = isolated.flushNotes(target);
  const b = content('B rich');
  let convert!: (text: string) => void;
  isolated.editNotes(target, b, new Promise(resolve => { convert = resolve; }));
  release(); await saving;
  expect(native.get(target).revision).toBe(5);
  expect(isolated.getNotesSnapshot(target).committedRevision).toBe(5);
  expect(isolated.getNotesSnapshot(target).content).toEqual(b);
  expect(isolated.getNotesSnapshot(target).converting).toBe(true);
  expect(isolated.getNotesSnapshot(target).state).not.toBe('saved');
  expect(isolated.getNotesSnapshot('meeting:unrelated').committedRevision).toBe(0);
  expect([...local.values()].map(v => JSON.parse(v)).some(d => d.content.markdown === 'B rich' && d.converting)).toBe(true);
  convert('B rich'); await settle(); await isolated.flushNotes(target);
  expect(isolated.getNotesSnapshot(target).error).toBe('B failed');
  expect(isolated.getNotesSnapshot(target).committedRevision).toBe(5);
  expect(isolated.getNotesSnapshot(target).content).toEqual(b);
  expect([...local.values()].map(v => JSON.parse(v)).some(d => d.content.markdown === 'B rich' && d.revision === 5)).toBe(true);
  rejectNewer = false; await isolated.flushNotes(target);
  expect(native.get(target).revision).toBe(6);
  expect(isolated.getNotesSnapshot(target).committedRevision).toBe(6);
  expect(isolated.getNotesSnapshot(target).state).toBe('saved');
  expect(isolated.getNotesSnapshot(target).content).toEqual(b);
  expect(local.size).toBe(0);
});

for (const target of ['meeting:conversion-ack', 'recording:/synthetic/conversion-ack']) {
  test(`R2 acknowledgement preserves newer conversion error and target isolation (${target})`, async () => {
    documents.set(target, { ...content('Original'), revision: 3 });
    await store.loadNotes(target);
    const other = 'meeting:conversion-control';
    await store.loadNotes(other);
    const before = store.getNotesSnapshot(other);
    store.editNotes(target, content('A'), Promise.resolve('A')); await settle();
    let release!: () => void;
    hold = new Promise(resolve => { release = resolve; });
    const saving = store.flushNotes(target);
    const b = content('B exact rich JSON');
    store.editNotes(target, b, Promise.reject(new Error('B conversion failed'))); await settle();
    expect(store.getNotesSnapshot(target).error).toBe('B conversion failed');
    hold = null; release(); await saving;
    expect(store.getNotesSnapshot(target).committedRevision).toBe(4);
    expect(store.getNotesSnapshot(target).state).toBe('error');
    expect(store.getNotesSnapshot(target).error).toBe('B conversion failed');
    expect(store.getNotesSnapshot(target).content).toEqual(b);
    expect(store.getNotesSnapshot(target).converting).toBe(true);
    expect(draftFor(target).content).toEqual(b);
    expect(draftFor(target).revision).toBe(4);
    expect(store.getNotesSnapshot(other)).toBe(before);
    store.editNotes(target, b, Promise.resolve(b.markdown)); await settle(); await store.flushNotes(target);
    expect(store.getNotesSnapshot(target).state).toBe('saved');
    expect(store.getNotesSnapshot(target).committedRevision).toBe(5);
  });
}
