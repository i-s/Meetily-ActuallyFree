import { listen } from '@tauri-apps/api/event';

/** Native publication is durable; this listener only refreshes normal readers.
 * Focus/mount reads cover a WebView that missed the process-owned event. */
export function onPostCallCanonicalChange(changed: () => void): () => void {
  let disposed = false;
  let stop: (() => void) | undefined;
  void listen('post-call-queue-changed', () => { if (!disposed) changed(); })
    .then(unlisten => { if (disposed) unlisten(); else stop = unlisten; })
    .catch(() => {});
  window.addEventListener('focus', changed);
  return () => { disposed = true; stop?.(); window.removeEventListener('focus', changed); };
}
