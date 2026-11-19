'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { recordingSources, setRecordingSourceEnabled, type RecordingSources } from '@/lib/recording-sources';

const CHANGED = 'meetily-recording-sources-changed';

export function useRecordingSources() {
  const [sources, setSources] = useState<RecordingSources | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  const revision = useRef(0);

  const retry = useCallback(async () => {
    const before = ++revision.current;
    setSources(null);
    try {
      const value = await invoke('get_recording_preferences');
      if (mounted.current && before === revision.current) {
        setSources(recordingSources(value));
        setError(null);
      }
    } catch (error) {
      if (mounted.current && before === revision.current) setError(String(error));
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    // Replies and events can arrive out of commit order across windows.
    // Both invalidate the view; only a fresh preference read supplies its state.
    const refresh = () => { if (active) void retry(); };
    window.addEventListener(CHANGED, refresh);
    const unlisten = listen('recording-sources-changed', refresh);
    void unlisten.then(refresh, refresh);
    return () => {
      mounted.current = false;
      active = false;
      revision.current++;
      window.removeEventListener(CHANGED, refresh);
      void unlisten.then(stop => stop()).catch(console.error);
    };
  }, [retry]);

  const setEnabled = useCallback((source: 'microphone' | 'system', enabled: boolean) => {
    setSaving(true);
    void setRecordingSourceEnabled(source, enabled).then(() => {
      window.dispatchEvent(new Event(CHANGED));
    }).catch(error => {
      if (mounted.current) toast.error('Could not save recording sources', { description: String(error) });
    }).finally(() => { if (mounted.current) setSaving(false); });
  }, []);

  return { sources, saving, error, retry, setEnabled };
}
