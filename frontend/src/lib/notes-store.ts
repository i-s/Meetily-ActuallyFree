import { invoke } from '@tauri-apps/api/core';

export interface NotesContent { markdown: string; json: unknown[] | null }
export interface NotesDocument extends NotesContent { revision: number; updatedAt?: string }
export interface NotesSnapshot {
  loaded: boolean;
  content: NotesContent;
  state: 'loading' | 'saved' | 'pending' | 'saving' | 'error';
  error: string | null;
  backupBlocked?: boolean;
  recoveryPaused?: boolean;
  converting?: boolean;
  warning?: string | null;
  recoveryDrafts?: Array<{ key: string; content: NotesContent; converting: boolean }>;
}
interface DraftRef { key: string; raw?: string; legacy?: boolean }
interface Draft {
  target: string;
  content: NotesContent;
  revision: number;
  converting: boolean;
  supersedes?: DraftRef[];
}
interface Entry {
  snapshot: NotesSnapshot;
  revision: number;
  generation: number;
  dirty: boolean;
  backupBlocked: boolean;
  paused: boolean;
  journal?: DraftRef;
  sources: DraftRef[];
  converting: boolean;
  timer?: ReturnType<typeof setTimeout>;
  loading?: Promise<void>;
  saving?: Promise<void>;
  listeners: Set<() => void>;
}
const draftPrefix = (target: string) => `af-notes-v2:draft:${encodeURIComponent(target)}:`;
const receiptPrefix = (target: string) => `af-notes-v2:receipt:${encodeURIComponent(target)}:`;
const legacyKey = (target: string) => `af-notes-v1:${target}`;
const message = (e: unknown) => e instanceof Error ? e.message : String(e);
// Native serde_json may reorder object properties. Preserve rich block values
// and array order, without treating JSON object insertion order as content.
function sameJson(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (left === null || right === null || typeof left !== 'object' || typeof right !== 'object') return false;
  if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.every((value, index) => sameJson(value, right[index]));
  const a = left as Record<string, unknown>;
  const b = right as Record<string, unknown>;
  return Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(key => Object.prototype.hasOwnProperty.call(b, key) && sameJson(a[key], b[key]));
}

/** One instance per WebView. Tests use independent instances sharing Storage and
 * native transport, not a shared JS Map masquerading as independent editors. */
export function createNotesStore(options: { invoke?: typeof invoke; storage?: Storage } = {}) {
  const transport = options.invoke ?? invoke;
  const storage = () => options.storage ?? localStorage;
  const entries = new Map<string, Entry>();
  function entryFor(target: string): Entry {
    let entry = entries.get(target);
    if (!entry) {
      entry = { snapshot: { loaded: false, content: { markdown: '', json: null }, state: 'loading', error: null }, revision: 0, generation: 0, dirty: false, backupBlocked: false, paused: false, sources: [], converting: false, listeners: new Set() };
      entries.set(target, entry);
    }
    return entry;
  }
  function publish(entry: Entry, patch: Partial<NotesSnapshot>) {
    entry.snapshot = { ...entry.snapshot, backupBlocked: entry.backupBlocked, recoveryPaused: entry.paused, converting: entry.converting, ...patch };
    entry.listeners.forEach(fn => fn());
  }
  function backupFailure(entry: Entry, error: unknown) {
    entry.backupBlocked = true;
    publish(entry, { state: 'error', error: `Recovery backup failed: ${message(error)}`, warning: 'Browser recovery storage is unavailable. Editing is locked; current changes are not newly journaled. Retained original drafts have not been erased.' });
  }
  function uniqueRefs(refs: DraftRef[]) {
    return refs.filter((ref, index) => refs.findIndex(other => other.key === ref.key && other.raw === ref.raw) === index);
  }
  function journal(target: string, entry: Entry) {
    const previous = entry.journal;
    const supersedes = uniqueRefs([...entry.sources, ...(previous ? [previous] : [])]);
    const raw = JSON.stringify({ target, content: entry.snapshot.content, revision: entry.revision, converting: entry.converting, supersedes } satisfies Draft);
    // Immutable, unguessably unique key per snapshot, never overwritten by this
    // or any other writer. No getItem/removeItem compare-and-delete is assumed.
    const key = `${draftPrefix(target)}${crypto.randomUUID()}`;
    storage().setItem(key, raw);
    entry.journal = { key };
    // Only this instance creates previous.key. A crash between set and remove
    // leaves two snapshots, linked by supersedes for deterministic recovery.
    if (previous) storage().removeItem(previous.key);
  }
  function retire(refs: DraftRef[]) {
    for (const ref of uniqueRefs(refs)) {
      if (ref.legacy) {
        // v1 keys are mutable across old WebViews. Keep them untouched and write
        // an immutable exact-value receipt: B may replace the key between any
        // read and remove. A different raw value is never covered by this receipt.
        storage().setItem(`${receiptPrefix(ref.key.slice('af-notes-v1:'.length))}${crypto.randomUUID()}`, JSON.stringify(ref));
      } else {
        // v2 keys are immutable. A concurrent writer uses another key even if
        // they edit the same target or recover this snapshot.
        storage().removeItem(ref.key);
      }
    }
  }
  function recoveryCandidates(targets: string[]) {
    const candidates: Array<{ ref: DraftRef; draft: Draft }> = [];
    const receipts: DraftRef[] = [];
    const keys = Array.from({ length: storage().length }, (_, index) => storage().key(index)).filter((key): key is string => key !== null);
    for (const target of targets) {
      const raw = storage().getItem(legacyKey(target));
      if (raw) {
        const draft = JSON.parse(raw);
        const inherited = draft.inheritedAlias;
        candidates.push({ ref: { key: legacyKey(target), raw, legacy: true }, draft: { ...draft, target, supersedes: inherited ? [{ key: legacyKey(inherited.target), raw: inherited.raw, legacy: true }] : [] } });
      }
    }
    for (const key of keys) {
      const target = targets.find(target => key.startsWith(draftPrefix(target)) || key.startsWith(receiptPrefix(target)));
      if (!target) continue;
      const raw = storage().getItem(key);
      if (raw === null) continue;
      if (key.startsWith(receiptPrefix(target))) receipts.push(JSON.parse(raw));
      else {
        const draft = JSON.parse(raw) as Draft;
        if (draft.target !== target) throw new Error('Invalid Notes recovery identity. Draft retained.');
        candidates.push({ ref: { key }, draft });
      }
    }
    for (const { draft } of candidates) {
      if (typeof draft.content?.markdown !== 'string' || !(draft.content.json === null || Array.isArray(draft.content.json)) || !Number.isSafeInteger(draft.revision) || draft.revision < 0) throw new Error('Invalid Notes recovery draft. It has been retained.');
    }
    const superseded = [...receipts, ...candidates.flatMap(({ draft }) => draft.supersedes ?? [])];
    return candidates.filter(({ ref }) => !superseded.some(other => other.key === ref.key && other.raw === ref.raw));
  }
  const readNotesDocument = (target: string) => transport<NotesDocument | null>('api_read_notes_document', { target });
  const currentRecordingNotesTarget = async () => {
    const folder = await transport<string | null>('get_meeting_folder_path');
    if (!folder) throw new Error('The recording has no durable Notes identity yet. Retry after recording starts.');
    return `recording:${folder}`;
  };
  const getNotesSnapshot = (target: string) => entryFor(target).snapshot;
  function subscribeNotes(target: string, listener: () => void) {
    const entry = entryFor(target);
    entry.listeners.add(listener);
    return () => { entry.listeners.delete(listener); };
  }
  function loadNotes(target: string, refresh = false, recoveryKey?: string): Promise<void> {
    const entry = entryFor(target);
    // Transcript/version/summary refetches do not own dirty Notes, including
    // conversions and native writes still in flight.
    if (entry.snapshot.loaded && (!refresh || entry.dirty || entry.saving)) return Promise.resolve();
    if (entry.loading) return entry.loading;
    publish(entry, { loaded: false, state: 'loading', error: null, recoveryDrafts: undefined });
    entry.loading = (async () => {
      try {
        const alias = target.startsWith('meeting:')
          ? await transport<string | null>('api_notes_recording_target', { meetingId: target.slice(8) }) : null;
        if (alias && entries.has(alias)) await flushNotes(alias);
        const stored = await readNotesDocument(target);
        const candidates = recoveryCandidates(alias ? [target, alias] : [target]);
        if (candidates.length > 1 && !recoveryKey) {
          publish(entry, { state: 'error', error: 'Multiple Notes recovery drafts exist. Review and choose a draft; other drafts will be retained.', recoveryDrafts: candidates.map(({ ref, draft }) => ({ key: ref.key, content: draft.content, converting: draft.converting })) });
          return;
        }
        const selected = recoveryKey ? candidates.find(({ ref }) => ref.key === recoveryKey) : candidates[0];
        if (recoveryKey && !selected) throw new Error('The selected Notes draft changed. Reload the recovery choices.');
        entry.revision = selected?.draft.revision ?? stored?.revision ?? 0;
        entry.dirty = !!selected;
        entry.converting = !!selected?.draft.converting;
        entry.sources = selected ? [selected.ref, ...(selected.draft.supersedes ?? [])] : [];
        entry.snapshot = { ...entry.snapshot, content: selected?.draft.content ?? { markdown: stored?.markdown ?? '', json: stored?.json ?? null } };
        entry.backupBlocked = false;
        entry.paused = false;
        try { if (selected) journal(target, entry); }
        catch (error) { backupFailure(entry, error); }
        // Recovery can read/reconvert the original immutable source and commit
        // it natively even when Storage cannot allocate a copy. Keep that source
        // intact, mount read-only, and require an explicit native-save action.
        publish(entry, { loaded: true, state: entry.backupBlocked ? 'error' : selected ? 'pending' : 'saved', error: entry.backupBlocked ? entry.snapshot.error : null, warning: entry.backupBlocked ? entry.snapshot.warning : null, recoveryDrafts: undefined });
        if (selected && !entry.converting && !entry.backupBlocked) void flushNotes(target);
      } catch (e) {
        publish(entry, { state: 'error', error: message(e) });
      } finally { entry.loading = undefined; }
    })();
    return entry.loading;
  }
  const recoverNotesDraft = (target: string, key: string) => loadNotes(target, true, key);

  /** Exact editor JSON is journaled before asynchronous conversion starts. */
  function editNotes(target: string, content: NotesContent, markdown: Promise<string>) {
    const entry = entryFor(target);
    if (entry.paused) { void markdown.catch(() => {}); return; }
    if (entry.backupBlocked && !sameJson(content.json, entry.snapshot.content.json)) {
      void markdown.catch(() => {});
      publish(entry, { state: 'error', error: 'Editing is locked until recovery backup succeeds. The retained JSON has not been changed.' });
      return;
    }
    const generation = ++entry.generation;
    const previousMarkdown = entry.snapshot.content.markdown;
    // A read-only readiness/reconversion callback is not a new user edit.
    // After native recovery, reconverting the same blocks must not fabricate
    // another unsaved document solely because the editor remounted.
    if (!entry.backupBlocked) entry.dirty = true;
    entry.converting = true;
    publish(entry, { content: structuredClone(content), state: 'pending', error: null });
    if (!entry.backupBlocked) {
      try { journal(target, entry); }
      catch (error) { backupFailure(entry, error); }
    }
    void markdown.then(value => {
      if (generation !== entry.generation || entry.paused) return;
      entry.converting = false;
      if (entry.backupBlocked && value !== previousMarkdown) entry.dirty = true;
      publish(entry, { content: { ...entry.snapshot.content, markdown: value }, state: entry.dirty ? 'pending' : 'saved' });
      if (!entry.backupBlocked) {
        try { journal(target, entry); }
        catch (error) { backupFailure(entry, error); }
      }
      clearTimeout(entry.timer);
      if (!entry.backupBlocked) entry.timer = setTimeout(() => void flushNotes(target), 400);
    }).catch(e => {
      if (generation === entry.generation) publish(entry, { state: 'error', error: message(e) });
    });
  }

  /** One writer per target in this WebView; SQLite revisions arbitrate others. */
  async function flushNotes(target: string, explicitRecovery = false): Promise<void> {
    const entry = entryFor(target);
    clearTimeout(entry.timer);
    if (entry.saving) return entry.saving;
    if (!entry.dirty || entry.converting || !entry.snapshot.loaded || entry.paused || (entry.backupBlocked && !explicitRecovery)) return;
    entry.saving = (async () => {
      while (entry.dirty && !entry.converting && !entry.paused) {
        const generation = entry.generation;
        const content = entry.snapshot.content;
        publish(entry, { state: 'saving', error: null });
        try {
          const revision = await transport<number>('api_write_notes_document', { target, ...content, expectedRevision: entry.revision });
          entry.revision = revision;
          if (generation === entry.generation) {
            if (entry.backupBlocked) {
              // Do not retire the only durable browser copy on just a UI promise
              // or failed verification. Native readback must match the exact
              // document/revision acknowledged by the CAS write.
              const verified = await readNotesDocument(target);
              if (entry.generation !== generation || verified?.revision !== revision || verified.markdown !== content.markdown || !sameJson(verified.json ?? null, content.json)) throw new Error('Notes conflict or native recovery verification failed. Original draft retained.');
            }
            const consumed = [...entry.sources, ...(entry.journal ? [entry.journal] : [])];
            // A legacy receipt itself needs storage. With zero capacity, keep
            // the immutable supersession record AND its legacy sources instead
            // of allocating a receipt or exposing an older superseded draft.
            const deferredCleanup = entry.backupBlocked && consumed.some(ref => ref.legacy);
            if (!deferredCleanup) {
              retire(consumed);
              entry.sources = [];
              entry.journal = undefined;
            }
            entry.dirty = false;
            publish(entry, { state: 'saved', error: null, warning: entry.backupBlocked ? `Notes saved natively. Browser recovery storage is unavailable; editing stays locked until Retry recovery backup succeeds.${deferredCleanup ? ' Original recovery records are retained until safe legacy cleanup is possible.' : ''}` : null });
          } else journal(target, entry);
        } catch (e) {
          publish(entry, { state: 'error', error: message(e) });
          break;
        }
      }
    })().finally(() => { entry.saving = undefined; });
    return entry.saving;
  }
  async function saveReviewedNotes(target: string, reviewedRevision: number) {
    const entry = entryFor(target);
    if (entry.saving) throw new Error('Native save is in progress. Review and retry after it completes.');
    if (!entry.snapshot.loaded || entry.converting || entry.paused) throw new Error('Notes conversion is pending or recovery is paused. Resume and retry shortly.');
    entry.revision = reviewedRevision;
    entry.dirty = true;
    if (!entry.backupBlocked) journal(target, entry);
    await flushNotes(target, entry.backupBlocked);
    if (entry.snapshot.error) throw new Error(entry.snapshot.error);
  }

  async function saveRetainedNotes(target: string) {
    const entry = entryFor(target);
    if (!entry.snapshot.loaded || entry.paused || entry.converting) throw new Error('Resume recovery and finish JSON conversion before saving.');
    await flushNotes(target, true);
    if (entry.snapshot.error) throw new Error(entry.snapshot.error);
  }
  function cancelNotesRecovery(target: string) {
    const entry = entryFor(target);
    if (entry.saving) throw new Error('Native save is in progress and cannot be cancelled here. Original drafts remain until verification.');
    clearTimeout(entry.timer);
    ++entry.generation;
    entry.paused = true;
    publish(entry, { state: entry.dirty ? 'pending' : 'saved', error: null, warning: 'Recovery paused. Retained original drafts are unchanged. Resume explicitly or copy the exact JSON/text.' });
  }
  function resumeNotesRecovery(target: string) {
    const entry = entryFor(target);
    entry.paused = false;
    publish(entry, { state: 'pending', error: null, warning: 'Recovery resumed read-only. Convert the retained JSON before explicit native save.' });
  }
  async function retryNotesBackup(target: string) {
    const entry = entryFor(target);
    if (!entry.snapshot.loaded || entry.saving) throw new Error('Wait for Notes loading or native save before retrying recovery backup.');
    try {
      journal(target, entry);
      entry.backupBlocked = false;
      entry.paused = false;
      publish(entry, { state: entry.dirty ? 'pending' : 'saved', error: null, warning: null });
      await flushNotes(target);
    } catch (error) { backupFailure(entry, error); throw error; }
  }

  /** Read boundary for #13/#15/#18: return one immutable committed document,
   * never the current editor JSON paired with an earlier native read. */
  async function notesForContext(target: string): Promise<NotesDocument & { source: 'user_notes' }> {
    await loadNotes(target);
    await flushNotes(target);
    const entry = entryFor(target);
    const generation = entry.generation;
    const revision = entry.revision;
    if (!entry.snapshot.loaded || entry.dirty || entry.converting || entry.snapshot.error) throw new Error(entry.snapshot.error || 'Notes are still being saved. Retry shortly.');
    const committed = structuredClone(await readNotesDocument(target));
    if (entries.get(target) !== entry || entry.generation !== generation || entry.revision !== revision || !entry.snapshot.loaded || entry.dirty || entry.converting || entry.snapshot.error) throw new Error('Notes changed or are pending while context was loading. Retry shortly.');
    if ((committed?.revision ?? 0) !== revision) throw new Error('Notes changed in another editor. Reopen Notes before preparing context.');
    return { markdown: committed?.markdown ?? '', json: committed?.json ?? null, revision, updatedAt: committed?.updatedAt, source: 'user_notes' };
  }
  return { readNotesDocument, currentRecordingNotesTarget, getNotesSnapshot, subscribeNotes, loadNotes, recoverNotesDraft, saveRetainedNotes, cancelNotesRecovery, resumeNotesRecovery, retryNotesBackup, editNotes, flushNotes, saveReviewedNotes, notesForContext };
}

export const { readNotesDocument, currentRecordingNotesTarget, getNotesSnapshot, subscribeNotes, loadNotes, recoverNotesDraft, saveRetainedNotes, cancelNotesRecovery, resumeNotesRecovery, retryNotesBackup, editNotes, flushNotes, saveReviewedNotes, notesForContext } = createNotesStore();
