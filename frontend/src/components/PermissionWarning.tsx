import React from 'react';
import { AlertTriangle, Mic, RefreshCw, Speaker } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import type { SystemAudioPermission } from '@/lib/system-audio-permission';
import { useIsLinux } from '@/hooks/usePlatform';

interface PermissionWarningProps {
  hasMicrophone: boolean;
  systemAudio: SystemAudioPermission;
  onRecheck: () => void;
  isRechecking?: boolean;
  className?: string;
}

/** Explains a missing microphone or system audio permission, with ways to fix it. */
export function PermissionWarning({
  hasMicrophone,
  systemAudio,
  onRecheck,
  isRechecking = false,
  className,
}: PermissionWarningProps) {
  const isLinux = useIsLinux();

  const hasSystemAudio = systemAudio === 'verified';
  const systemAudioDenied = systemAudio === 'denied';

  // Linux has no permission prompts; nothing to explain when both work.
  if (isLinux || (hasMicrophone && hasSystemAudio)) return null;

  const isMacOS = navigator.userAgent.includes('Mac');
  const openSettings = (preferencePane: string) =>
    invoke('open_system_settings', { preferencePane }).catch((error) =>
      console.error(`Failed to open ${preferencePane} settings:`, error),
    );

  const title = !hasMicrophone
    ? systemAudioDenied ? 'Meetily can’t hear your microphone or computer audio' : 'Meetily can’t hear your microphone'
    : systemAudioDenied ? 'Computer audio permission was denied' : 'Computer audio has not been verified';

  return (
    <Alert variant="warning" className={className}>
      <AlertTriangle className="h-4 w-4" />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription className="space-y-2 text-af-text-2">
        {!hasMicrophone && (
          <p>
            No microphone was found. Check that one is connected, that Meetily is allowed to use it in your system settings, and that no other
            app has taken it over.
          </p>
        )}
        {!hasSystemAudio && (
          <p>
            {systemAudioDenied
              ? 'Allow Audio Capture for Meetily in system settings, then play audio and check again.'
              : 'Play some computer audio and check again. A quiet check cannot confirm whether capture is available.'}
          </p>
        )}
        <div className="flex flex-wrap gap-2 pt-1">
          {isMacOS && !hasMicrophone && (
            <Button size="sm" variant="secondary" onClick={() => openSettings('Privacy_Microphone')}>
              <Mic />
              Microphone settings
            </Button>
          )}
          {isMacOS && systemAudioDenied && (
            <Button size="sm" variant="secondary" onClick={() => openSettings('Privacy_AudioCapture')}>
              <Speaker />
              Audio Capture settings
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={onRecheck} disabled={isRechecking}>
            <RefreshCw className={isRechecking ? 'animate-spin' : undefined} />
            Check again
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  );
}
