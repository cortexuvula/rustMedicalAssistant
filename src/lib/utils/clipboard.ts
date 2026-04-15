import { writeText } from '@tauri-apps/plugin-clipboard-manager';

/** Copy text to clipboard using Tauri's clipboard plugin. */
export async function copyToClipboard(text: string): Promise<void> {
  await writeText(text);
}
