<script lang="ts">
  import { audio } from '../stores/audio';
  import { transcribeRecording } from '../api/transcription';
  import { importAudioFile } from '../api/recordings';
  import { recordings } from '../stores/recordings';
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  let transcribing = $state(false);
  let transcriptionError = $state<string | null>(null);

  // Import flow state
  let importedRecordingId = $state<string | null>(null);
  let importedFilename = $state<string | null>(null);
  let importing = $state(false);

  async function handleTranscribe() {
    const recordingId = $audio.lastRecordingId;
    if (!recordingId) return;
    transcribing = true;
    transcriptionError = null;
    try {
      await transcribeRecording(recordingId);
      await recordings.load();
    } catch (e: any) {
      transcriptionError = e?.toString() || 'Transcription failed';
    } finally {
      transcribing = false;
    }
  }

  async function handleUploadAudio() {
    transcriptionError = null;
    try {
      const selected = await open({
        multiple: false,
        filters: [
          { name: 'Audio Files', extensions: ['wav', 'mp3', 'ogg', 'flac', 'm4a', 'aac', 'wma', 'webm'] },
        ],
      });
      if (!selected) return;

      importing = true;
      const filePath = typeof selected === 'string' ? selected : selected;
      const recordingId = await importAudioFile(filePath);
      importedRecordingId = recordingId;
      importedFilename = filePath.split('/').pop()?.split('\\').pop() ?? 'audio file';
      await recordings.load();
    } catch (e: any) {
      transcriptionError = e?.toString() || 'Import failed';
    } finally {
      importing = false;
    }
  }

  async function handleTranscribeImported() {
    if (!importedRecordingId) return;
    transcribing = true;
    transcriptionError = null;
    try {
      await transcribeRecording(importedRecordingId);
      await recordings.load();
    } catch (e: any) {
      transcriptionError = e?.toString() || 'Transcription failed';
    } finally {
      transcribing = false;
    }
  }

  function handleNewImport() {
    importedRecordingId = null;
    importedFilename = null;
    transcriptionError = null;
  }
</script>

<div class="record-tab">
  <RecordingHeader />

  <div class="record-content">
    {#if importedRecordingId && $audio.state === 'idle'}
      <div class="state-message">
        <div class="state-icon">✓</div>
        <h2>Audio File Imported</h2>
        <p><strong>{importedFilename}</strong> has been added to your recordings.</p>

        <div class="post-actions">
          <button
            class="btn-transcribe"
            onclick={handleTranscribeImported}
            disabled={transcribing}
          >
            {#if transcribing}
              <span class="spinner"></span> Transcribing...
            {:else}
              Transcribe Recording
            {/if}
          </button>
          <button class="btn-secondary" onclick={handleNewImport}>
            Import Another
          </button>
        </div>

        {#if transcriptionError}
          <div class="error-text">{transcriptionError}</div>
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
            onclick={handleUploadAudio}
            disabled={importing}
          >
            {#if importing}
              <span class="spinner"></span> Importing...
            {:else}
              Upload Audio File
            {/if}
          </button>
        </div>

        {#if transcriptionError}
          <div class="error-text">{transcriptionError}</div>
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

        {#if $audio.lastRecordingId}
          <div class="post-actions">
            <button
              class="btn-transcribe"
              onclick={handleTranscribe}
              disabled={transcribing}
            >
              {#if transcribing}
                <span class="spinner"></span> Transcribing...
              {:else}
                Transcribe Recording
              {/if}
            </button>
          </div>
        {/if}

        {#if transcriptionError}
          <div class="error-text">{transcriptionError}</div>
        {/if}

        <p class="hint">Or go to <strong>Recordings</strong> to manage it, or start a <strong>New Recording</strong>.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .record-tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .record-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 32px;
  }

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

  h2 {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 8px;
  }

  p {
    color: var(--text-muted);
    font-size: 14px;
    line-height: 1.6;
  }

  strong {
    color: var(--text-secondary);
  }

  .post-actions {
    margin-top: 16px;
    margin-bottom: 8px;
    display: flex;
    gap: 10px;
    justify-content: center;
    flex-wrap: wrap;
  }

  .btn-transcribe {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: var(--accent);
    color: white;
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-transcribe:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-transcribe:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-text {
    margin-top: 8px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background-color: rgba(239, 68, 68, 0.1);
    color: var(--danger, #ef4444);
    font-size: 13px;
  }

  .hint {
    margin-top: 12px;
    font-size: 13px;
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

  .btn-secondary {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: transparent;
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-secondary:hover {
    background-color: var(--bg-hover);
  }
</style>
