<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import ServerWizard from './sharing/ServerWizard.svelte';
  import ServerStatus from './sharing/ServerStatus.svelte';
  import ClientPair from './sharing/ClientPair.svelte';

  type Mode = 'off' | 'server' | 'client';
  let mode: Mode = 'off';
  let sharingOn = false;
  let pairedTo: string | null = null;

  async function refresh() {
    try {
      const status = await invoke<{ enabled: boolean }>('sharing_status');
      sharingOn = !!status.enabled;
    } catch {
      sharingOn = false;
    }
    try {
      const paired = await invoke<{ label: string } | null>('paired_endpoint');
      pairedTo = paired?.label ?? null;
    } catch {
      pairedTo = null;
    }
    if (sharingOn) mode = 'server';
    else if (pairedTo) mode = 'client';
    else mode = 'off';
  }
  onMount(refresh);
</script>

<div class="sharing">
  <h2>Sharing across machines</h2>
  <p class="hint">
    Run FerriScribe's heavy AI on one office computer and connect from your
    laptop or other clinicians' machines.
  </p>

  <div class="modes">
    <label><input type="radio" bind:group={mode} value="off" /> Off</label>
    <label><input type="radio" bind:group={mode} value="server" /> This machine is the office server</label>
    <label><input type="radio" bind:group={mode} value="client" /> This machine connects to an office server</label>
  </div>

  {#if mode === 'server' && !sharingOn}
    <ServerWizard on:done={refresh} />
  {:else if mode === 'server' && sharingOn}
    <ServerStatus on:stopped={refresh} />
  {:else if mode === 'client'}
    <ClientPair />
  {/if}
</div>

<style>
  .sharing { display: flex; flex-direction: column; gap: 1rem; }
  .modes { display: flex; gap: 1rem; }
  .hint { color: var(--text-muted, #888); }
</style>
