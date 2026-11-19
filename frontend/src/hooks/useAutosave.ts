'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

export type SaveState = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

/**
 * Debounced saving for editors. `schedule` queues the latest value; it is
 * written after `delay` ms of quiet, and any pending write is flushed when the
 * component unmounts, so leaving the page never loses the last keystrokes.
 */
export function useAutosave<T>(save: (value: T) => Promise<void>, delay = 800) {
  const [state, setState] = useState<SaveState>('idle');
  const pending = useRef<{ value: T } | null>(null);
  const timer = useRef<number | null>(null);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const saveRef = useRef(save);
  saveRef.current = save;

  const flush = useCallback(async (): Promise<boolean> => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    // A transcript switch must await a write already in flight, not just the
    // debounce buffer. Drain newer edits serially so old replies cannot win.
    if (inFlight.current) return inFlight.current;
    const write = async () => {
      while (pending.current) {
        const next = pending.current;
        const writer = saveRef.current;
        pending.current = null;
        setState('saving');
        try {
          await writer(next.value);
        } catch (error) {
          if (!pending.current) pending.current = next;
          console.error('Autosave failed', error);
          setState('error');
          return false;
        }
      }
      setState('saved');
      return true;
    };
    const work = write();
    inFlight.current = work;
    try { return await work; }
    finally { inFlight.current = null; }
  }, []);

  const schedule = useCallback(
    (value: T) => {
      pending.current = { value };
      setState('pending');
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => void flush(), delay);
    },
    [delay, flush],
  );

  useEffect(() => () => void flush(), [flush]);

  return { schedule, flush, state };
}

export function saveStateLabel(state: SaveState): string {
  switch (state) {
    case 'pending':
    case 'saving':
      return 'Saving…';
    case 'saved':
      return 'Saved';
    case 'error':
      return 'Not saved. Retrying on next edit.';
    default:
      return '';
  }
}
