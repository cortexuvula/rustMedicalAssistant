import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import * as audioApi from '../api/audio';

export type RecordingState = 'idle' | 'recording' | 'paused' | 'stopped';

export interface AudioStoreState {
  state: RecordingState;
  elapsed: number;
  waveformData: number[];
  deviceName: string | null;
  lastRecordingId: string | null;
  error: string | null;
}

const initialState: AudioStoreState = {
  state: 'idle',
  elapsed: 0,
  waveformData: [],
  deviceName: null,
  lastRecordingId: null,
  error: null,
};

function createAudioStore() {
  const { subscribe, set, update } = writable<AudioStoreState>(initialState);

  let timer: ReturnType<typeof setInterval> | null = null;
  let waveformUnlisten: UnlistenFn | null = null;
  let busy = false;

  function clearTimer() {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  }

  function startTimer() {
    clearTimer();
    timer = setInterval(() => {
      update((s) => ({ ...s, elapsed: s.elapsed + 1 }));
    }, 1000);
  }

  return {
    subscribe,

    async startRecording(device: string | null = null) {
      if (busy) return;
      busy = true;
      try {
        // Clean up any stale listener before attaching a new one
        if (waveformUnlisten) { waveformUnlisten(); waveformUnlisten = null; }
        // Listen for waveform events BEFORE starting recording
        waveformUnlisten = await listen<number[]>('waveform-data', (event) => {
          update((s) => ({
            ...s,
            waveformData: [...s.waveformData, ...event.payload].slice(-256),
          }));
        });

        const recordingId = await audioApi.startRecording();
        update((s) => ({
          ...s,
          state: 'recording',
          elapsed: 0,
          waveformData: [],
          deviceName: device,
          lastRecordingId: recordingId,
          error: null,
        }));
        startTimer();
      } catch (e: any) {
        if (waveformUnlisten) {
          waveformUnlisten();
          waveformUnlisten = null;
        }
        update((s) => ({
          ...s,
          error: e?.toString() || 'Failed to start recording',
        }));
      } finally {
        busy = false;
      }
    },

    async pause() {
      try {
        await audioApi.pauseRecording();
        clearTimer();
        update((s) => ({ ...s, state: 'paused' }));
      } catch (e: any) {
        update((s) => ({
          ...s,
          error: e?.toString() || 'Failed to pause',
        }));
      }
    },

    async resume() {
      try {
        await audioApi.resumeRecording();
        update((s) => ({ ...s, state: 'recording' }));
        startTimer();
      } catch (e: any) {
        update((s) => ({
          ...s,
          error: e?.toString() || 'Failed to resume',
        }));
      }
    },

    async stop() {
      if (busy) return;
      busy = true;
      clearTimer();
      try {
        const recordingId = await audioApi.stopRecording();
        if (waveformUnlisten) {
          waveformUnlisten();
          waveformUnlisten = null;
        }
        update((s) => ({
          ...s,
          state: 'stopped',
          lastRecordingId: recordingId,
        }));
      } catch (e: any) {
        if (waveformUnlisten) {
          waveformUnlisten();
          waveformUnlisten = null;
        }
        // Don't change state to 'stopped' on error — backend may still be recording
        update((s) => ({
          ...s,
          error: e?.toString() || 'Failed to stop recording',
        }));
        startTimer(); // Restore timer if we were recording
      } finally {
        busy = false;
      }
    },

    async cancel() {
      if (busy) return;
      busy = true;
      clearTimer();
      try {
        await audioApi.cancelRecording();
      } catch (_e: any) {
        // Best-effort — even if backend fails, reset the frontend state
      }
      if (waveformUnlisten) {
        waveformUnlisten();
        waveformUnlisten = null;
      }
      set(initialState);
      busy = false;
    },

    reset() {
      clearTimer();
      if (waveformUnlisten) {
        waveformUnlisten();
        waveformUnlisten = null;
      }
      set(initialState);
    },

    pushWaveform(data: number[]) {
      update((s) => ({
        ...s,
        waveformData: [...s.waveformData, ...data].slice(-256),
      }));
    },
  };
}

export const audio = createAudioStore();
