<script lang="ts">
  import './app.css';
  import { onMount, onDestroy } from 'svelte';
  import { settings } from './lib/stores/settings';
  import { theme } from './lib/stores/theme';
  import { generation } from './lib/stores/generation';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';

  import Sidebar from './lib/components/Sidebar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import SettingsDialog from './lib/dialogs/SettingsDialog.svelte';
  import { selectedRecording, selectRecording } from './lib/stores/recordings';
  import { pipeline } from './lib/stores/pipeline';
  import { toasts } from './lib/stores/toasts';
  import ToastContainer from './lib/components/ToastContainer.svelte';

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

  let progressUnlisten: UnlistenFn | null = null;
  let pipelineCompleteUnlisten: UnlistenFn | null = null;
  let pipelineFailedUnlisten: UnlistenFn | null = null;
  let settingsUnsubscribe: (() => void) | null = null;

  async function navigateToSoap(tab: string, recordingId: string) {
    await selectRecording(recordingId);
    activeTab = tab;
  }

  onMount(async () => {
    // Tear down any prior listeners (Vite HMR re-runs onMount without onDestroy)
    progressUnlisten?.();
    pipelineCompleteUnlisten?.();
    pipelineFailedUnlisten?.();
    pipeline.destroy();

    // Listen for generation progress events globally so state persists across tab switches
    progressUnlisten = await listen<{ type: string; status: string }>(
      'generation-progress',
      (event) => {
        generation.setProgress(`${event.payload.type}: ${event.payload.status}`);
      }
    );

    await settings.load();
    // Set theme from loaded settings
    settingsUnsubscribe = settings.subscribe((cfg) => {
      theme.set(cfg.theme);
    });

    await pipeline.init();

    pipelineCompleteUnlisten = await listen<{ recording_id: string; display_name: string }>(
      'pipeline-complete',
      (event) => {
        const { recording_id, display_name } = event.payload;
        toasts.add({
          message: `SOAP note ready for ${display_name}`,
          type: 'success',
          recordingId: recording_id,
          displayName: display_name,
          autoDismiss: true,
        });
      },
    );

    pipelineFailedUnlisten = await listen<{ recording_id: string; stage: string; error?: string }>(
      'pipeline-progress',
      (event) => {
        if (event.payload.stage === 'failed') {
          toasts.add({
            message: `Processing failed: ${event.payload.error ?? 'Unknown error'}`,
            type: 'error',
            recordingId: event.payload.recording_id,
            autoDismiss: false,
          });
        }
      },
    );
  });

  onDestroy(() => {
    progressUnlisten?.();
    settingsUnsubscribe?.();
    pipeline.destroy();
    pipelineCompleteUnlisten?.();
    pipelineFailedUnlisten?.();
  });
</script>

<div class="app-shell">
  <aside class="app-sidebar">
    <Sidebar bind:activeTab />
  </aside>

  <main class="app-content">
    {#if $selectedRecording}
      <div class="selected-recording-banner">
        <span class="banner-icon">🎙</span>
        <span class="banner-name">{$selectedRecording.patient_name || $selectedRecording.filename}</span>
        <span class="banner-meta">{new Date($selectedRecording.created_at).toLocaleDateString()}</span>
      </div>
    {/if}
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

  <ToastContainer onNavigate={navigateToSoap} />
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

  .selected-recording-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 16px;
    background-color: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    flex-shrink: 0;
  }

  .banner-icon {
    font-size: 14px;
  }

  .banner-name {
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .banner-meta {
    color: var(--text-muted);
    margin-left: auto;
    flex-shrink: 0;
  }
</style>
