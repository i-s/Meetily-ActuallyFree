import { expect, test } from 'bun:test';
import { createNotesStore, type NotesContent } from '../../src/lib/notes-store';
import type { invoke } from '@tauri-apps/api/core';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
const content = (markdown: string): NotesContent => ({ markdown, json: [{ type: 'paragraph', content: markdown }] });
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };
function harness() {
  const rows = new Map<string, string>();
  let onRemove: ((key: string) => void) | undefined;
  let onSet: ((key: string) => void) | undefined;
  const storage: Storage = {
    get length() { return rows.size; },
    key: index => [...rows.keys()][index] ?? null,
    getItem: key => rows.get(key) ?? null,
    setItem: (key, value) => { rows.set(key, value); onSet?.(key); },
    removeItem: key => { onRemove?.(key); rows.delete(key); },
    clear: () => rows.clear(),
  };
  let document: any = null;
  let read: ReturnType<typeof deferred<any>> | null = null;
  let readStarted: ReturnType<typeof deferred<void>> | null = null;
  let write: ReturnType<typeof deferred<void>> | null = null;
  const transport = (async (command: string, args: any) => {
    if (command === 'api_notes_recording_target') return 'recording:/synthetic/shared';
    if (command === 'api_read_notes_document') {
      if (read) { readStarted!.resolve(); return read.promise; }
      return structuredClone(document);
    }
    if (command === 'api_write_notes_document') {
      if (write) await write.promise;
      if ((document?.revision ?? 0) !== args.expectedRevision) throw new Error('Notes conflict');
      document = { ...structuredClone(args), revision: args.expectedRevision + 1 };
      return document.revision;
    }
    throw new Error(command);
  }) as typeof invoke;
  return { rows, storage, store: () => createNotesStore({ invoke: transport, storage }),
    get document() { return document; },
    onRemove(fn: (key: string) => void) { onRemove = fn; },
    onSet(fn: (key: string) => void) { onSet = fn; },
    holdRead() { read = deferred<any>(); readStarted = deferred<void>(); return readStarted.promise; },
    releaseRead() { const pending = read!; read = null; pending.resolve(structuredClone(document)); },
    holdWrite() { write = deferred<void>(); },
    releaseWrite() { const pending = write!; write = null; pending.resolve(); },
  };
}

for (const pendingConversion of [false, true]) {
  test(`context read refuses an edit during delayed native read (conversion pending=${pendingConversion})`, async () => {
    const h = harness(); const store = h.store(); const target = 'meeting:shared';
    await store.loadNotes(target);
    store.editNotes(target, content('committed A'), Promise.resolve('committed A')); await settle();
    await store.flushNotes(target);
    const started = h.holdRead();
    const reading = store.notesForContext(target).then(value => ({ value }), error => ({ error }));
    await started;
    const converting = deferred<string>();
    store.editNotes(target, { ...content('UNSAVED B'), markdown: 'committed A' }, converting.promise);
    if (!pendingConversion) { converting.resolve('UNSAVED B'); await settle(); }
    h.releaseRead();
    const result = await reading;
    expect('error' in result).toBe(true);
    if ('value' in result) expect(result.value.json).toEqual(h.document.json);
    converting.resolve('UNSAVED B'); await settle(); await store.flushNotes(target);
  });
}

for (const alias of [false, true]) {
  test(`A acknowledgement cannot erase B's shared-storage draft (recording alias=${alias})`, async () => {
    const h = harness(); const a = h.store(); const b = h.store();
    const aTarget = alias ? 'recording:/synthetic/shared' : 'meeting:shared';
    await a.loadNotes(aTarget); await b.loadNotes(aTarget);
    a.editNotes(aTarget, content('A to commit'), Promise.resolve('A to commit')); await settle();
    h.holdWrite(); const savingA = a.flushNotes(aTarget);
    const bConversion = deferred<string>();
    b.editNotes(aTarget, { ...content('B newer exact JSON'), markdown: '' }, bConversion.promise);
    const before = [...h.rows.values()].join('\n');
    expect(before).toContain('B newer exact JSON');
    h.releaseWrite(); await savingA;
    expect([...h.rows.values()].join('\n')).toContain('B newer exact JSON');
    // A fresh instance represents another WebView/app restart: no access to B's
    // JS memory or listeners, only shared durable browser/native state.
    const restarted = h.store(); const recoveredTarget = 'meeting:shared';
    await restarted.loadNotes(recoveredTarget);
    expect(restarted.getNotesSnapshot(recoveredTarget).content.json).toEqual(content('B newer exact JSON').json);
    bConversion.resolve('B newer exact JSON'); await settle();
    await b.flushNotes(aTarget); // B's stale revision is a visible conflict, not data loss.
    expect(b.getNotesSnapshot(aTarget).state).toBe('error');
  });
}


test('a concurrent writer inserted inside cleanup cannot be deleted by read/remove interleaving', async () => {
  const h = harness(); const a = h.store(); const b = h.store(); const target = 'meeting:shared';
  await a.loadNotes(target); await b.loadNotes(target);
  a.editNotes(target, content('A'), Promise.resolve('A')); await settle();
  h.holdWrite(); const savingA = a.flushNotes(target);
  let interleaved = false;
  const converting = deferred<string>();
  h.onRemove(() => {
    if (interleaved) return;
    interleaved = true;
    b.editNotes(target, { ...content('B during remove'), markdown: '' }, converting.promise);
  });
  h.releaseWrite(); await savingA;
  expect(interleaved).toBe(true);
  const restarted = h.store(); await restarted.loadNotes(target);
  expect(restarted.getNotesSnapshot(target).content.json).toEqual(content('B during remove').json);
  converting.resolve('B during remove'); await settle(); await b.flushNotes(target);
});

for (const alias of [false, true]) {
  test(`legacy mutable journal replaced while retiring is never removed or suppressed (alias=${alias})`, async () => {
    const h = harness(); const target = 'meeting:shared';
    const source = alias ? 'recording:/synthetic/shared' : target;
    const key = `af-notes-v1:${source}`;
    const first = JSON.stringify({ content: content('legacy A'), revision: 0, converting: false });
    const second = JSON.stringify({ content: { ...content('legacy B concurrent'), markdown: '' }, revision: 0, converting: true });
    h.rows.set(key, first);
    h.holdWrite();
    const a = h.store(); await a.loadNotes(target);
    let interleaved = false;
    h.onSet(writtenKey => {
      if (!interleaved && writtenKey.startsWith('af-notes-v2:receipt:')) {
        interleaved = true;
        h.rows.set(key, second);
      }
    });
    h.releaseWrite(); await a.flushNotes(target);
    expect(interleaved).toBe(true);
    expect(h.rows.get(key)).toBe(second);
    const restarted = h.store(); await restarted.loadNotes(target);
    expect(restarted.getNotesSnapshot(target).content.json).toEqual(content('legacy B concurrent').json);
  });
}

test('independent drafts require explicit selection and choosing one retains the other across restart', async () => {
  const h = harness(); const target = 'meeting:shared'; const a = h.store(); const b = h.store();
  await a.loadNotes(target); await b.loadNotes(target);
  const convertingA = deferred<string>(); const convertingB = deferred<string>();
  a.editNotes(target, content('independent A'), convertingA.promise);
  b.editNotes(target, content('independent B'), convertingB.promise);
  const restarted = h.store(); await restarted.loadNotes(target);
  const conflict = restarted.getNotesSnapshot(target);
  expect(conflict.loaded).toBe(false);
  expect(conflict.recoveryDrafts?.map(draft => draft.content.markdown).sort()).toEqual(['independent A', 'independent B']);
  const chosen = conflict.recoveryDrafts!.find(draft => draft.content.markdown === 'independent A')!;
  await restarted.recoverNotesDraft(target, chosen.key);
  expect(restarted.getNotesSnapshot(target).content.markdown).toBe('independent A');
  restarted.editNotes(target, content('independent A'), Promise.resolve('independent A')); await settle();
  await restarted.flushNotes(target);
  const again = h.store(); await again.loadNotes(target);
  expect(again.getNotesSnapshot(target).content.json).toEqual(content('independent B').json);
  convertingA.resolve('independent A'); convertingB.resolve('independent B'); await settle();
  await a.flushNotes(target); await b.flushNotes(target);
});

test('successful context snapshots are committed, coherent and detached from editor mutations', async () => {
  const h = harness(); const target = 'meeting:shared'; const store = h.store();
  await store.loadNotes(target);
  const original = content('committed');
  store.editNotes(target, original, Promise.resolve(original.markdown)); await settle(); await store.flushNotes(target);
  const snapshot = await store.notesForContext(target);
  expect(snapshot.markdown).toBe('committed'); expect(snapshot.json).toEqual(original.json); expect(snapshot.revision).toBe(1);
  original.json!.push({ type: 'paragraph', content: 'mutable caller' });
  store.editNotes(target, content('later edit'), Promise.resolve('later edit')); await settle();
  expect(snapshot.json).toEqual(content('committed').json);
  expect(snapshot.markdown).toBe('committed');
  await store.flushNotes(target);
});
