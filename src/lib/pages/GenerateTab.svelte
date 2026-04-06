<script lang="ts">
  import { selectedRecording, recordings, selectRecording } from '../stores/recordings';
  import { generateSoap, generateReferral, generateLetter } from '../api/generation';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';

  let generating = $state<string | null>(null);
  let error = $state<string | null>(null);
  let progressStatus = $state<string | null>(null);

  let progressUnlisten: UnlistenFn | null = null;

  onMount(async () => {
    progressUnlisten = await listen<{ type: string; status: string }>(
      'generation-progress',
      (event) => {
        progressStatus = `${event.payload.type}: ${event.payload.status}`;
      }
    );
  });

  onDestroy(() => {
    progressUnlisten?.();
  });

  async function handleGenerate(type: 'soap' | 'referral' | 'letter') {
    if (!$selectedRecording) return;
    generating = type;
    error = null;
    progressStatus = null;
    try {
      if (type === 'soap') {
        await generateSoap($selectedRecording.id);
      } else if (type === 'referral') {
        await generateReferral($selectedRecording.id);
      } else {
        await generateLetter($selectedRecording.id);
      }
      // Refresh the selected recording to get updated data
      await selectRecording($selectedRecording.id);
      // Also refresh the recordings list
      await recordings.load();
    } catch (e: any) {
      error = e?.toString() || `Failed to generate ${type}`;
    } finally {
      generating = null;
      progressStatus = null;
    }
  }
</script>

<div class="generate-tab">
  {#if !$selectedRecording}
    <div class="empty-state">
      <div class="empty-icon">⚡</div>
      <h2>Generate Documentation</h2>
      <p>Select a recording from the <strong>Recordings</strong> tab first.</p>
    </div>

  {:else}
    <div class="generate-content">
      <div class="generate-header">
        <h2>Generate Documentation</h2>
        {#if $selectedRecording.patient_name}
          <p class="patient">for {$selectedRecording.patient_name}</p>
        {/if}
      </div>

      {#if error}
        <div class="error-banner">
          <span>{error}</span>
          <button class="error-dismiss" onclick={() => (error = null)}>Dismiss</button>
        </div>
      {/if}

      {#if progressStatus}
        <div class="progress-banner">{progressStatus}</div>
      {/if}

      <div class="generate-buttons">
        <div class="generate-item">
          <div class="item-info">
            <div class="item-title">SOAP Note</div>
            <div class="item-desc">Structured clinical note (Subjective, Objective, Assessment, Plan)</div>
          </div>
          <div class="item-action">
            {#if $selectedRecording.soap_note}
              <span class="done-badge">Done ✓</span>
            {:else if generating === 'soap'}
              <button class="btn-generate" disabled>
                <span class="spinner"></span> Generating...
              </button>
            {:else}
              <button
                class="btn-generate"
                onclick={() => handleGenerate('soap')}
                disabled={generating !== null}
              >
                Generate
              </button>
            {/if}
          </div>
        </div>

        <div class="generate-item">
          <div class="item-info">
            <div class="item-title">Referral Letter</div>
            <div class="item-desc">Specialist referral letter based on the consultation</div>
          </div>
          <div class="item-action">
            {#if $selectedRecording.referral}
              <span class="done-badge">Done ✓</span>
            {:else if generating === 'referral'}
              <button class="btn-generate" disabled>
                <span class="spinner"></span> Generating...
              </button>
            {:else}
              <button
                class="btn-generate"
                onclick={() => handleGenerate('referral')}
                disabled={generating !== null}
              >
                Generate
              </button>
            {/if}
          </div>
        </div>

        <div class="generate-item">
          <div class="item-info">
            <div class="item-title">Patient Letter</div>
            <div class="item-desc">Patient-friendly summary of the consultation</div>
          </div>
          <div class="item-action">
            {#if $selectedRecording.letter}
              <span class="done-badge">Done ✓</span>
            {:else if generating === 'letter'}
              <button class="btn-generate" disabled>
                <span class="spinner"></span> Generating...
              </button>
            {:else}
              <button
                class="btn-generate"
                onclick={() => handleGenerate('letter')}
                disabled={generating !== null}
              >
                Generate
              </button>
            {/if}
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .generate-tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
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
    font-size: 48px;
    margin-bottom: 12px;
  }

  h2 {
    font-size: 20px;
    font-weight: 600;
    color: var(--text-primary);
  }

  p {
    font-size: 14px;
    color: var(--text-muted);
  }

  strong {
    color: var(--text-secondary);
  }

  .generate-content {
    flex: 1;
    overflow-y: auto;
    padding: 24px;
  }

  .generate-header {
    margin-bottom: 24px;
  }

  .generate-header h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 4px;
  }

  .patient {
    font-size: 13px;
    color: var(--text-muted);
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    margin-bottom: 16px;
    background-color: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--danger, #ef4444);
    border-radius: var(--radius-md);
    font-size: 13px;
    color: var(--danger, #ef4444);
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

  .progress-banner {
    padding: 8px 12px;
    margin-bottom: 16px;
    background-color: rgba(59, 130, 246, 0.1);
    border: 1px solid var(--accent, #3b82f6);
    border-radius: var(--radius-md);
    font-size: 13px;
    color: var(--accent, #3b82f6);
  }

  .generate-buttons {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .generate-item {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
    background-color: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .item-info {
    flex: 1;
  }

  .item-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 2px;
  }

  .item-desc {
    font-size: 12px;
    color: var(--text-muted);
  }

  .item-action {
    flex-shrink: 0;
  }

  .btn-generate {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background-color: var(--accent);
    color: white;
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-generate:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-generate:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .done-badge {
    display: inline-flex;
    align-items: center;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    font-weight: 500;
    background-color: var(--accent-light);
    color: var(--success);
    border: 1px solid var(--success);
  }
</style>
