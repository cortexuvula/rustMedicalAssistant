<script lang="ts">
  import type { RecordingSummary } from '../types';

  export let recording: RecordingSummary;
  export let selected: boolean = false;
  export let onClick: () => void = () => {};

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  }

  function formatDuration(seconds: number | null): string {
    if (seconds === null) return '--:--';
    const m = Math.floor(seconds / 60).toString().padStart(2, '0');
    const s = (seconds % 60).toString().padStart(2, '0');
    return `${m}:${s}`;
  }

  function statusIcon(status: RecordingSummary['status']): string {
    switch (status.status) {
      case 'completed':   return '✓';
      case 'processing':  return '⟳';
      case 'failed':      return '✗';
      default:            return '—';
    }
  }

  function statusColor(status: RecordingSummary['status']): string {
    switch (status.status) {
      case 'completed':  return 'var(--success)';
      case 'processing': return 'var(--warning)';
      case 'failed':     return 'var(--danger)';
      default:           return 'var(--text-muted)';
    }
  }
</script>

<button
  class="recording-card"
  class:selected
  on:click={onClick}
>
  <div class="card-status" style="color: {statusColor(recording.status)}">
    {statusIcon(recording.status)}
  </div>

  <div class="card-main">
    <div class="card-name truncate">
      {recording.patient_name ?? recording.filename}
    </div>
    <div class="card-meta">
      <span>{formatDate(recording.created_at)}</span>
      <span class="sep">·</span>
      <span>{formatDuration(recording.duration_seconds)}</span>
    </div>
  </div>

  <div class="card-badges">
    {#if recording.has_transcript}
      <span class="badge" title="Transcript">T</span>
    {/if}
    {#if recording.has_soap_note}
      <span class="badge" title="SOAP Note">S</span>
    {/if}
    {#if recording.has_referral}
      <span class="badge" title="Referral">R</span>
    {/if}
  </div>
</button>

<style>
  .recording-card {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-light);
    text-align: left;
    transition: background-color 0.1s ease;
    background-color: transparent;
  }

  .recording-card:hover {
    background-color: var(--bg-hover);
  }

  .recording-card.selected {
    background-color: var(--accent-light);
  }

  .card-status {
    font-size: 14px;
    width: 18px;
    flex-shrink: 0;
    text-align: center;
  }

  .card-main {
    flex: 1;
    min-width: 0;
  }

  .card-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    margin-bottom: 2px;
  }

  .card-meta {
    font-size: 11px;
    color: var(--text-muted);
    display: flex;
    gap: 4px;
  }

  .sep {
    color: var(--border);
  }

  .card-badges {
    display: flex;
    gap: 3px;
    flex-shrink: 0;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 700;
    background-color: var(--accent-light);
    color: var(--accent);
    border: 1px solid var(--accent);
  }
</style>
