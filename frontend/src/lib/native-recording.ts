import { invoke } from '@tauri-apps/api/core';

export interface LifecycleOwner {
  source: string;
  session_id: string;
  app: string;
  process: string;
}
export interface RecordingLease {
  generation: string;
  owner: LifecycleOwner | null;
}
export interface NativeRecordingState {
  is_recording?: boolean;
  is_stopping?: boolean;
  recording_lease?: RecordingLease | null;
}

/** A rejected automated stop has NO frontend stop/postprocessing effects. The
 * native engine lock performs the decisive comparison, not this WebView check.
 */
export async function stopRecordingRequest(savePath: string, expected?: RecordingLease): Promise<boolean> {
  return invoke<boolean>('stop_recording', { args: {
    save_path: savePath,
    ...(expected ? { expected } : {}),
  } });
}
