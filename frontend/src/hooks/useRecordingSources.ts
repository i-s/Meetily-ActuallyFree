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
  const mounted = useRef(false);
  const revision = useRef(0);

  useEffect(() => {
    mounted.current = true;
    const accept = (value: unknown) => {
      const next = recordingSources(value);
      revision.current++;
      if (mounted.current) setSources(next);
    };
    const local = (event: Event) => accept((event as CustomEvent).detail);
    window.addEventListener(CHANGED, local);
    const unlisten = listen<RecordingSources>('recording-sources-changed', event => accept(event.payload));
    const before = revision.current;
    void invoke('get_recording_preferences').then(value => {
      if (mounted.current && before === revision.current) accept(value);
    }).catch(error => {
      if (mounted.current) toast.error('Could not load recording sources', { description: String(error) });
    });
    return () => {
      mounted.current = false;
      window.removeEventListener(CHANGED, local);
      void unlisten.then(stop => stop()).catch(console.error);
    };
  }, []);

  const setEnabled = useCallback((source: 'microphone' | 'system', enabled: boolean) => {
    setSaving(true);
    void setRecordingSourceEnabled(source, enabled).then(next => {
      window.dispatchEvent(new CustomEvent(CHANGED, { detail: next }));
    }).catch(error => {
      if (mounted.current) toast.error('Could not save recording sources', { description: String(error) });
    }).finally(() => { if (mounted.current) setSaving(false); });
  }, []);

  return { sources, saving, setEnabled };
}
