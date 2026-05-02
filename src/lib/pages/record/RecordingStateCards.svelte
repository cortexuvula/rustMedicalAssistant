<script lang="ts">
  import { audio } from '../../stores/audio';
  import { settings } from '../../stores/settings';

  interface Props {
    importedRecordingId: string | null;
    importedFilename: string | null;
    importing: boolean;
    importError: string | null;
    onProcessRecording: () => void;
    onUploadAudio: () => Promise<void> | void;
  }
  let {
    importedRecordingId,
    importedFilename,
    importing,
    importError,
    onProcessRecording,
    onUploadAudio,
  }: Props = $props();
</script>

{#if importedRecordingId && $audio.state === 'idle'}
  <!-- Imported file, pipeline not yet started -->
  <div class="state-message">
    <div class="state-icon">✓</div>
    <h2>Audio File Imported</h2>
    <p><strong>{importedFilename}</strong> has been added to your recordings.</p>

    {#if !$settings.auto_generate_soap}
      <div class="post-actions">
        <button class="btn-primary" onclick={onProcessRecording}>
          Process Recording
        </button>
      </div>
    {/if}

    {#if importError}
      <div class="error-text">{importError}</div>
    {/if}
  </div>

{:else if $audio.state === 'idle'}
  <div class="state-message">
    <div class="state-icon">🎙</div>
    <h2>Ready to Record</h2>
    <p>Press <strong>Record</strong> to start capturing audio, or upload an existing file.</p>

    <div class="post-actions">
      <button
        class="btn-upload"
        onclick={onUploadAudio}
        disabled={importing}
      >
        {#if importing}
          <span class="spinner"></span> Importing...
        {:else}
          Upload Audio File
        {/if}
      </button>
    </div>

    {#if importError}
      <div class="error-text">{importError}</div>
    {/if}
  </div>

{:else if $audio.state === 'recording'}
  <div class="state-message">
    <div class="state-icon recording-pulse">●</div>
    <h2>Recording in Progress</h2>
    <p>Audio is being captured. Press <strong>Pause</strong> or <strong>Stop</strong> when done.</p>
  </div>

{:else if $audio.state === 'paused'}
  <div class="state-message">
    <div class="state-icon">⏸</div>
    <h2>Recording Paused</h2>
    <p>Press <strong>Resume</strong> to continue or <strong>Stop</strong> to finish.</p>
  </div>

{:else if $audio.state === 'stopped'}
  <div class="state-message">
    <div class="state-icon">✓</div>
    <h2>Recording Complete</h2>
    <p>Your recording has been saved.</p>

    {#if !$settings.auto_generate_soap && $audio.lastRecordingId}
      <div class="post-actions">
        <button class="btn-primary" onclick={onProcessRecording}>
          Process Recording
        </button>
      </div>
    {/if}

    <p class="hint">Or start a <strong>New Recording</strong>.</p>
  </div>
{/if}

<style>
  .state-message {
    text-align: center;
    max-width: 400px;
  }

  .state-icon {
    font-size: 48px;
    margin-bottom: 16px;
    line-height: 1;
  }

  .recording-pulse {
    color: var(--danger);
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .btn-upload {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: var(--bg-tertiary, #374151);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-upload:hover:not(:disabled) {
    background-color: var(--bg-hover);
  }

  .btn-upload:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
