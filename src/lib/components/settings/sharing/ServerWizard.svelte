<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { createEventDispatcher } from 'svelte';
  const dispatch = createEventDispatcher();

  let friendlyName = 'Clinic Server';
  let busy = false;
  let error: string | null = null;

  async function start() {
    busy = true;
    error = null;
    try {
      await invoke('start_sharing', { friendlyName });
      dispatch('done');
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section>
  <h3>Become office server</h3>
  <ol class="steps">
    <li>Friendly name (visible to clinicians on this machine):
      <input bind:value={friendlyName} />
    </li>
    <li>FerriScribe will configure persistent Ollama, download whisper.cpp,
        start an authenticated proxy, and advertise this server on the
        local network.</li>
    <li>If LM Studio is installed, open it and click "Start Server" in its
        Local Server tab. (We don't manage LM Studio's server toggle.)</li>
    <li>Your operating system may ask permission for FerriScribe to accept
        incoming connections. Click <b>Allow</b>.</li>
  </ol>
  <button disabled={busy} on:click={start}>
    {busy ? 'Setting up…' : 'Start sharing'}
  </button>
  {#if error}<div class="error">{error}</div>{/if}
</section>

<style>
  .error { color: #c00; margin-top: 0.5rem; }
  .steps { padding-left: 1.2rem; }
</style>
