import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { usePlatform } from './usePlatform';
import { applySystemAudioProbe, readSystemAudioPermission, SystemAudioPermission, SystemAudioProbeResult } from '@/lib/system-audio-permission';
export { MACOS_SYSTEM_AUDIO_VERIFIED_KEY } from '@/lib/system-audio-permission';

export interface PermissionStatus {
  hasMicrophone: boolean;
  systemAudio: SystemAudioPermission;
  isChecking: boolean;
  error: string | null;
}

// Device availability is cached for a smooth remount; permission evidence lives
// in the versioned session cache and is never inferred from an output device.
let lastMicrophone: boolean | null = null;

export function usePermissionCheck() {
  const platform = usePlatform();
  const requestInFlight = useRef<Promise<void> | null>(null);
  const evidence = useRef<SystemAudioPermission>(readSystemAudioPermission());
  const [status, setStatus] = useState<PermissionStatus>(() => ({
    hasMicrophone: lastMicrophone ?? false,
    systemAudio: evidence.current,
    isChecking: lastMicrophone === null,
    error: null,
  }));

  const checkPermissions = async () => {
    try {
      const devices = await invoke<Array<{ name: string; device_type: 'Input' | 'Output' }>>('get_audio_devices');
      const hasMicrophone = devices.some(d => d.device_type === 'Input');
      const cached = readSystemAudioPermission();
      if (cached !== 'unknown') evidence.current = cached;
      const systemAudio = platform === 'macos' || platform === 'unknown'
        ? evidence.current
        : devices.some(d => d.device_type === 'Output') ? 'verified' : 'unknown';
      lastMicrophone = hasMicrophone;
      setStatus({ hasMicrophone, systemAudio, isChecking: false, error: null });
    } catch (error) {
      // Enumeration failures do not revoke prior capture evidence.
      setStatus(prev => ({ ...prev, isChecking: false,
        error: error instanceof Error ? error.message : 'Failed to check permissions' }));
    }
  };

  const requestPermissions = () => {
    // Deduplicate the bounded native probe; a slower attempt cannot overwrite a
    // later one in this hook. Silence and IPC errors never poison the cache.
    if (requestInFlight.current) return requestInFlight.current;
    const request = (async () => {
      setStatus(prev => ({ ...prev, isChecking: true, error: null }));
      try {
        await invoke('trigger_microphone_permission');
        if (platform === 'macos') {
          const result = await invoke<SystemAudioProbeResult>('trigger_system_audio_permission_command');
          evidence.current = applySystemAudioProbe(result, evidence.current);
          setStatus(prev => ({ ...prev, systemAudio: evidence.current }));
        }
        await new Promise(resolve => setTimeout(resolve, 1000));
        await checkPermissions();
      } catch (error) {
        setStatus(prev => ({ ...prev,
          error: error instanceof Error ? error.message : 'Could not verify audio. Play audio and check again.' }));
      } finally {
        requestInFlight.current = null;
        setStatus(prev => ({ ...prev, isChecking: false }));
      }
    })();
    requestInFlight.current = request;
    return request;
  };

  useEffect(() => { void checkPermissions(); }, [platform]);

  return { ...status, checkPermissions, requestPermissions };
}
