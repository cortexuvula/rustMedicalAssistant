<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';
  import { formatDuration } from '../utils/format';
</script>

<div class="statusbar">
  <div class="status-left">
    {#if $audio.state === 'recording'}
      <span class="status-indicator recording">● REC</span>
      <span class="status-timer">{formatDuration($audio.elapsed)}</span>
    {:else if $audio.state === 'paused'}
      <span class="status-indicator paused">⏸ PAUSED</span>
      <span class="status-timer">{formatDuration($audio.elapsed)}</span>
    {:else if $audio.state === 'stopped'}
      <span class="status-indicator stopped">■ Stopped</span>
    {:else}
      <span class="status-indicator ready">Ready</span>
    {/if}
  </div>

  <div class="status-right">
    <span class="status-provider">AI: {$settings.ai_provider}/{$settings.ai_model}</span>
    <span class="status-sep">·</span>
    <span class="status-provider">STT: {$settings.whisper_model}</span>
  </div>
</div>

<style>
  .statusbar {
    height: var(--statusbar-height);
    background-color: var(--bg-secondary);
    border-top: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    font-size: 11px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .status-left {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .status-right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .status-indicator {
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  .status-indicator.recording {
    color: var(--danger);
  }

  .status-indicator.paused {
    color: var(--warning);
  }

  .status-indicator.stopped {
    color: var(--text-secondary);
  }

  .status-indicator.ready {
    color: var(--text-muted);
  }

  .status-timer {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
  }

  .status-provider {
    font-size: 11px;
  }

  .status-sep {
    color: var(--border);
  }
</style>
