<script lang="ts">
  import { onMount } from 'svelte';
  import { recordings, loading, selectedRecording, selectRecording } from '../stores/recordings';
  import SearchBar from '../components/SearchBar.svelte';
  import RecordingCard from '../components/RecordingCard.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';

  let deleteTarget = $state<{ id: string; name: string } | null>(null);

  onMount(() => {
    recordings.load();
  });

  function requestDelete(id: string, name: string) {
    deleteTarget = { id, name };
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    try {
      await recordings.remove(deleteTarget.id);
    } catch (err) {
      console.error('Failed to delete recording:', err);
    } finally {
      deleteTarget = null;
    }
  }
</script>

<div class="recordings-tab">
  <SearchBar
    placeholder="Search recordings…"
    onSearch={(q) => recordings.search(q)}
  />

  <div class="recordings-list">
    {#if $loading}
      <div class="state-msg">
        <span>Loading recordings…</span>
      </div>

    {:else if $recordings.length === 0}
      <div class="state-msg">
        <div class="state-icon">📋</div>
        <p>No recordings yet.</p>
        <p class="hint">Go to the <strong>Record</strong> tab to capture audio.</p>
      </div>

    {:else}
      {#each $recordings as rec (rec.id)}
        <RecordingCard
          recording={rec}
          selected={$selectedRecording?.id === rec.id}
          onClick={() => selectRecording(rec.id)}
          onDelete={() => requestDelete(rec.id, rec.patient_name || rec.filename)}
        />
      {/each}
    {/if}
  </div>
</div>

<ConfirmDialog
  open={deleteTarget !== null}
  title="Delete Recording"
  message={deleteTarget ? `This will permanently delete "${deleteTarget.name}" including its audio file, transcript, SOAP note, and all generated documents.` : ''}
  confirmLabel="Delete"
  onConfirm={confirmDelete}
  onCancel={() => deleteTarget = null}
/>

<style>
  .recordings-tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .recordings-list {
    flex: 1;
    overflow-y: auto;
  }

  .state-msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 40px 20px;
    text-align: center;
    color: var(--text-muted);
    gap: 6px;
  }

  .state-icon {
    font-size: 40px;
    margin-bottom: 8px;
  }

  p {
    font-size: 14px;
  }

  .hint {
    font-size: 12px;
  }

  strong {
    color: var(--text-secondary);
  }
</style>
