import { writable } from 'svelte/store';

export type RecordingState = 'idle' | 'recording' | 'paused' | 'stopped';

export interface AudioStoreState {
  state: RecordingState;
  elapsed: number;
  waveformData: number[];
  deviceName: string | null;
}

const initialState: AudioStoreState = {
  state: 'idle',
  elapsed: 0,
  waveformData: [],
  deviceName: null,
};

function createAudioStore() {
  const { subscribe, set, update } = writable<AudioStoreState>(initialState);

  let timer: ReturnType<typeof setInterval> | null = null;

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

    startRecording(device: string | null = null) {
      update((s) => ({
        ...s,
        state: 'recording',
        elapsed: 0,
        waveformData: [],
        deviceName: device,
      }));
      startTimer();
    },

    pause() {
      clearTimer();
      update((s) => ({ ...s, state: 'paused' }));
    },

    resume() {
      update((s) => ({ ...s, state: 'recording' }));
      startTimer();
    },

    stop() {
      clearTimer();
      update((s) => ({ ...s, state: 'stopped' }));
    },

    reset() {
      clearTimer();
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
