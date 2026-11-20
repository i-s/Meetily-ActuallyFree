'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ASK_AI_SETTINGS_CHANGED } from '@/lib/ask-ai-context';
import type { AskAiContextRange } from '@/lib/workspace-api';

/** Preview uses the same backend selector as Send. Each saved answer retains
 * its own snapshot; this value is only the moving boundary for the next send. */
export function useAskAiContext(meetingId: string | null, revision: unknown, live: boolean) {
  const [state, setState] = useState<{ id: string | null; range: AskAiContextRange | null; error: string | null }>({ id: null, range: null, error: null });
  useEffect(() => {
    let alive = true;
    let sequence = 0;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      if (!meetingId) return;
      const request = ++sequence;
      try {
        const range = await invoke<AskAiContextRange>('get_meeting_chat_context', { meetingId });
        if (alive && request === sequence) setState({ id: meetingId, range, error: null });
      } catch (error) {
        if (alive && request === sequence) setState({ id: meetingId, range: null, error: String(error) });
      }
    };
    void refresh();
    const changed = () => void refresh();
    window.addEventListener(ASK_AI_SETTINGS_CHANGED, changed);
    void listen('ask-ai-context-settings-changed', changed).then(stop => { if (alive) unlisten = stop; else stop(); }).catch(() => {});
    // Native persistence can complete after the corresponding UI event.
    const timer = live ? setInterval(changed, 2000) : undefined;
    return () => {
      alive = false;
      window.removeEventListener(ASK_AI_SETTINGS_CHANGED, changed);
      unlisten?.();
      if (timer) clearInterval(timer);
    };
  }, [meetingId, revision, live]);
  return state.id === meetingId ? state : { id: meetingId, range: null, error: null };
}
