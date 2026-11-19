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

  const accept = useCallback((value: unknown) => {
    const next = recordingSources(value);
    revision.current++;
    if (mounted.current) { setSources(next); setError(null); }
  }, []);

  const retry = useCallback(async () => {
    const before = ++revision.current;
    try {
      const value = await invoke('get_recording_preferences');
      if (mounted.current && before === revision.current) accept(value);
    } catch (error) {
      if (mounted.current && before === revision.current) setError(String(error));
    }
  }, [accept]);

  useEffect(() => {
    mounted.current = true;
    const local = (event: Event) => accept((event as CustomEvent).detail);
    window.addEventListener(CHANGED, local);
    const unlisten = listen<RecordingSources>('recording-sources-changed', event => accept(event.payload));
    void retry();
    return () => {
      mounted.current = false;
      revision.current++;
      window.removeEventListener(CHANGED, local);
      void unlisten.then(stop => stop()).catch(console.error);
    };
  }, [accept, retry]);

  const setEnabled = useCallback((source: 'microphone' | 'system', enabled: boolean) => {
    setSaving(true);
    void setRecordingSourceEnabled(source, enabled).then(next => {
      window.dispatchEvent(new CustomEvent(CHANGED, { detail: next }));
    }).catch(error => {
      if (mounted.current) toast.error('Could not save recording sources', { description: String(error) });
    }).finally(() => { if (mounted.current) setSaving(false); });
  }, []);

  return { sources, saving, error, retry, setEnabled };
}
