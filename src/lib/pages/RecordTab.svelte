<script lang="ts">
  import { audio } from '../stores/audio';
  import { settings } from '../stores/settings';
  import { pipeline } from '../stores/pipeline';
  import { recordings } from '../stores/recordings';
  import { importAudioFile, getRecording } from '../api/recordings';
  import { checkRecordingAudioLevels } from '../api/audio';
  import { copyWithStatus } from '../utils/clipboard';
  import RecordingHeader from '../components/RecordingHeader.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import RecordingStateCards from './record/RecordingStateCards.svelte';
  import PipelineStatus from './record/PipelineStatus.svelte';
  import PatientContextPanel from './record/PatientContextPanel.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { contextTemplates } from '../stores/contextTemplates';
  import { toasts } from '../stores/toasts';
  import { rsvp } from '../stores/rsvp';
  import { formatError } from '../types/errors';
  import { buildPatientContext } from '../utils/patient_context';

  // Context panel state — owned by parent because buildPatientContext(...) needs them at pipeline-launch time.
  let contextText = $state('');
  let medicationsText = $state('');
  let allergiesText = $state('');
  let conditionsText = $state('');
  let contextCollapsed = $state(true);

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
  <PatientContextPanel
    bind:contextText
    bind:medicationsText
    bind:allergiesText
    bind:conditionsText
    bind:contextCollapsed
  />

  <!-- Recording Controls (middle, unchanged) -->
  <RecordingHeader
    onStart={handleStartRecording}
    onStop={handleStopRecording}
  />

  <!-- Main content area -->
  <div class="record-content">
    {#if $pipeline.current && pipelineRecordingId}
      <PipelineStatus
        {pipelineRecordingId}
        bind:copyStatus
        onCancel={handleCancelPipeline}
        onRetry={handleRetry}
        onCopySoap={handleCopySoap}
        onSpeedRead={handleSpeedRead}
      />
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

</style>
