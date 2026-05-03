<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, createEventDispatcher } from 'svelte';
  import PairingQr from './PairingQr.svelte';
  const dispatch = createEventDispatcher();

  let qrPayload = '';
  let clients: { id: number; label: string }[] = [];
  let pollHandle: ReturnType<typeof setInterval>;

  async function refresh() {
    await invoke('sharing_status');
    clients = await invoke('list_paired_clients');
  }

  async function regenQr() {
    qrPayload = await invoke('pairing_qr');
  }

  async function revoke(id: number) {
    await invoke('revoke_client', { id });
    await refresh();
  }

  async function stop() {
    await invoke('stop_sharing');
    dispatch('stopped');
  }

  onMount(() => {
    refresh().then(() => regenQr());
    pollHandle = setInterval(refresh, 5000);
    return () => clearInterval(pollHandle);
  });
</script>

<section>
  <h3>This machine is the office server</h3>
  <div class="grid">
    <div>
      <h4>Pairing</h4>
      <PairingQr payload={qrPayload} />
      <button on:click={regenQr}>New code</button>
    </div>
    <div>
      <h4>Connected clients ({clients.length})</h4>
      {#if clients.length === 0}
        <p class="hint">No clinicians paired yet. Have them open
        Settings &rarr; Sharing &rarr; "This machine connects to an office server"
        and either pick this server from the list or scan the QR.</p>
      {:else}
        <ul class="clients">
          {#each clients as c}
            <li>{c.label} <button on:click={() => revoke(c.id)}>Revoke</button></li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
  <button class="danger" on:click={stop}>Stop sharing</button>
</section>

<style>
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 2rem; }
  .danger { color: #c00; margin-top: 1rem; }
  .clients { list-style: none; padding: 0; }
  .clients li { display: flex; justify-content: space-between; padding: 0.25rem 0; }
</style>
