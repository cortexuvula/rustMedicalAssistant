<script lang="ts">
  import './app.css';
  import { onMount } from 'svelte';
  import { settings } from './lib/stores/settings';
  import { theme } from './lib/stores/theme';

  import Sidebar from './lib/components/Sidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import SettingsDialog from './lib/dialogs/SettingsDialog.svelte';

  // Pages
  import RecordTab from './lib/pages/RecordTab.svelte';
  import RecordingsTab from './lib/pages/RecordingsTab.svelte';
  import GenerateTab from './lib/pages/GenerateTab.svelte';
  import ChatTab from './lib/pages/ChatTab.svelte';
  import EditorTab from './lib/pages/EditorTab.svelte';

  let activeTab = $state('record');
  let settingsOpen = $state(false);
  let previousTab = $state('record');

  // Intercept settings tab — open modal instead of navigating
  $effect(() => {
    if (activeTab === 'settings') {
      settingsOpen = true;
      activeTab = previousTab;
    } else {
      previousTab = activeTab;
    }
  });

  onMount(async () => {
    await settings.load();
    // Set theme from loaded settings
    const unsubscribe = settings.subscribe((cfg) => {
      theme.set(cfg.theme);
    });
    return unsubscribe;
  });
</script>

<div class="app-shell">
  <aside class="app-sidebar">
    <Sidebar bind:activeTab />
  </aside>

  <main class="app-content">
    {#if activeTab === 'record'}
      <RecordTab />
    {:else if activeTab === 'recordings'}
      <RecordingsTab />
    {:else if activeTab === 'generate'}
      <GenerateTab />
    {:else if activeTab === 'chat'}
      <ChatTab />
    {:else if activeTab === 'transcript'}
      <EditorTab tabId="transcript" />
    {:else if activeTab === 'soap'}
      <EditorTab tabId="soap" />
    {:else if activeTab === 'referral'}
      <EditorTab tabId="referral" />
    {:else if activeTab === 'letter'}
      <EditorTab tabId="letter" />
    {/if}
  </main>

  <footer class="app-statusbar">
    <StatusBar />
  </footer>
</div>

<SettingsDialog bind:open={settingsOpen} />

<style>
  .app-shell {
    display: grid;
    grid-template-columns: var(--sidebar-width) 1fr;
    grid-template-rows: 1fr var(--statusbar-height);
    height: 100vh;
    overflow: hidden;
  }

  .app-sidebar {
    grid-column: 1;
    grid-row: 1;
    overflow: hidden;
  }

  .app-content {
    grid-column: 2;
    grid-row: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background-color: var(--bg-primary);
  }

  .app-statusbar {
    grid-column: 1 / -1;
    grid-row: 2;
  }
</style>
