import { writable } from 'svelte/store';
import type { AppConfig } from '../types';
import { getSettings, saveSettings } from '../api/settings';

const defaults: AppConfig = {
  theme: 'dark',
  language: 'en',
  ai_provider: 'openai',
  ai_model: 'gpt-4o',
  stt_provider: 'groq',
  tts_provider: 'elevenlabs',
  tts_voice: 'Rachel',
  temperature: 0.4,
  sample_rate: 44100,
  autosave_enabled: true,
  autosave_interval_secs: 60,
  search_top_k: 5,
  mmr_lambda: 0.5,
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppConfig>(defaults);

  return {
    subscribe,

    async load(): Promise<void> {
      try {
        const config = await getSettings();
        set(config);
      } catch (err) {
        console.error('Failed to load settings:', err);
      }
    },

    async save(config: AppConfig): Promise<void> {
      try {
        await saveSettings(config);
        set(config);
      } catch (err) {
        console.error('Failed to save settings:', err);
        throw err;
      }
    },

    async updateField(key: string, value: any): Promise<void> {
      let current: AppConfig = defaults;
      const unsubscribe = subscribe((v) => {
        current = v;
      });
      unsubscribe();

      const updated: AppConfig = { ...current, [key]: value };
      await this.save(updated);
    },
  };
}

export const settings = createSettingsStore();
