import { writable } from 'svelte/store';
import type { AppConfig } from '../types';
import { getSettings, saveSettings } from '../api/settings';

const defaults: AppConfig = {
  theme: 'dark',
  language: 'en-US',
  ai_provider: 'openai',
  ai_model: 'gpt-4o',
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
  vocabulary_enabled: true,
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
      update((current) => {
        const updated: AppConfig = { ...current, [key]: value };
        saveSettings(updated).catch((e) => console.error('Save failed:', e));
        return updated;
      });
    },
  };
}

export const settings = createSettingsStore();
