import { invoke } from '@tauri-apps/api/core';

export interface RecordingSources {
  microphone_enabled: boolean;
  system_audio_enabled: boolean;
}

export function recordingSources(value: unknown): RecordingSources {
  const sources = value as Partial<RecordingSources> | null;
  if (typeof sources?.microphone_enabled !== 'boolean' || typeof sources?.system_audio_enabled !== 'boolean') {
    throw new Error('Recording source preferences are unavailable. No source was enabled automatically.');
  }
  return { microphone_enabled: sources.microphone_enabled, system_audio_enabled: sources.system_audio_enabled };
}

// Shared by all hook instances and every routed Start. A failed durable write
// fences Start until an explicit successful retry, rather than using stale UI.
let pending: Promise<RecordingSources | void> = Promise.resolve();
export function setRecordingSourceEnabled(source: 'microphone' | 'system', enabled: boolean): Promise<RecordingSources> {
  const next = pending.catch(() => undefined).then(async () => recordingSources(
    await invoke('set_recording_source_enabled', { source, enabled }),
  ));
  pending = next;
  return next;
}

export async function flushRecordingSources(): Promise<void> {
  let observed;
  do { observed = pending; await observed; } while (observed !== pending);
}
