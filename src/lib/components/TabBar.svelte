<script lang="ts">
  interface Props {
    tabs: { id: string; label: string }[];
    activeTab: string;
  }

  let { tabs = [], activeTab = $bindable('') }: Props = $props();
</script>

<div class="tabbar">
  {#each tabs as tab}
    <button
      class="tab"
      class:active={activeTab === tab.id}
      onclick={() => (activeTab = tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</div>

<style>
  .tabbar {
    display: flex;
    align-items: stretch;
    background-color: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    overflow-x: auto;
  }

  .tab {
    padding: 8px 16px;
    font-size: 13px;
    color: var(--text-secondary);
    position: relative;
    white-space: nowrap;
    transition: color 0.12s ease;
  }

  .tab:hover {
    color: var(--text-primary);
  }

  .tab.active {
    color: var(--accent);
    font-weight: 500;
  }

  .tab.active::after {
    content: '';
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 2px;
    background-color: var(--accent);
    border-radius: 1px 1px 0 0;
  }
</style>
