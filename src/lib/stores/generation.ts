import { writable } from 'svelte/store';

export type GeneratingType = 'soap' | 'referral' | 'letter' | null;

interface GenerationState {
  /** Which document type is currently being generated, or null if idle. */
  generating: GeneratingType;
  /** Live progress text from the backend event. */
  progressStatus: string | null;
  /** Error message from the last generation attempt. */
  error: string | null;
}

function createGenerationStore() {
  const { subscribe, update, set } = writable<GenerationState>({
    generating: null,
    progressStatus: null,
    error: null,
  });

  return {
    subscribe,
    startGenerating(type: 'soap' | 'referral' | 'letter') {
      update((s) => ({ ...s, generating: type, error: null, progressStatus: null }));
    },
    setProgress(status: string) {
      update((s) => ({ ...s, progressStatus: status }));
    },
    setError(error: string) {
      update((s) => ({ ...s, generating: null, progressStatus: null, error }));
    },
    finish() {
      update((s) => ({ ...s, generating: null, progressStatus: null }));
    },
    clearError() {
      update((s) => ({ ...s, error: null }));
    },
  };
}

export const generation = createGenerationStore();
