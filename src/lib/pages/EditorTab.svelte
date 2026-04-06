<script lang="ts">
  import type { Recording } from '../types';
  import { selectedRecording } from '../stores/recordings';
  import TextEditor from '../components/TextEditor.svelte';

  let { tabId }: { tabId: 'transcript' | 'soap' | 'referral' | 'letter' } = $props();

  type TabConfig = { field: keyof Recording; label: string };

  const tabConfigs: Record<string, TabConfig> = {
    transcript: { field: 'transcript', label: 'Transcript' },
    soap:       { field: 'soap_note', label: 'SOAP Note' },
    referral:   { field: 'referral',  label: 'Referral Letter' },
    letter:     { field: 'letter',    label: 'Patient Letter' },
  };

  const config = $derived(tabConfigs[tabId]);
  const content = $derived(
    $selectedRecording
      ? ($selectedRecording[config.field] as string | null) ?? ''
      : null
  );
</script>

<div class="editor-tab">
  <div class="editor-header">
    <div class="editor-header-left">
      <h2 class="doc-type">{config.label}</h2>
      {#if $selectedRecording?.patient_name}
        <span class="patient-name">— {$selectedRecording.patient_name}</span>
      {/if}
    </div>
  </div>

  {#if content === null}
    <div class="empty-state">
      <div class="empty-icon">📄</div>
      <h3>No recording selected</h3>
      <p>Select a recording from the <strong>Recordings</strong> tab to view its {config.label.toLowerCase()}.</p>
    </div>
  {:else if content === ''}
    <div class="empty-state">
      <div class="empty-icon">✏</div>
      <h3>No {config.label} yet</h3>
      <p>Go to the <strong>Generate</strong> tab to create this document.</p>
    </div>
  {:else}
    <TextEditor value={content} placeholder="No content…" />
  {/if}
</div>

<style>
  .editor-tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background-color: var(--bg-secondary);
    flex-shrink: 0;
  }

  .editor-header-left {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .doc-type {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .patient-name {
    font-size: 13px;
    color: var(--text-muted);
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 40px;
    gap: 8px;
    color: var(--text-muted);
  }

  .empty-icon {
    font-size: 40px;
    margin-bottom: 8px;
  }

  h3 {
    font-size: 16px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  p {
    font-size: 13px;
    line-height: 1.6;
  }

  strong {
    color: var(--text-secondary);
  }
</style>
