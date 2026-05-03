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

  type PairPorts = { ollama: number; whisper: number; pairing: number; lmstudio: number | null };

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

  async function pairManual(
    lan: string | null,
    tailscale: string | null,
    ports: PairPorts,
    code: string,
  ) {
    busy = true;
    error = null;
    try {
      const tokenLabel = label || 'this laptop';
      await invoke('pair_with_server', {
        lan,
        tailscale,
        ports,
        code,
        label: tokenLabel,
      });
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
    const op = Number(u.searchParams.get('op') ?? '11435');
    const wp = Number(u.searchParams.get('wp') ?? '8081');
    const pp = Number(u.searchParams.get('pp') ?? '11436');
    const lp = u.searchParams.has('lp') ? Number(u.searchParams.get('lp')) : null;
    const code = u.searchParams.get('code') ?? '';
    if (!lan && !ts) { error = 'No reachable address in URL'; return; }
    pairManual(lan, ts, { ollama: op, whisper: wp, pairing: pp, lmstudio: lp }, code);
  }

  function pairDiscovered(d: Discovered) {
    const lan = d.addresses[0] ?? null;
    const ports: PairPorts = {
      ollama: d.ports.ollama ?? 11435,
      whisper: d.ports.whisper ?? 8081,
      pairing: d.ports.pairing ?? 11436,
      lmstudio: d.ports.lmstudio ?? null,
    };
    const code = prompt('Enter the 6-digit code from the office server.') ?? '';
    if (!code) return;
    pairManual(lan, null, ports, code);
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
