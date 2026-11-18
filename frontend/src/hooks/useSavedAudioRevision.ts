import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

/** Saved audio is a filesystem observation, not a durable retention lease.
 * Native cleanup invalidates even an already-open player in another WebView. */
export function useSavedAudioRevision() {
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    const changed = () => { if (active) setRevision(value => value + 1); };
    if (typeof window !== 'undefined') window.addEventListener('focus', changed);
    // Focus also re-observes files removed outside this WebView. Native commands
    // still revalidate audio if event registration is unavailable.
    void listen('audio-cleanup-finished', changed).then(unlisten => {
      if (active) stop = unlisten; else unlisten();
    }).catch(() => {});
    return () => { active = false; stop?.(); if (typeof window !== 'undefined') window.removeEventListener('focus', changed); };
  }, []);
  return revision;
}
