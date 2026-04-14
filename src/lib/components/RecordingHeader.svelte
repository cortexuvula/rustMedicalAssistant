<script lang="ts">
  import { audio } from '../stores/audio';
  import Waveform from './Waveform.svelte';

  function formatTime(seconds: number): string {
    const m = Math.floor(seconds / 60).toString().padStart(2, '0');
    const s = (seconds % 60).toString().padStart(2, '0');
    return `${m}:${s}`;
  }
</script>

<div class="recording-header">
  {#if $audio.error}
    <div class="error-banner">
      <span class="error-text">{$audio.error}</span>
      <button class="error-dismiss" onclick={() => audio.reset()}>Dismiss</button>
    </div>
  {/if}

  <div class="controls-row">
    <div class="timer">
      {formatTime($audio.elapsed)}
    </div>

    <div class="controls">
      {#if $audio.state === 'idle'}
        <button class="btn btn-record" onclick={() => audio.startRecording()}>
          <span class="btn-icon">●</span> Record
        </button>
      {:else if $audio.state === 'recording'}
        <button class="btn btn-pause" onclick={() => audio.pause()}>
          <span class="btn-icon">⏸</span> Pause
        </button>
        <button class="btn btn-stop" onclick={() => audio.stop()}>
          <span class="btn-icon">■</span> Stop
        </button>
        <button class="btn btn-cancel" onclick={() => audio.cancel()}>
          <span class="btn-icon">✕</span> Cancel
        </button>
      {:else if $audio.state === 'paused'}
        <button class="btn btn-resume" onclick={() => audio.resume()}>
          <span class="btn-icon">▶</span> Resume
        </button>
        <button class="btn btn-stop" onclick={() => audio.stop()}>
          <span class="btn-icon">■</span> Stop
        </button>
        <button class="btn btn-cancel" onclick={() => audio.cancel()}>
          <span class="btn-icon">✕</span> Cancel
        </button>
      {:else if $audio.state === 'stopped'}
        <button class="btn btn-new" onclick={() => audio.reset()}>
          <span class="btn-icon">+</span> New Recording
        </button>
      {/if}
    </div>
  </div>

  <div class="waveform-container">
    <Waveform />
  </div>
</div>

<style>
  .recording-header {
    background-color: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    padding: 16px;
    flex-shrink: 0;
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    margin-bottom: 12px;
    background-color: var(--danger-bg, rgba(239, 68, 68, 0.1));
    border: 1px solid var(--danger, #ef4444);
    border-radius: var(--radius-md);
    font-size: 13px;
    color: var(--danger, #ef4444);
  }

  .error-text {
    flex: 1;
  }

  .error-dismiss {
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    color: var(--danger, #ef4444);
    border: 1px solid var(--danger, #ef4444);
    background: transparent;
    cursor: pointer;
  }

  .error-dismiss:hover {
    background-color: var(--danger, #ef4444);
    color: white;
  }

  .controls-row {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
  }

  .timer {
    font-family: var(--font-mono);
    font-size: 28px;
    font-weight: 600;
    color: var(--text-primary);
    min-width: 90px;
  }

  .controls {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 500;
    transition: opacity 0.15s ease, filter 0.15s ease;
  }

  .btn:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-icon {
    font-size: 12px;
  }

  .btn-record {
    background-color: var(--danger);
    color: white;
  }

  .btn-pause {
    background-color: var(--warning);
    color: white;
  }

  .btn-stop {
    background-color: var(--bg-tertiary);
    color: var(--text-primary);
    border: 1px solid var(--border);
  }

  .btn-resume {
    background-color: var(--success);
    color: white;
  }

  .btn-cancel {
    background-color: transparent;
    color: var(--danger);
    border: 1px solid var(--danger);
  }

  .btn-cancel:hover:not(:disabled) {
    background-color: var(--danger);
    color: white;
    filter: none;
  }

  .btn-new {
    background-color: var(--accent);
    color: white;
  }

  .waveform-container {
    background-color: var(--bg-tertiary);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
</style>
