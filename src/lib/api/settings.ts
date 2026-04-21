import { invoke } from '@tauri-apps/api/core';
import type { AppConfig } from '../types';

export async function getSettings(): Promise<AppConfig> {
  return invoke('get_settings');
}

export async function saveSettings(config: AppConfig): Promise<void> {
  return invoke('save_settings', { config });
}

export async function testLmStudioConnection(host: string, port: number): Promise<string> {
  return invoke('test_lmstudio_connection', { host, port });
}
