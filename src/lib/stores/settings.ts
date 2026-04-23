import { writable } from 'svelte/store';
import type { AppConfig } from '../types';
import { getSettings, saveSettings } from '../api/settings';

const defaults: AppConfig = {
  theme: 'dark',
  language: 'en-US',
  ai_provider: 'lmstudio',
  ai_model: '',
  whisper_model: 'large-v3-turbo',
  tts_provider: 'elevenlabs',
  tts_voice: 'default',
  temperature: 0.2,
  sample_rate: 44100,
  autosave_enabled: true,
  autosave_interval_secs: 60,
  auto_generate_soap: false,
  search_top_k: 5,
  mmr_lambda: 0.7,
  storage_path: null,
  lmstudio_host: 'localhost',
  lmstudio_port: 1234,
  stt_mode: 'local',
  stt_remote_host: '',
  stt_remote_port: 8080,
  stt_remote_model: 'whisper-1',
  ollama_host: 'localhost',
  ollama_port: 11434,
  vocabulary_enabled: true,
  custom_context_templates: [],
  custom_soap_prompt: null,
  custom_referral_prompt: null,
  custom_letter_prompt: null,
  custom_synopsis_prompt: null,
  rsvp_wpm: 300,
  rsvp_font_size: 48,
  rsvp_chunk_size: 1,
  rsvp_dark_theme: true,
  rsvp_show_context: false,
  rsvp_audio_cue: false,
  rsvp_auto_start: true,
  rsvp_remember_sections: false,
  rsvp_remembered_sections: [],
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppConfig>(defaults);

  // Guard flag: prevents saving until a successful load has completed.
  // This protects Rust-only fields (soap_note_settings, agent_settings, etc.)
  // from being overwritten with incomplete TS defaults.
  let loaded = false;

  return {
    subscribe,

    async load(): Promise<void> {
      try {
        const config = await getSettings();
        set(config);
        loaded = true;
      } catch (err) {
        console.error('Failed to load settings:', err);
      }
    },

    async save(config: AppConfig): Promise<void> {
      if (!loaded) {
        console.warn('Settings not loaded yet, refusing to save');
        return;
      }
      set(config);
      try {
        await saveSettings(config);
      } catch (err) {
        console.error('Failed to save settings:', err);
        throw err;
      }
    },

    async updateField(key: string, value: any): Promise<void> {
      if (!loaded) {
        console.warn('Settings not loaded yet, refusing to save');
        return;
      }
      // Capture the previous state (for rollback) and the optimistic new state
      // inside the synchronous update callback, then await the save outside.
      // Awaiting is what makes `await settings.updateField(...)` followed by
      // `reinitProviders()` read-after-write safe — without it, the save was
      // a fire-and-forget background promise and the DB still held the old
      // value when providers reloaded from it.
      let captured: { prev: AppConfig; next: AppConfig } | null = null;
      update((current) => {
        const next: AppConfig = { ...current, [key]: value };
        captured = { prev: current, next };
        return next;
      });
      if (!captured) return; // unreachable: update runs its callback synchronously
      const { prev, next } = captured as { prev: AppConfig; next: AppConfig };
      try {
        await saveSettings(next);
      } catch (err) {
        console.error('Save failed:', err);
        set(prev);
        throw err;
      }
    },
  };
}

export const settings = createSettingsStore();
