import { writeText } from '@tauri-apps/plugin-clipboard-manager';

/** Copy text to clipboard using Tauri's clipboard plugin. */
export async function copyToClipboard(text: string): Promise<void> {
  await writeText(text);
}

export type CopyStatus = 'idle' | 'copying' | 'copied';

export interface CopyWithStatusOptions {
  /** Receives status transitions: 'copying' → 'copied' → 'idle'. Idempotent. */
  setStatus: (s: CopyStatus) => void;
  /** Producer for the text to copy. Returning undefined / empty is a no-op. */
  getText: () => string | undefined | Promise<string | undefined>;
  /** Reset to 'idle' this many ms after entering 'copied'. Default 2000. */
  copiedDurationMs?: number;
  /** Invoked when getText or writeText throws. Always called before status resets. */
  onError?: (err: unknown) => void;
}

/**
 * Run the canonical copy-to-clipboard state machine: copying → copied → idle.
 *
 * Centralizes the timing and error-handling pattern previously duplicated in
 * RecordTab.svelte and GenerateTab.svelte. The caller owns the status state
 * (typically a Svelte 5 `$state` rune) and provides a setter; this helper
 * just walks it through the transitions.
 *
 * Returns `true` if text was successfully written to the clipboard, `false`
 * if the producer returned empty or an error occurred.
 */
export async function copyWithStatus(opts: CopyWithStatusOptions): Promise<boolean> {
  const { setStatus, getText, copiedDurationMs = 2000, onError } = opts;
  setStatus('copying');
  try {
    const text = await getText();
    if (!text) {
      setStatus('idle');
      return false;
    }
    await copyToClipboard(text);
    setStatus('copied');
    setTimeout(() => setStatus('idle'), copiedDurationMs);
    return true;
  } catch (e) {
    onError?.(e);
    console.error('Failed to copy:', e);
    setStatus('idle');
    return false;
  }
}
