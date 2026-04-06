<script lang="ts">
  import { selectedRecording } from '../stores/recordings';

  // Placeholder generation functions — will wire to real API later
  function handleGenerate(type: 'soap' | 'referral' | 'letter') {
    console.log(`Generating ${type} for recording ${$selectedRecording?.id}`);
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

      <div class="generate-buttons">
        <div class="generate-item">
          <div class="item-info">
            <div class="item-title">SOAP Note</div>
            <div class="item-desc">Structured clinical note (Subjective, Objective, Assessment, Plan)</div>
          </div>
          <div class="item-action">
            {#if $selectedRecording.soap_note}
              <span class="done-badge">Done ✓</span>
            {:else}
              <button class="btn-generate" on:click={() => handleGenerate('soap')}>
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
            {:else}
              <button class="btn-generate" on:click={() => handleGenerate('referral')}>
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
            {:else}
              <button class="btn-generate" on:click={() => handleGenerate('letter')}>
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
    padding: 8px 16px;
    background-color: var(--accent);
    color: white;
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 500;
    transition: background-color 0.15s ease;
  }

  .btn-generate:hover {
    background-color: var(--accent-hover);
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
