<script lang="ts">
  import { onMount } from 'svelte';
  import { recordings, loading, selectedRecording, selectRecording } from '../stores/recordings';
  import SearchBar from '../components/SearchBar.svelte';
  import RecordingCard from '../components/RecordingCard.svelte';

  onMount(() => {
    recordings.load();
  });
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
        />
      {/each}
    {/if}
  </div>
</div>

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
