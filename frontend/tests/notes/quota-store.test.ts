import { expect, test } from 'bun:test';
import { createNotesStore, type NotesContent } from '../../src/lib/notes-store';
import type { invoke } from '@tauri-apps/api/core';

const exact: NotesContent = {
  markdown: '**Моя мысль**, not speech\n- follow up API',
  json: [{ id: 'rich', type: 'paragraph', props: {}, content: [{ type: 'text', text: 'Моя мысль', styles: { bold: true } }], children: [] }],
};
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
function harness(converting = false, alias = false) {
  const target = 'meeting:quota';
  const source = alias ? 'recording:/synthetic/quota' : target;
  const key = `af-notes-v2:draft:${encodeURIComponent(source)}:retained`;
  const raw = JSON.stringify({ target: source, content: { ...exact, markdown: converting ? 'STALE TEXT' : exact.markdown }, revision: 0, converting });
  const rows = new Map([[key, raw]]);
  let quota = true;
  let failure = false;
  let verifyFailure = false;
  let reorderJson = false;
  let writes = 0;
  let native: any = null;
  let writeHold: ReturnType<typeof deferred<void>> | null = null;
  let verificationHold: ReturnType<typeof deferred<void>> | null = null;
  const storage: Storage = {
    get length() { return rows.size; }, key: index => [...rows.keys()][index] ?? null,
    getItem: key => rows.get(key) ?? null,
    setItem: (key, value) => { if (quota) throw new DOMException('Storage full', 'QuotaExceededError'); rows.set(key, value); },
    removeItem: key => { rows.delete(key); }, clear: () => rows.clear(),
  };
  const transport = (async (command: string, args: any) => {
    if (command === 'api_notes_recording_target') return alias ? source : null;
    if (command === 'api_read_notes_document') {
      if (verificationHold && native) await verificationHold.promise;
      if (verifyFailure && native) throw new Error('native verification unavailable');
      const copy = structuredClone(native);
      if (copy && reorderJson) copy.json = JSON.parse(JSON.stringify(copy.json, (_key, value) => value && typeof value === 'object' && !Array.isArray(value) ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))) : value));
      return copy;
    }
    if (command === 'api_write_notes_document') {
      writes++;
      if (writeHold) await writeHold.promise;
      if (failure) throw new Error('SQLite unavailable');
      if ((native?.revision ?? 0) !== args.expectedRevision) {
        // Match the production identical lost-response retry semantics.
        if (native?.revision === args.expectedRevision + 1 && native.markdown === args.markdown && JSON.stringify(native.json) === JSON.stringify(args.json)) return native.revision;
        throw new Error('Notes conflict');
      }
      native = { ...structuredClone(args), revision: args.expectedRevision + 1 };
      return native.revision;
    }
    throw new Error(command);
  }) as typeof invoke;
  return {
    target, source, key, raw, rows, store: () => createNotesStore({ invoke: transport, storage }),
    get writes() { return writes; }, get native() { return native; },
    setFailure(value: boolean) { failure = value; },
    setQuota(value: boolean) { quota = value; },
    setVerifyFailure(value: boolean) { verifyFailure = value; },
    reorderNativeJson() { reorderJson = true; },
    conflict() { native = { ...exact, markdown: 'Another writer', revision: 1 }; },
    holdWrite() { writeHold = deferred<void>(); },
    releaseWrite() { const pending = writeHold!; writeHold = null; pending.resolve(); },
    holdVerification() { verificationHold = deferred<void>(); },
    releaseVerification() { const pending = verificationHold!; verificationHold = null; pending.resolve(); },
  };
}

for (const alias of [false, true]) {
  test(`quota recovery needs no extra storage and retains the original until verified native commit (alias=${alias})`, async () => {
    const h = harness(false, alias); const store = h.store();
    await store.loadNotes(h.target);
    expect(store.getNotesSnapshot(h.target).loaded).toBe(true);
    expect(store.getNotesSnapshot(h.target).backupBlocked).toBe(true);
    expect(store.getNotesSnapshot(h.target).content).toEqual(exact);
    expect(h.writes).toBe(0); expect(h.rows.get(h.key)).toBe(h.raw);
    h.holdWrite(); h.holdVerification();
    const saving = store.saveRetainedNotes(h.target);
    expect(h.rows.get(h.key)).toBe(h.raw);
    expect(() => store.cancelNotesRecovery(h.target)).toThrow('cannot be cancelled');
    h.releaseWrite(); await settle();
    expect(h.rows.get(h.key)).toBe(h.raw);
    h.releaseVerification(); await saving;
    expect(h.native.markdown).toBe(exact.markdown); expect(h.native.json).toEqual(exact.json);
    expect(h.rows.has(h.key)).toBe(false);
    expect(store.getNotesSnapshot(h.target).state).toBe('saved');
    expect(store.getNotesSnapshot(h.target).backupBlocked).toBe(true);
    const reopened = h.store(); await reopened.loadNotes(h.target);
    expect(reopened.getNotesSnapshot(h.target).content).toEqual(exact);
    expect(h.writes).toBe(1);
  });
}

test('failed native recovery remains readable and retryable after restart without added storage', async () => {
  const h = harness(); h.setFailure(true); const store = h.store(); await store.loadNotes(h.target);
  await expect(store.saveRetainedNotes(h.target)).rejects.toThrow('SQLite unavailable');
  expect(h.rows.get(h.key)).toBe(h.raw);
  const restarted = h.store(); await restarted.loadNotes(h.target);
  expect(restarted.getNotesSnapshot(h.target).loaded).toBe(true);
  expect(restarted.getNotesSnapshot(h.target).content.json).toEqual(exact.json);
  h.setFailure(false); await restarted.saveRetainedNotes(h.target);
  expect(h.native.markdown).toBe(exact.markdown); expect(h.rows.has(h.key)).toBe(false);
});

test('failed readback preserves the source even after native write; retry verifies before retirement', async () => {
  const h = harness(); const store = h.store(); await store.loadNotes(h.target);
  h.setVerifyFailure(true);
  await expect(store.saveRetainedNotes(h.target)).rejects.toThrow('verification unavailable');
  expect(h.rows.get(h.key)).toBe(h.raw);
  expect(h.native.markdown).toBe(exact.markdown);
  expect(store.getNotesSnapshot(h.target).committedRevision).toBe(0);
  expect(store.getNotesSnapshot(h.target).state).toBe('error');
  h.setVerifyFailure(false); await store.saveRetainedNotes(h.target);
  expect(h.rows.has(h.key)).toBe(false);
  expect(store.getNotesSnapshot(h.target).committedRevision).toBe(h.native.revision);
});

test('cancel invalidates pending conversion, keeps the source, and requires explicit resume/reconversion', async () => {
  const h = harness(true); const store = h.store(); await store.loadNotes(h.target);
  const converting = deferred<string>();
  store.editNotes(h.target, { ...exact, markdown: 'STALE TEXT' }, converting.promise);
  store.cancelNotesRecovery(h.target);
  converting.resolve('completion after cancel'); await settle();
  await store.flushNotes(h.target);
  await expect(store.saveRetainedNotes(h.target)).rejects.toThrow('Resume recovery');
  expect(h.writes).toBe(0); expect(h.rows.get(h.key)).toBe(h.raw);
  const restarted = h.store(); await restarted.loadNotes(h.target);
  expect(restarted.getNotesSnapshot(h.target).content.json).toEqual(exact.json);
  store.resumeNotesRecovery(h.target);
  store.editNotes(h.target, { ...exact, markdown: 'STALE TEXT' }, Promise.resolve(exact.markdown)); await settle();
  await store.saveRetainedNotes(h.target);
  expect(h.native.markdown).toBe(exact.markdown); expect(h.native.json).toEqual(exact.json);
});

test('quota lock rejects later unjournaled edits and only a successful backup unlocks editing', async () => {
  const h = harness(); const store = h.store(); await store.loadNotes(h.target);
  store.editNotes(h.target, { markdown: 'injected new edit', json: [] }, Promise.resolve('injected new edit')); await settle();
  expect(store.getNotesSnapshot(h.target).content).toEqual(exact);
  expect(store.getNotesSnapshot(h.target).error).toContain('Editing is locked');
  await expect(store.retryNotesBackup(h.target)).rejects.toThrow('Storage full');
  expect(h.rows.get(h.key)).toBe(h.raw);
  expect(store.getNotesSnapshot(h.target).backupBlocked).toBe(true);
  h.setQuota(false); await store.retryNotesBackup(h.target);
  expect(store.getNotesSnapshot(h.target).backupBlocked).toBe(false);
  expect(h.native.markdown).toBe(exact.markdown);
});

test('multiple retained drafts and CAS conflicts do not erase either writer during quota recovery', async () => {
  const h = harness(); const otherKey = `af-notes-v2:draft:${encodeURIComponent(h.target)}:other`;
  const otherRaw = JSON.stringify({ target: h.target, content: { markdown: 'Other draft', json: [] }, revision: 0, converting: false });
  h.rows.set(otherKey, otherRaw);
  const store = h.store(); await store.loadNotes(h.target);
  expect(store.getNotesSnapshot(h.target).recoveryDrafts).toHaveLength(2);
  await store.recoverNotesDraft(h.target, h.key);
  expect(store.getNotesSnapshot(h.target).loaded).toBe(true);
  h.conflict();
  await expect(store.saveRetainedNotes(h.target)).rejects.toThrow('Notes conflict');
  expect(h.rows.get(h.key)).toBe(h.raw); expect(h.rows.get(otherKey)).toBe(otherRaw);
  await store.saveReviewedNotes(h.target, 1);
  expect(h.native.markdown).toBe(exact.markdown);
  expect(h.rows.has(h.key)).toBe(false); expect(h.rows.get(otherKey)).toBe(otherRaw);
  const restarted = h.store(); await restarted.loadNotes(h.target);
  expect(restarted.getNotesSnapshot(h.target).content.markdown).toBe('Other draft');
  expect(h.rows.get(otherKey)).toBe(otherRaw);
});


test('native readback accepts reordered JSON object keys but preserves rich values and array order', async () => {
  const h = harness(); const store = h.store(); await store.loadNotes(h.target);
  h.reorderNativeJson(); await store.saveRetainedNotes(h.target);
  expect(h.rows.has(h.key)).toBe(false);
  expect(store.getNotesSnapshot(h.target).state).toBe('saved');
  expect(h.native.json).toEqual(exact.json);
});


test('a retained v2 snapshot with legacy ancestry commits without allocating cleanup receipts', async () => {
  const h = harness(); const legacyKey = `af-notes-v1:${h.target}`;
  const legacyRaw = JSON.stringify({ content: { markdown: 'Superseded old text', json: [] }, revision: 0, converting: false });
  h.rows.set(legacyKey, legacyRaw);
  const source = JSON.parse(h.raw);
  source.supersedes = [{ key: legacyKey, raw: legacyRaw, legacy: true }];
  const retained = JSON.stringify(source); h.rows.set(h.key, retained);
  const store = h.store(); await store.loadNotes(h.target); await store.saveRetainedNotes(h.target);
  expect(h.native.markdown).toBe(exact.markdown);
  expect(store.getNotesSnapshot(h.target).state).toBe('saved');
  expect(h.rows.get(h.key)).toBe(retained); expect(h.rows.get(legacyKey)).toBe(legacyRaw);
  const restarted = h.store(); await restarted.loadNotes(h.target);
  expect(restarted.getNotesSnapshot(h.target).content).toEqual(exact);
  expect(restarted.getNotesSnapshot(h.target).recoveryDrafts).toBeUndefined();
});
