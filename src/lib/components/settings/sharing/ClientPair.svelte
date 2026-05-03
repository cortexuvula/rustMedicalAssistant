<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';

  type Discovered = {
    instance_name: string;
    host: string;
    addresses: string[];
    ports: { ollama: number | null; whisper: number | null; lmstudio: number | null; pairing: number | null };
    version: string;
  };

  let discovered: Discovered[] = [];
  let scanning = false;
  let pasteUrl = '';
  let label = '';
  let busy = false;
  let error: string | null = null;
  let success = false;

  async function rescan() {
    scanning = true;
    discovered = [];
    try {
      discovered = await invoke('discover_servers', { timeoutMs: 3000 });
    } finally {
      scanning = false;
    }
  }

  async function pairManual(serverUrl: string, code: string) {
    busy = true;
    error = null;
    try {
      const tokenLabel = label || 'this laptop';
      await invoke<string>('pair_with_server', {
        serverUrl,
        code,
        label: tokenLabel,
      });
      // Best-effort: record the label so Task 12 can pick it up.
      try {
        localStorage.setItem('ferriscribe_paired_token_label', tokenLabel);
      } catch {}
      success = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function pairFromUrl() {
    if (!pasteUrl.startsWith('ferriscribe://pair?')) {
      error = 'Not a FerriScribe pairing URL.';
      return;
    }
    const u = new URL(pasteUrl.replace('ferriscribe://', 'http://x/'));
    const lan = u.searchParams.get('lan');
    const ts = u.searchParams.get('ts');
    const pp = u.searchParams.get('pp');
    const code = u.searchParams.get('code') ?? '';
    const base = lan ? `http://${lan}:${pp}` : ts ? `http://${ts}:${pp}` : '';
    if (!base) { error = 'No reachable address in URL'; return; }
    pairManual(base, code);
  }

  function pairDiscovered(d: Discovered) {
    const lan = d.addresses[0];
    const port = d.ports.pairing ?? 11436;
    const code = prompt('Enter the 6-digit code from the office server.') ?? '';
    if (!code) return;
    pairManual(`http://${lan}:${port}`, code);
  }

  function onPairUrlEvent(e: Event) {
    const detail = (e as CustomEvent<string>).detail;
    if (typeof detail === 'string' && detail.startsWith('ferriscribe://pair?')) {
      pasteUrl = detail;
      pairFromUrl();
    }
  }

  onMount(() => {
    rescan();
    window.addEventListener('ferriscribe-pair-url', onPairUrlEvent);
  });

  onDestroy(() => {
    window.removeEventListener('ferriscribe-pair-url', onPairUrlEvent);
  });
</script>

<section>
  <h3>Connect to an office server</h3>
  {#if success}
    <div class="ok">Paired. The model pickers in Models settings now show
    the office server's installed models.</div>
  {:else}
    <div class="discovery">
      <h4>Found on your network</h4>
      {#if scanning}<p>Scanning...</p>{/if}
      {#if !scanning && discovered.length === 0}
        <p class="hint">No servers found. Either no office server is running,
        or your Wi-Fi blocks discovery (UniFi / Meraki client isolation).
        Use the QR or code option below.</p>
      {/if}
      <ul class="servers">
        {#each discovered as d}
          <li>
            <div>
              <strong>{d.host}</strong>
              <small>{d.addresses.join(', ')}</small>
            </div>
            <button onclick={() => pairDiscovered(d)}>Connect</button>
          </li>
        {/each}
      </ul>
      <button onclick={rescan}>Rescan</button>
    </div>

    <div class="paste">
      <h4>Or paste a pairing URL</h4>
      <input bind:value={pasteUrl} placeholder="ferriscribe://pair?..." />
      <input bind:value={label} placeholder="Label (e.g. Dr. Smith's MacBook)" />
      <button disabled={busy} onclick={pairFromUrl}>Pair</button>
    </div>

    {#if error}<div class="error">{error}</div>{/if}
  {/if}
</section>

<style>
  .ok { color: #080; }
  .error { color: #c00; }
  .servers { list-style: none; padding: 0; }
  .servers li { display: flex; justify-content: space-between; padding: 0.5rem 0; border-bottom: 1px solid var(--border, #ddd); }
</style>
