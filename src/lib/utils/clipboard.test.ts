import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';

// Mock the Tauri clipboard plugin BEFORE importing the helper under test.
const writeTextMock = vi.fn(async (_: string) => undefined);
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: (text: string) => writeTextMock(text),
}));

import { copyWithStatus } from './clipboard';

type Status = 'idle' | 'copying' | 'copied';

describe('copyWithStatus', () => {
  beforeEach(() => {
    writeTextMock.mockClear();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('runs the idle → copying → copied → idle cycle on success', async () => {
    const transitions: Status[] = [];
    const result = await copyWithStatus({
      setStatus: (s) => transitions.push(s),
      getText: () => 'hello',
    });
    expect(result).toBe(true);
    expect(transitions).toEqual(['copying', 'copied']);
    expect(writeTextMock).toHaveBeenCalledWith('hello');
    // After timeout, status returns to idle.
    vi.advanceTimersByTime(2000);
    expect(transitions).toEqual(['copying', 'copied', 'idle']);
  });

  it('uses the configured copiedDurationMs', async () => {
    const transitions: Status[] = [];
    await copyWithStatus({
      setStatus: (s) => transitions.push(s),
      getText: () => 'hello',
      copiedDurationMs: 500,
    });
    vi.advanceTimersByTime(499);
    expect(transitions).toEqual(['copying', 'copied']);
    vi.advanceTimersByTime(1);
    expect(transitions).toEqual(['copying', 'copied', 'idle']);
  });

  it('returns false and does not call writeText when getText returns empty', async () => {
    const transitions: Status[] = [];
    const result = await copyWithStatus({
      setStatus: (s) => transitions.push(s),
      getText: () => undefined,
    });
    expect(result).toBe(false);
    expect(transitions).toEqual(['copying', 'idle']);
    expect(writeTextMock).not.toHaveBeenCalled();
  });

  it('awaits async getText producers', async () => {
    const transitions: Status[] = [];
    await copyWithStatus({
      setStatus: (s) => transitions.push(s),
      getText: async () => 'async-hello',
    });
    expect(writeTextMock).toHaveBeenCalledWith('async-hello');
    expect(transitions).toEqual(['copying', 'copied']);
  });

  it('resets to idle and invokes onError when writeText throws', async () => {
    writeTextMock.mockRejectedValueOnce(new Error('clipboard denied'));
    const transitions: Status[] = [];
    let captured: unknown = null;
    const result = await copyWithStatus({
      setStatus: (s) => transitions.push(s),
      getText: () => 'hello',
      onError: (e) => { captured = e; },
    });
    expect(result).toBe(false);
    expect(transitions).toEqual(['copying', 'idle']);
    expect((captured as Error).message).toBe('clipboard denied');
  });
});
