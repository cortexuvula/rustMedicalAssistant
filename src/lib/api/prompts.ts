import { invoke } from '@tauri-apps/api/core';

export type DocType = 'soap' | 'referral' | 'letter' | 'synopsis';

export async function getDefaultPrompt(docType: DocType): Promise<string> {
  return await invoke<string>('get_default_prompt', { docType });
}
