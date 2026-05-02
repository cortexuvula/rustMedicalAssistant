<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';
  import { pipeline, type PipelineStage } from '../stores/pipeline';
  import { recordings } from '../stores/recordings';
  import { importAudioFile, getRecording } from '../api/recordings';
  import { checkRecordingAudioLevels } from '../api/audio';
  import { copyWithStatus } from '../utils/clipboard';
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import RecordingStateCards from './record/RecordingStateCards.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { upsertContextTemplate } from '../api/contextTemplates';
  import { contextTemplates } from '../stores/contextTemplates';
  import { toasts } from '../stores/toasts';
  import { rsvp } from '../stores/rsvp';
  import { formatError } from '../types/errors';
  import { buildPatientContext } from '../utils/patient_context';

  // Context panel state
  let contextText = $state('');
  let medicationsText = $state('');
  let allergiesText = $state('');
  let conditionsText = $state('');
  let contextCollapsed = $state(true);

  const hasActiveContext = $derived(
    contextText.trim().length > 0 ||
      medicationsText.trim().length > 0 ||
      allergiesText.trim().length > 0 ||
      conditionsText.trim().length > 0,
  );

  // Template picker state
  let selectedTemplate = $state('');

  // Save-as-template modal state
  let saveModalOpen = $state(false);
  let saveModalName = $state('');
  let saveModalError = $state('');
  let saveModalOverwriteConfirm = $state(false);

  function applyTemplate(name: string) {
    if (!name) return;
    const t = $contextTemplates.find((x) => x.name === name);
    if (!t) return;
    if (contextText.trim() === '') {
      contextText = t.body;
    } else {
      contextText = contextText.replace(/\s+$/, '') + '\n\n' + t.body;
    }
    // Reset dropdown so the same template can be applied again
    selectedTemplate = '';
    // Ensure the accordion is open so the user sees the inserted text
    contextCollapsed = false;
  }

  function openSaveModal() {
    if (contextText.trim() === '') return;
    saveModalName = '';
    saveModalError = '';
    saveModalOverwriteConfirm = false;
    saveModalOpen = true;
  }

  function closeSaveModal() {
    saveModalOpen = false;
    saveModalError = '';
    saveModalOverwriteConfirm = false;
  }

  async function confirmSaveTemplate() {
    const name = saveModalName.trim();
    if (!name) {
      saveModalError = 'Name is required.';
      return;
    }
    const exists = $contextTemplates.some((t) => t.name === name);
    if (exists && !saveModalOverwriteConfirm) {
      saveModalOverwriteConfirm = true;
      saveModalError = `A template named "${name}" exists. Click Save again to overwrite.`;
      return;
    }
    try {
      await upsertContextTemplate(name, contextText);
      await contextTemplates.load();
      closeSaveModal();
    } catch (err: any) {
      saveModalError = formatError(err) || 'Failed to save template.';
    }
  }

  onMount(() => {
    contextTemplates.load();
  });

  // Import flow state
  let importedRecordingId = $state<string | null>(null);
  let importedFilename = $state<string | null>(null);
  let importing = $state(false);
  let importError = $state<string | null>(null);

  // Track the recording ID the current pipeline status refers to
  let pipelineRecordingId = $state<string | null>(null);

  // Silent-recording warning dialog state
  let silenceDialogOpen = $state(false);
  let silenceDialogRecordingId = $state<string | null>(null);
  let silenceDialogMessage = $state('');

  function stageLabel(stage: PipelineStage): string {
    switch (stage) {
      case 'transcribing': return 'Transcribing audio...';
      case 'generating_soap': return 'Generating SOAP note...';
      case 'completed': return 'SOAP note ready';
      case 'failed': return 'Pipeline failed';
      default: return '';
    }
  }

  // Live clock for the pipeline-elapsed counter. Ticks once per second while
  // a pipeline is in flight, then stops so we don't burn a timer forever.
  let nowMs = $state(Date.now());
  $effect(() => {
    const cur = $pipeline.current;
    if (!cur || cur.finishedAt !== null) return;
    nowMs = Date.now();
    const id = setInterval(() => { nowMs = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  function formatPipelineElapsed(ms: number): string {
    const secs = Math.max(0, Math.floor(ms / 1000));
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
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

  function describeSilence(rms: number): string {
    const rmsDb = rms > 0 ? 20 * Math.log10(rms) : -Infinity;
    const formatted = isFinite(rmsDb) ? `${rmsDb.toFixed(1)} dBFS` : 'digital silence';
    return (
      `The recording appears to contain no audio (${formatted}). ` +
      'Your microphone or audio routing likely isn’t capturing sound — ' +
      'processing this file will probably produce an unreliable transcript.'
    );
  }

  async function maybeLaunchPipeline(recordingId: string) {
    try {
      const levels = await checkRecordingAudioLevels(recordingId);
      if (levels.is_silent) {
        silenceDialogRecordingId = recordingId;
        silenceDialogMessage = describeSilence(levels.rms);
        silenceDialogOpen = true;
        return;
      }
    } catch (_e) {
      // If the silence check itself fails, don't block the pipeline.
    }
    pipeline.launch(recordingId, contextText || undefined, undefined, buildPatientContext(medicationsText, allergiesText, conditionsText));
  }

  async function warnIfSilent(recordingId: string) {
    try {
      const levels = await checkRecordingAudioLevels(recordingId);
      if (levels.is_silent) {
        silenceDialogRecordingId = recordingId;
        silenceDialogMessage = describeSilence(levels.rms);
        silenceDialogOpen = true;
      }
    } catch (_e) {
      // Silent failure is fine — this is advisory only.
    }
  }

  function confirmSilentProcess() {
    const id = silenceDialogRecordingId;
    silenceDialogOpen = false;
    silenceDialogRecordingId = null;
    if (id) {
      pipelineRecordingId = id;
      pipeline.launch(id, contextText || undefined, undefined, buildPatientContext(medicationsText, allergiesText, conditionsText));
    }
  }

  function dismissSilenceDialog() {
    silenceDialogOpen = false;
    silenceDialogRecordingId = null;
  }

  function handleStopRecording() {
    audio.stop().then(() => {
      const recordingId = $audio.lastRecordingId;
      if (!recordingId) return;

      pipelineRecordingId = recordingId;

      if ($settings.auto_generate_soap) {
        maybeLaunchPipeline(recordingId);
      } else {
        warnIfSilent(recordingId);
      }
    });
  }

  function handleProcessRecording() {
    const recordingId = $audio.lastRecordingId ?? importedRecordingId;
    if (!recordingId) return;
    pipelineRecordingId = recordingId;
    maybeLaunchPipeline(recordingId);
  }

  function handleRetry() {
    if (!pipelineRecordingId) return;
    pipeline.retry(pipelineRecordingId, contextText || undefined, undefined, buildPatientContext(medicationsText, allergiesText, conditionsText));
  }

  function handleCancelPipeline() {
    if (!pipelineRecordingId) return;
    pipeline.cancel(pipelineRecordingId);
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
        maybeLaunchPipeline(recordingId);
      }
    } catch (e: any) {
      importError = formatError(e) || 'Import failed';
    } finally {
      importing = false;
    }
  }

  let copyStatus = $state<'idle' | 'copying' | 'copied'>('idle');

  async function handleCopySoap() {
    if (copyStatus !== 'idle') return;
    const rid = pipelineRecordingId;
    if (!rid) return;
    await copyWithStatus({
      setStatus: (s) => (copyStatus = s),
      getText: async () => {
        const rec = await getRecording(rid);
        return rec?.soap_note ?? undefined;
      },
      onError: (e) => toasts.error(`Failed to copy SOAP note: ${e}`),
    });
  }

  async function handleSpeedRead() {
    const rid = pipelineRecordingId;
    if (!rid) return;
    try {
      const rec = await getRecording(rid);
      if (rec?.soap_note) {
        rsvp.openSoap(rec.soap_note);
      } else {
        toasts.error('No SOAP note to read yet.');
      }
    } catch (e) {
      console.error('Failed to open speed reader:', e);
      toasts.error(`Failed to open speed reader: ${e}`);
    }
  }
</script>

<div class="record-tab">
  <!-- Context Panel (collapsible, top) -->
  <div class="context-panel" class:collapsed={contextCollapsed}>
    <button class="context-toggle" onclick={() => (contextCollapsed = !contextCollapsed)}>
      <span class="toggle-arrow">{contextCollapsed ? '▶' : '▼'}</span>
      Patient Context
      {#if hasActiveContext}
        <span class="context-badge">Active</span>
      {:else}
        <span class="context-hint">(optional)</span>
      {/if}
    </button>
    {#if !contextCollapsed}
      <label class="field-label" for="rt-medications">Medications (one per line)</label>
      <textarea
        id="rt-medications"
        class="context-textarea structured"
        placeholder="Lisinopril 10mg PO daily"
        bind:value={medicationsText}
        rows="3"
      ></textarea>

      <label class="field-label" for="rt-allergies">Allergies (one per line)</label>
      <textarea
        id="rt-allergies"
        class="context-textarea structured"
        placeholder="Penicillin (rash)"
        bind:value={allergiesText}
        rows="2"
      ></textarea>

      <label class="field-label" for="rt-conditions">Known conditions (one per line)</label>
      <textarea
        id="rt-conditions"
        class="context-textarea structured"
        placeholder="Type 2 diabetes"
        bind:value={conditionsText}
        rows="3"
      ></textarea>

      <label class="field-label" for="rt-notes">Notes</label>
      <div class="template-toolbar">
        <select
          class="template-picker"
          bind:value={selectedTemplate}
          onchange={() => applyTemplate(selectedTemplate)}
          disabled={$contextTemplates.length === 0}
        >
          <option value="">
            {$contextTemplates.length === 0 ? 'No templates saved' : 'Apply template…'}
          </option>
          {#each $contextTemplates as t (t.name)}
            <option value={t.name}>{t.name}</option>
          {/each}
        </select>
        <button
          class="btn-save-template"
          onclick={openSaveModal}
          disabled={contextText.trim() === ''}
          title={contextText.trim() === '' ? 'Type something first' : 'Save current text as a new template'}
        >
          Save as template
        </button>
      </div>
      <textarea
        id="rt-notes"
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

        <p class="pipeline-elapsed">
          {#if $pipeline.current.finishedAt !== null}
            {#if $pipeline.current.stage === 'completed'}
              Processing took {formatPipelineElapsed($pipeline.current.finishedAt - $pipeline.current.startedAt)}
            {:else}
              Stopped after {formatPipelineElapsed($pipeline.current.finishedAt - $pipeline.current.startedAt)}
            {/if}
          {:else}
            Elapsed {formatPipelineElapsed(nowMs - $pipeline.current.startedAt)}
          {/if}
        </p>

        {#if ['transcribing', 'generating_soap'].includes($pipeline.current.stage)}
          <div class="post-actions">
            <button class="btn-secondary" onclick={handleCancelPipeline}>Cancel</button>
          </div>
        {/if}

        {#if $pipeline.current.stage === 'completed'}
          <div class="post-actions">
            <button class="btn-secondary" onclick={handleSpeedRead}>Speed Read</button>
            <button
              class="btn-primary"
              onclick={handleCopySoap}
              disabled={copyStatus !== 'idle'}
            >
              {copyStatus === 'copying' ? 'Copying…' : copyStatus === 'copied' ? 'Copied!' : 'Copy SOAP Note'}
            </button>
          </div>
        {/if}

        {#if $pipeline.current.stage === 'failed'}
          <div class="error-text">{$pipeline.current.error}</div>
          <div class="post-actions">
            <button class="btn-primary" onclick={handleRetry}>Retry</button>
          </div>
        {/if}
      </div>

    {:else}
      <RecordingStateCards
        {importedRecordingId}
        {importedFilename}
        {importing}
        {importError}
        onProcessRecording={handleProcessRecording}
        onUploadAudio={handleUploadAudio}
      />
    {/if}
  </div>
</div>

{#if saveModalOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="save-modal-overlay" onclick={closeSaveModal}>
    <div class="save-modal" onclick={(e) => e.stopPropagation()}>
      <div class="save-modal-header">
        <h3>Save as Template</h3>
        <button class="btn-close" aria-label="Close" onclick={closeSaveModal}>&times;</button>
      </div>
      {#if saveModalError}
        <div class="save-modal-error">{saveModalError}</div>
      {/if}
      <label class="save-modal-field">
        <span>Name</span>
        <input type="text" bind:value={saveModalName} placeholder="e.g. Follow-up visit" autofocus />
      </label>
      <div class="save-modal-field">
        <span>Preview</span>
        <pre class="save-modal-preview">{contextText}</pre>
      </div>
      <div class="save-modal-actions">
        <button class="btn-save" onclick={confirmSaveTemplate}>
          {saveModalOverwriteConfirm ? 'Overwrite' : 'Save'}
        </button>
        <button class="btn-cancel" onclick={closeSaveModal}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<ConfirmDialog
  open={silenceDialogOpen}
  title="Silent recording detected"
  message={silenceDialogMessage}
  confirmLabel="Process anyway"
  cancelLabel="Cancel"
  danger
  onConfirm={confirmSilentProcess}
  onCancel={dismissSilenceDialog}
/>

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

  .field-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-top: 8px;
    margin-bottom: 4px;
    display: block;
  }

  .context-textarea.structured {
    min-height: 56px;
  }

  .context-badge {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--accent);
    background-color: color-mix(in srgb, var(--accent) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    margin-left: 6px;
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

  .btn-secondary {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 24px;
    background-color: transparent;
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    transition: background-color 0.15s ease, border-color 0.15s ease;
    cursor: pointer;
  }

  .btn-secondary:hover:not(:disabled) {
    background-color: var(--bg-hover);
    border-color: var(--accent);
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
    margin-bottom: 4px;
  }

  .pipeline-elapsed {
    font-size: 13px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
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

  .template-toolbar {
    display: flex;
    gap: 8px;
    padding: 8px 16px 0;
    align-items: center;
  }
  .template-picker {
    flex: 1 1 auto;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
    font-size: 0.88rem;
  }
  .template-picker:disabled { opacity: 0.6; cursor: not-allowed; }
  .btn-save-template {
    flex: 0 0 auto;
    padding: 6px 14px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: transparent;
    color: var(--text-primary, #e0e0e0);
    cursor: pointer;
    font-size: 0.88rem;
    white-space: nowrap;
  }
  .btn-save-template:hover:not(:disabled) { background: rgba(255, 255, 255, 0.05); }
  .btn-save-template:disabled { opacity: 0.4; cursor: not-allowed; }

  .save-modal-overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 1000;
  }
  .save-modal {
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border-radius: 8px;
    width: 90vw; max-width: 520px; max-height: 85vh;
    display: flex; flex-direction: column;
    padding: 20px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
  }
  .save-modal-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .save-modal-header h3 { margin: 0; font-size: 1.05rem; }
  .save-modal .btn-close {
    background: none; border: none; color: var(--text-secondary, #aaa);
    font-size: 1.4rem; line-height: 1; padding: 4px 8px; cursor: pointer; border-radius: 4px;
  }
  .save-modal .btn-close:hover { background: rgba(255, 255, 255, 0.08); }
  .save-modal-error {
    color: #ff6b6b; margin-bottom: 10px; font-size: 0.85rem;
    padding: 6px 10px; background: rgba(255, 107, 107, 0.1); border-radius: 4px;
  }
  .save-modal-field { display: flex; flex-direction: column; gap: 4px; font-size: 0.85rem; color: var(--text-secondary, #aaa); margin-bottom: 10px; }
  .save-modal-field span { font-weight: 500; }
  .save-modal-field input {
    padding: 7px 10px; border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0); font-size: 0.9rem;
  }
  .save-modal-preview {
    background: var(--bg-primary, #111); padding: 10px; border-radius: 4px;
    border: 1px solid var(--border-color, #333); max-height: 180px; overflow-y: auto;
    white-space: pre-wrap; font-size: 0.85rem; margin: 0; font-family: inherit;
  }
  .save-modal-actions { display: flex; gap: 8px; margin-top: 8px; }
  .save-modal .btn-save {
    padding: 7px 18px; border-radius: 4px; border: none;
    background: var(--accent-color, #4a9eff); color: white; cursor: pointer; font-size: 0.9rem;
  }
  .save-modal .btn-save:hover { filter: brightness(1.1); }
  .save-modal .btn-cancel {
    padding: 7px 18px; border-radius: 4px;
    border: 1px solid var(--border-color, #444); background: transparent;
    color: var(--text-primary, #e0e0e0); cursor: pointer; font-size: 0.9rem;
  }
</style>
