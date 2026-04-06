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
      try {
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
        update((s) => ({
          ...s,
          state: 'stopped',
          error: e?.toString() || 'Failed to stop',
        }));
      }
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
