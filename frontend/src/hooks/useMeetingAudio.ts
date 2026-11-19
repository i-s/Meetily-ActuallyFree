'use client';

/**
 * Plays a meeting's recording. Rust finds the file and allows it through the
 * asset protocol; the page gets a streaming URL plus transport controls. The
 * clock ticks on animation frames while playing so the transcript highlight
 * and scrubber move smoothly.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { getMeetingAudio } from '@/lib/workspace-api';
import { useSavedAudioRevision } from '@/hooks/useSavedAudioRevision';

export type AudioStatus = 'loading' | 'ready' | 'unavailable' | 'error';

export interface MeetingAudioControls {
  status: AudioStatus;
  /** The recording's file, once found. */
  path: string | null;
  playing: boolean;
  currentTime: number;
  duration: number;
  rate: number;
  play: () => void;
  pause: () => void;
  toggle: () => void;
  seek: (seconds: number, autoplay?: boolean) => void;
  skip: (delta: number) => void;
  setRate: (rate: number) => void;
}

export const PLAYBACK_RATES = [1, 1.25, 1.5, 1.75, 2];
/** Labs waveform scrubbing adds slower speeds for close listening. */
export const SLOW_PLAYBACK_RATES = [0.5, 0.75, ...PLAYBACK_RATES];

export function useMeetingAudio(meetingId: string | null | undefined): MeetingAudioControls {
  const savedAudioRevision = useSavedAudioRevision(meetingId);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const frame = useRef<number | null>(null);
  const checkGeneration = useRef(0);
  const sourceRef = useRef<{ path: string; revision: string | null } | null>(null);
  const rateRef = useRef(1);
  const [status, setStatus] = useState<AudioStatus>('loading');
  const [path, setPath] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [rate, setRateState] = useState(1);

  const releaseAudio = useCallback(() => {
    const element = audioRef.current;
    audioRef.current = null;
    sourceRef.current = null;
    if (!element) return;
    element.onloadedmetadata = element.onerror = element.onplay = element.onpause = element.onended = element.ontimeupdate = null;
    element.pause();
    element.removeAttribute('src');
    element.load();
  }, []);

  // Navigation owns disposal; an observation/focus change does not own the
  // existing element and must not stop unchanged playback.
  useEffect(() => {
    ++checkGeneration.current;
    releaseAudio();
    setStatus(meetingId ? 'loading' : 'unavailable');
    setPath(null); setPlaying(false); setCurrentTime(0); setDuration(0);
    return () => { ++checkGeneration.current; releaseAudio(); };
  }, [meetingId, releaseAudio]);

  useEffect(() => {
    const generation = ++checkGeneration.current;
    if (!meetingId) return;
    void getMeetingAudio(meetingId).then(audio => {
      if (generation !== checkGeneration.current) return;
      const revision = audio.playbackRevision ?? null;
      const current = sourceRef.current;
      if (audio.path && audioRef.current && current?.path === audio.path && current.revision === revision) return;
      releaseAudio();
      setPlaying(false); setCurrentTime(0); setDuration(0); setPath(audio.path);
      if (!audio.path) { setStatus('unavailable'); return; }
      setStatus('loading');
      const element = new Audio();
      const ownsElement = () => audioRef.current === element;
      element.preload = 'metadata';
      element.playbackRate = rateRef.current;
      element.onloadedmetadata = () => {
        if (!ownsElement()) return;
        setDuration(Number.isFinite(element.duration) ? element.duration : 0);
        setStatus('ready');
      };
      element.onerror = () => { if (ownsElement()) setStatus('error'); };
      element.onplay = () => { if (ownsElement()) setPlaying(true); };
      element.onpause = element.onended = () => { if (ownsElement()) setPlaying(false); };
      element.ontimeupdate = () => { if (ownsElement()) setCurrentTime(element.currentTime); };
      audioRef.current = element;
      sourceRef.current = { path: audio.path, revision };
      const url = convertFileSrc(audio.path);
      // A replacement at the same path must not reuse cached old media bytes.
      element.src = revision ? `${url}${url.includes('?') ? '&' : '?'}revision=${encodeURIComponent(revision)}` : url;
    }).catch(() => {
      // A failed observation is not proof an already-playing track disappeared.
      if (generation === checkGeneration.current && !audioRef.current) setStatus('error');
    });
    return () => { ++checkGeneration.current; };
  }, [meetingId, savedAudioRevision, releaseAudio]);

  // Smooth clock while playing.
  useEffect(() => {
    if (!playing) return;
    const tick = () => {
      const element = audioRef.current;
      if (element) setCurrentTime(element.currentTime);
      frame.current = requestAnimationFrame(tick);
    };
    frame.current = requestAnimationFrame(tick);
    return () => {
      if (frame.current !== null) cancelAnimationFrame(frame.current);
    };
  }, [playing]);

  const play = useCallback(() => {
    const element = audioRef.current;
    void element?.play().catch(() => { if (audioRef.current === element) setStatus('error'); });
  }, []);
  const pause = useCallback(() => audioRef.current?.pause(), []);
  const toggle = useCallback(() => {
    const element = audioRef.current;
    if (!element) return;
    if (element.paused) void element.play().catch(() => { if (audioRef.current === element) setStatus('error'); });
    else element.pause();
  }, []);
  const seek = useCallback((seconds: number, autoplay = false) => {
    const element = audioRef.current;
    if (!element) return;
    const limit = Number.isFinite(element.duration) ? element.duration : seconds;
    element.currentTime = Math.max(0, Math.min(limit, seconds));
    setCurrentTime(element.currentTime);
    if (autoplay && element.paused) void element.play().catch(() => { if (audioRef.current === element) setStatus('error'); });
  }, []);
  const skip = useCallback((delta: number) => {
    const element = audioRef.current;
    if (!element) return;
    element.currentTime = Math.max(0, Math.min(element.duration || Infinity, element.currentTime + delta));
    setCurrentTime(element.currentTime);
  }, []);
  const setRate = useCallback((next: number) => {
    rateRef.current = next;
    if (audioRef.current) audioRef.current.playbackRate = next;
    setRateState(next);
  }, []);

  return { status, path, playing, currentTime, duration, rate, play, pause, toggle, seek, skip, setRate };
}
