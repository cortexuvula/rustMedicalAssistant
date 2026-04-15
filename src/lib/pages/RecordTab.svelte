<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';
  import { pipeline, type PipelineStage } from '../stores/pipeline';
  import { recordings } from '../stores/recordings';
  import { importAudioFile, getRecording } from '../api/recordings';
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  // Context panel state
  let contextText = $state('');
  let contextCollapsed = $state(true);

  // Import flow state
  let importedRecordingId = $state<string | null>(null);
  let importedFilename = $state<string | null>(null);
  let importing = $state(false);
  let importError = $state<string | null>(null);

  // Track the recording ID the current pipeline status refers to
  let pipelineRecordingId = $state<string | null>(null);

  function stageLabel(stage: PipelineStage): string {
    switch (stage) {
      case 'transcribing': return 'Transcribing audio...';
      case 'generating_soap': return 'Generating SOAP note...';
      case 'completed': return 'SOAP note ready';
      case 'failed': return 'Pipeline failed';
      default: return '';
    }
  }

  function handleStartRecording() {
    // Clear context for a fresh recording
    contextText = '';
    importedRecordingId = null;
    importedFilename = null;
    importError = null;
    pipeline.clearCurrent();
    audio.startRecording();
  }

  function handleStopRecording() {
    audio.stop().then(() => {
      const recordingId = $audio.lastRecordingId;
      if (!recordingId) return;

      pipelineRecordingId = recordingId;

      if ($settings.auto_generate_soap) {
        pipeline.launch(recordingId, contextText || undefined);
      }
    });
  }

  function handleProcessRecording() {
    const recordingId = $audio.lastRecordingId ?? importedRecordingId;
    if (!recordingId) return;
    pipelineRecordingId = recordingId;
    pipeline.launch(recordingId, contextText || undefined);
  }

  function handleRetry() {
    if (!pipelineRecordingId) return;
    pipeline.retry(pipelineRecordingId, contextText || undefined);
  }

  async function handleUploadAudio() {
    importError = null;
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

      if ($settings.auto_generate_soap) {
        pipelineRecordingId = recordingId;
        pipeline.launch(recordingId, contextText || undefined);
      }
    } catch (e: any) {
      importError = e?.toString() || 'Import failed';
    } finally {
      importing = false;
    }
  }

  async function handleCopySoap() {
    // The SOAP text was persisted to the recording by the pipeline.
    // Read it from the pipeline result isn't stored locally, so we
    // load the recording and copy its soap_note.
    const rid = pipelineRecordingId;
    if (!rid) return;
    try {
      const rec = await getRecording(rid);
      if (rec?.soap_note) {
        await navigator.clipboard.writeText(rec.soap_note);
      }
    } catch (_) {
      // Fallback: clipboard API may fail in some contexts
    }
  }
</script>

<div class="record-tab">
  <!-- Context Panel (collapsible, top) -->
  <div class="context-panel" class:collapsed={contextCollapsed}>
    <button class="context-toggle" onclick={() => (contextCollapsed = !contextCollapsed)}>
      <span class="toggle-arrow">{contextCollapsed ? '▶' : '▼'}</span>
      Patient Context
      <span class="context-hint">(optional)</span>
    </button>
    {#if !contextCollapsed}
      <textarea
        class="context-textarea"
        placeholder="Paste chart notes, medications, history..."
        bind:value={contextText}
        rows="5"
      ></textarea>
    {/if}
  </div>

  <!-- Recording Controls (middle, unchanged) -->
  <RecordingHeader
    onStart={handleStartRecording}
    onStop={handleStopRecording}
  />

  <!-- Main content area -->
  <div class="record-content">
    {#if $pipeline.current && pipelineRecordingId}
      <!-- Pipeline Status -->
      <div class="pipeline-status">
        <div class="pipeline-stages">
          <div class="stage" class:active={$pipeline.current.stage === 'transcribing'} class:done={['generating_soap', 'completed'].includes($pipeline.current.stage)}>
            {#if $pipeline.current.stage === 'transcribing'}
              <span class="spinner"></span>
            {:else if ['generating_soap', 'completed'].includes($pipeline.current.stage)}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            Transcribe
          </div>
          <span class="stage-arrow">→</span>
          <div class="stage" class:active={$pipeline.current.stage === 'generating_soap'} class:done={$pipeline.current.stage === 'completed'}>
            {#if $pipeline.current.stage === 'generating_soap'}
              <span class="spinner"></span>
            {:else if $pipeline.current.stage === 'completed'}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            SOAP Note
          </div>
          <span class="stage-arrow">→</span>
          <div class="stage" class:done={$pipeline.current.stage === 'completed'}>
            {#if $pipeline.current.stage === 'completed'}
              <span class="stage-check">✓</span>
            {:else}
              <span class="stage-dot">○</span>
            {/if}
            Done
          </div>
        </div>

        <p class="pipeline-label">{stageLabel($pipeline.current.stage)}</p>

        {#if $pipeline.current.stage === 'completed'}
          <div class="post-actions">
            <button class="btn-primary" onclick={handleCopySoap}>Copy SOAP Note</button>
          </div>
        {/if}

        {#if $pipeline.current.stage === 'failed'}
          <div class="error-text">{$pipeline.current.error}</div>
          <div class="post-actions">
            <button class="btn-primary" onclick={handleRetry}>Retry</button>
          </div>
        {/if}
      </div>

    {:else if importedRecordingId && $audio.state === 'idle'}
      <!-- Imported file, pipeline not yet started -->
      <div class="state-message">
        <div class="state-icon">✓</div>
        <h2>Audio File Imported</h2>
        <p><strong>{importedFilename}</strong> has been added to your recordings.</p>

        {#if !$settings.auto_generate_soap}
          <div class="post-actions">
            <button class="btn-primary" onclick={handleProcessRecording}>
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
            <button class="btn-primary" onclick={handleProcessRecording}>
              Process Recording
            </button>
          </div>
        {/if}

        <p class="hint">Or start a <strong>New Recording</strong>.</p>
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

  /* Context Panel */
  .context-panel {
    background-color: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .context-toggle {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    text-align: left;
    cursor: pointer;
    background: none;
    border: none;
  }

  .context-toggle:hover {
    color: var(--text-primary);
  }

  .toggle-arrow {
    font-size: 10px;
    width: 12px;
    text-align: center;
  }

  .context-hint {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 12px;
  }

  .context-textarea {
    display: block;
    width: 100%;
    padding: 8px 16px 12px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-primary);
    background-color: var(--bg-primary);
    border: none;
    border-top: 1px solid var(--border-light, var(--border));
    resize: vertical;
    min-height: 80px;
    max-height: 200px;
  }

  .context-textarea:focus {
    outline: none;
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .context-textarea::placeholder {
    color: var(--text-muted);
  }

  /* Main Content */
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

  .btn-primary {
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

  .btn-primary:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  /* Pipeline Status */
  .pipeline-status {
    text-align: center;
    max-width: 500px;
  }

  .pipeline-stages {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  .stage {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-muted);
    transition: color 0.2s ease;
  }

  .stage.active {
    color: var(--accent);
  }

  .stage.done {
    color: var(--success);
  }

  .stage-arrow {
    color: var(--text-muted);
    font-size: 14px;
  }

  .stage-check {
    color: var(--success);
    font-size: 14px;
  }

  .stage-dot {
    color: var(--text-muted);
    font-size: 12px;
  }

  .pipeline-label {
    font-size: 15px;
    font-weight: 500;
    color: var(--text-primary);
    margin-bottom: 8px;
  }

  .spinner {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: var(--accent);
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
</style>
