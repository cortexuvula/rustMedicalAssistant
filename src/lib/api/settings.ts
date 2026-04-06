import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '../types';

export async function getSettings(): Promise<AppConfig> {
  return invoke('get_settings');
}

export async function saveSettings(config: AppConfig): Promise<void> {
  return invoke('save_settings', { config });
}

export async function getApiKey(provider: string): Promise<string | null> {
  return invoke('get_api_key', { provider });
}

export async function setApiKey(provider: string, key: string): Promise<void> {
  return invoke('set_api_key', { provider, key });
}

export async function listApiKeys(): Promise<string[]> {
  return invoke('list_api_keys');
}
