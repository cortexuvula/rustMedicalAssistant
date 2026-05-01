<script lang="ts">
  import { onMount } from 'svelte';
  import { settings } from '../stores/settings';
  import {
    testLmStudioConnection,
    testSttRemoteConnection,
    testOllamaConnection,
    setApiKey,
    getApiKey,
  } from '../api/settings';
  import { listModels, setActiveProvider, reinitProviders, type ModelInfo } from '../api/chat';
  import { listAudioDevices } from '../api/audio';
  import type { AudioDevice } from '../types';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';
  import { listWhisperModels, listPyannoteModels, downloadModel, deleteModel, type ModelInfo as WhisperModelInfo } from '../api/models';
  import { toasts } from '../stores/toasts';
  import { formatError } from '../types/errors';
  import General from './settings/General.svelte';
  import Prompts from './settings/Prompts.svelte';

  type Section = 'general' | 'prompts' | 'models' | 'audio';
  let activeSection = $state<Section>('general');

  let availableModels = $state<ModelInfo[]>([]);
  let modelsLoading = $state(false);
  let modelMemory = $state<Record<string, string>>({});

  let audioDevices = $state<AudioDevice[]>([]);
  let devicesLoading = $state(false);

  let whisperModels = $state<WhisperModelInfo[]>([]);
  let pyannoteModels = $state<WhisperModelInfo[]>([]);
  let modelsRefreshing = $state(false);
  let downloadingModel = $state<string | null>(null);
  let downloadProgress = $state<Record<string, { downloaded: number; total: number }>>({});
  let lmstudioTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let lmstudioTestMessage = $state('');
  let sttMode = $state<'local' | 'remote'>(($settings.stt_mode as 'local' | 'remote') ?? 'local');
  let sttRemoteTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let sttRemoteTestMessage = $state('');
  let sttRemoteApiKey = $state('');
  let ollamaTestStatus = $state<'idle' | 'testing' | 'success' | 'error'>('idle');
  let ollamaTestMessage = $state('');
  let progressUnlisten: UnlistenFn | null = null;

  async function fetchAudioDevices() {
    devicesLoading = true;
    try {
      audioDevices = await listAudioDevices();
    } catch (e) {
      console.error('Failed to list audio devices:', e);
      audioDevices = [];
      toasts.error(`Failed to list audio devices: ${e}`);
    } finally {
      devicesLoading = false;
    }
  }

  async function fetchWhisperModels() {
    modelsRefreshing = true;
    try {
      whisperModels = await listWhisperModels();
    } catch (e) {
      console.error('Failed to list whisper models:', e);
    } finally {
      modelsRefreshing = false;
    }
  }

  async function fetchPyannoteModels() {
    try {
      pyannoteModels = await listPyannoteModels();
    } catch (e) {
      console.error('Failed to list pyannote models:', e);
    }
  }

  async function handleDownloadModel(modelId: string) {
    downloadingModel = modelId;
    try {
      await downloadModel(modelId);
      await Promise.all([fetchWhisperModels(), fetchPyannoteModels()]);
    } catch (e) {
      console.error(`Failed to download model ${modelId}:`, e);
    } finally {
      downloadingModel = null;
    }
  }

  async function handleDeleteModel(modelId: string) {
    try {
      await deleteModel(modelId);
      await Promise.all([fetchWhisperModels(), fetchPyannoteModels()]);
    } catch (e) {
      console.error(`Failed to delete model ${modelId}:`, e);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(0)} MB`;
    return `${(bytes / 1073741824).toFixed(1)} GB`;
  }

  async function handleWhisperModelChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('whisper_model', value);
  }

  async function fetchModelsForProvider(provider: string) {
    modelsLoading = true;
    try {
      availableModels = await listModels(provider);
    } catch (e) {
      console.error('Failed to fetch models:', e);
      availableModels = [];
    } finally {
      modelsLoading = false;
    }
  }

  onMount(async () => {
    if ($settings.ai_provider && $settings.ai_model) {
      modelMemory[$settings.ai_provider] = $settings.ai_model;
    }
    const results = await Promise.allSettled([
      fetchModelsForProvider($settings.ai_provider),
      fetchAudioDevices(),
      fetchWhisperModels(),
      fetchPyannoteModels(),
    ]);
    const labels = ['fetchModelsForProvider', 'fetchAudioDevices', 'fetchWhisperModels', 'fetchPyannoteModels'];
    for (const [i, r] of results.entries()) {
      if (r.status === 'rejected') {
        console.error(`Settings init: ${labels[i]} failed:`, r.reason);
      }
    }

    // Load the persisted STT remote API key so the password field reflects it.
    getApiKey('stt_remote_api_key').then((key) => {
      if (key) sttRemoteApiKey = key;
    }).catch(() => { /* ignore — keychain miss is fine */ });

    // Listen for model download progress events
    progressUnlisten = await listen<{ model_id: string; downloaded_bytes: number; total_bytes: number }>(
      'model-download-progress',
      (event) => {
        downloadProgress = {
          ...downloadProgress,
          [event.payload.model_id]: {
            downloaded: event.payload.downloaded_bytes,
            total: event.payload.total_bytes,
          },
        };
      }
    );
  });

  onDestroy(() => {
    progressUnlisten?.();
  });

  async function handleAiProviderChange(e: Event) {
    const newProvider = (e.target as HTMLSelectElement).value;
    const oldProvider = $settings.ai_provider;
    if (oldProvider && $settings.ai_model) {
      modelMemory[oldProvider] = $settings.ai_model;
    }
    await settings.updateField('ai_provider', newProvider);
    await setActiveProvider(newProvider);
    await fetchModelsForProvider(newProvider);
    const remembered = modelMemory[newProvider];
    if (remembered && availableModels.some((m) => m.id === remembered)) {
      await settings.updateField('ai_model', remembered);
    } else if (availableModels.length > 0) {
      await settings.updateField('ai_model', availableModels[0].id);
    }
  }

  async function handleAiModelChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('ai_model', value);
    modelMemory[$settings.ai_provider] = value;
  }

  async function handleTemperatureChange(e: Event) {
    const value = parseFloat((e.target as HTMLInputElement).value);
    await settings.updateField('temperature', value);
  }

  async function handleLmStudioHostChange(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    await settings.updateField('lmstudio_host', value);
    lmstudioTestStatus = 'idle';
    lmstudioTestMessage = '';
    await reinitProviders();
  }

  async function handleLmStudioPortChange(e: Event) {
    const value = parseInt((e.target as HTMLInputElement).value, 10);
    if (value >= 1 && value <= 65535) {
      await settings.updateField('lmstudio_port', value);
      lmstudioTestStatus = 'idle';
      lmstudioTestMessage = '';
      await reinitProviders();
    }
  }

  async function handleTestLmStudioConnection() {
    lmstudioTestStatus = 'testing';
    lmstudioTestMessage = '';
    try {
      const host = $settings.lmstudio_host || 'localhost';
      const port = $settings.lmstudio_port || 1234;
      const msg = await testLmStudioConnection(host, port);
      lmstudioTestStatus = 'success';
      lmstudioTestMessage = msg;
    } catch (err: any) {
      lmstudioTestStatus = 'error';
      lmstudioTestMessage = formatError(err) || 'Connection failed';
    }
  }

  async function handleInputDeviceChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('input_device', value || null);
  }

  async function handleSampleRateChange(e: Event) {
    const value = parseInt((e.target as HTMLSelectElement).value, 10);
    await settings.updateField('sample_rate', value);
  }

  const navItems: { id: Section; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'prompts', label: 'Prompts' },
    { id: 'models', label: 'AI Models' },
    { id: 'audio', label: 'Audio / STT' },
  ];
</script>

<div class="settings-layout">
  <nav class="settings-nav">
    {#each navItems as item}
      <button
        class="nav-item"
        class:active={activeSection === item.id}
        onclick={() => (activeSection = item.id)}
      >
        {item.label}
      </button>
    {/each}
  </nav>

  <div class="settings-content">
    {#if activeSection === 'general'}
      <General />

    {:else if activeSection === 'prompts'}
      <Prompts />

    {:else if activeSection === 'models'}
      <section class="settings-section">
        <h3 class="section-title">AI Models</h3>

        <div class="form-group">
          <label for="ai-provider" class="form-label">AI Provider</label>
          <select
            id="ai-provider"
            value={$settings.ai_provider}
            onchange={handleAiProviderChange}
          >
            <option value="lmstudio">LM Studio</option>
            <option value="ollama">Ollama</option>
          </select>
        </div>

        <div class="form-group">
          <label for="ai-model" class="form-label">Model</label>
          <div class="model-select-row">
            <select
              id="ai-model"
              value={$settings.ai_model}
              onchange={handleAiModelChange}
              disabled={modelsLoading}
            >
              {#if modelsLoading}
                <option value="">Loading models…</option>
              {:else if availableModels.length === 0}
                <option value="">No models available</option>
              {:else}
                {#each availableModels as model}
                  <option value={model.id}>{model.name}</option>
                {/each}
              {/if}
            </select>
            <button
              class="btn-refresh"
              onclick={() => fetchModelsForProvider($settings.ai_provider)}
              disabled={modelsLoading}
              title="Refresh model list"
            >
              {modelsLoading ? '…' : '↻'}
            </button>
          </div>
        </div>

        <div class="form-group">
          <label for="temperature" class="form-label">
            Temperature
            <span class="value-display">{$settings.temperature.toFixed(1)}</span>
          </label>
          <input
            id="temperature"
            type="range"
            min="0"
            max="2"
            step="0.1"
            value={$settings.temperature}
            oninput={handleTemperatureChange}
            class="range-input"
          />
          <div class="range-labels">
            <span>0 (Precise)</span>
            <span>2 (Creative)</span>
          </div>
        </div>

        <!-- LM Studio Server -->
        <div class="form-group-divider"></div>
        <h4 class="subsection-title">LM Studio Server</h4>
        <p class="subsection-hint">
          Configure the LM Studio server address. Use <code>localhost</code> if LM Studio runs on this machine, or enter a remote IP for a network server.
        </p>

        <div class="form-group">
          <label for="lmstudio-host" class="form-label">Host</label>
          <input
            id="lmstudio-host"
            type="text"
            value={$settings.lmstudio_host}
            placeholder="localhost"
            onchange={handleLmStudioHostChange}
            class="text-input"
          />
        </div>

        <div class="form-group">
          <label for="lmstudio-port" class="form-label">Port</label>
          <input
            id="lmstudio-port"
            type="number"
            value={$settings.lmstudio_port}
            placeholder="1234"
            min="1"
            max="65535"
            onchange={handleLmStudioPortChange}
            class="text-input port-input"
          />
        </div>

        <div class="form-group">
          <button
            class="btn-test-connection"
            onclick={handleTestLmStudioConnection}
            disabled={lmstudioTestStatus === 'testing'}
          >
            {#if lmstudioTestStatus === 'testing'}
              Testing…
            {:else}
              Test Connection
            {/if}
          </button>
          {#if lmstudioTestStatus === 'success'}
            <span class="test-result test-success">✓ {lmstudioTestMessage}</span>
          {:else if lmstudioTestStatus === 'error'}
            <span class="test-result test-error">✗ {lmstudioTestMessage}</span>
          {/if}
        </div>

        <!-- Ollama Server -->
        <div class="form-group-divider"></div>
        <h4 class="subsection-title">Ollama Server</h4>
        <p class="subsection-hint">
          Configure the Ollama server address. Use <code>localhost</code> if Ollama runs on this machine, or enter a remote IP / Tailscale hostname for a network server.
        </p>

        <div class="form-group">
          <label for="ollama-host" class="form-label">Host</label>
          <input
            id="ollama-host"
            type="text"
            value={$settings.ollama_host ?? ''}
            placeholder="localhost"
            onchange={async (e) => {
              await settings.updateField('ollama_host', (e.target as HTMLInputElement).value);
              ollamaTestStatus = 'idle';
              ollamaTestMessage = '';
              await reinitProviders();
            }}
            class="text-input"
          />
        </div>

        <div class="form-group">
          <label for="ollama-port" class="form-label">Port</label>
          <input
            id="ollama-port"
            type="number"
            value={$settings.ollama_port ?? 11434}
            placeholder="11434"
            min="1"
            max="65535"
            onchange={async (e) => {
              const value = parseInt((e.target as HTMLInputElement).value, 10);
              if (value >= 1 && value <= 65535) {
                await settings.updateField('ollama_port', value);
                ollamaTestStatus = 'idle';
                ollamaTestMessage = '';
                await reinitProviders();
              }
            }}
            class="text-input port-input"
          />
        </div>

        <div class="form-group">
          <button
            class="btn-test-connection"
            disabled={ollamaTestStatus === 'testing'}
            onclick={async () => {
              ollamaTestStatus = 'testing';
              ollamaTestMessage = '';
              try {
                const msg = await testOllamaConnection(
                  $settings.ollama_host || 'localhost',
                  $settings.ollama_port || 11434,
                );
                ollamaTestStatus = 'success';
                ollamaTestMessage = msg;
              } catch (err: any) {
                ollamaTestStatus = 'error';
                ollamaTestMessage = formatError(err) || 'Connection failed';
              }
            }}
          >
            {#if ollamaTestStatus === 'testing'}
              Testing…
            {:else}
              Test Connection
            {/if}
          </button>
          {#if ollamaTestStatus === 'success'}
            <span class="test-result test-success">✓ {ollamaTestMessage}</span>
          {:else if ollamaTestStatus === 'error'}
            <span class="test-result test-error">✗ {ollamaTestMessage}</span>
          {/if}
        </div>
      </section>

    {:else if activeSection === 'audio'}
      <section class="settings-section">
        <h3 class="section-title">Audio / STT</h3>

        <div class="form-group">
          <label for="input-device" class="form-label">Input Device</label>
          <select
            id="input-device"
            value={$settings.input_device ?? ''}
            onchange={handleInputDeviceChange}
            disabled={devicesLoading}
          >
            {#if devicesLoading}
              <option value="">Loading devices...</option>
            {:else}
              <option value="">System Default</option>
              {#each audioDevices as device}
                <option value={device.name}>
                  {device.name}{device.is_default ? ' (Default)' : ''}
                </option>
              {/each}
            {/if}
          </select>
        </div>

        <fieldset class="form-group radio-fieldset">
          <legend class="form-label">STT Mode</legend>
          <div class="radio-row">
            <label class="radio-label">
              <input
                type="radio"
                bind:group={sttMode}
                value="local"
                onchange={async () => {
                  await settings.updateField('stt_mode', sttMode);
                  await reinitProviders();
                }}
              /> Local
            </label>
            <label class="radio-label">
              <input
                type="radio"
                bind:group={sttMode}
                value="remote"
                onchange={async () => {
                  await settings.updateField('stt_mode', sttMode);
                  await reinitProviders();
                }}
              /> Remote
            </label>
          </div>
        </fieldset>

        {#if sttMode === 'local'}
          <div class="form-group">
            <label for="whisper-model" class="form-label">Whisper Model</label>
            <select
              id="whisper-model"
              value={$settings.whisper_model}
              onchange={handleWhisperModelChange}
              disabled={modelsRefreshing}
            >
              {#each whisperModels as model}
                <option value={model.id}>
                  {model.id} ({formatBytes(model.size_bytes)}) {model.downloaded ? '' : '- not downloaded'}
                </option>
              {/each}
            </select>
            <span class="form-hint">Larger models are more accurate but use more memory and take longer.</span>
          </div>

          <div class="form-group">
            <span class="form-label">Model Management</span>
            <div class="model-list">
              {#each whisperModels as model}
                <div class="model-row">
                  <div class="model-info">
                    <span class="model-name">{model.id}</span>
                    <span class="model-desc">{model.description}</span>
                    <span class="model-size">{formatBytes(model.size_bytes)}</span>
                  </div>
                  <div class="model-actions">
                    {#if model.downloaded}
                      <span class="badge-downloaded">Downloaded</span>
                      <button
                        class="btn-delete-model"
                        onclick={() => handleDeleteModel(model.id)}
                        disabled={model.id === $settings.whisper_model}
                        title={model.id === $settings.whisper_model ? 'Cannot delete the active model' : 'Delete to free disk space'}
                      >
                        Delete
                      </button>
                    {:else if downloadingModel === model.id}
                      <span class="download-progress">
                        {#if downloadProgress[model.id]}
                          {Math.round((downloadProgress[model.id].downloaded / (downloadProgress[model.id].total || 1)) * 100)}%
                        {:else}
                          Starting...
                        {/if}
                      </span>
                    {:else}
                      <button
                        class="btn-download-model"
                        onclick={() => handleDownloadModel(model.id)}
                        disabled={downloadingModel !== null}
                      >
                        Download
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <div class="form-group">
            <label for="stt-remote-host" class="form-label">Host</label>
            <input
              id="stt-remote-host"
              type="text"
              placeholder="computer-a.tailnet.ts.net"
              value={$settings.stt_remote_host ?? ''}
              onchange={async (e) => {
                await settings.updateField('stt_remote_host', (e.target as HTMLInputElement).value);
                sttRemoteTestStatus = 'idle';
                sttRemoteTestMessage = '';
                await reinitProviders();
              }}
              class="text-input"
            />
          </div>
          <div class="form-group">
            <label for="stt-remote-port" class="form-label">Port</label>
            <input
              id="stt-remote-port"
              type="number"
              value={$settings.stt_remote_port ?? 8080}
              min="1"
              max="65535"
              onchange={async (e) => {
                const value = parseInt((e.target as HTMLInputElement).value, 10);
                if (value >= 1 && value <= 65535) {
                  await settings.updateField('stt_remote_port', value);
                  sttRemoteTestStatus = 'idle';
                  sttRemoteTestMessage = '';
                  await reinitProviders();
                }
              }}
              class="text-input port-input"
            />
          </div>
          <div class="form-group">
            <label for="stt-remote-model" class="form-label">Model</label>
            <input
              id="stt-remote-model"
              type="text"
              value={$settings.stt_remote_model ?? ''}
              onchange={async (e) => {
                await settings.updateField('stt_remote_model', (e.target as HTMLInputElement).value);
                await reinitProviders();
              }}
              class="text-input"
            />
            <span class="form-hint">Model name as served by your Whisper server (e.g. <code>whisper-1</code>).</span>
          </div>
          <div class="form-group">
            <label for="stt-remote-key" class="form-label">API key (optional)</label>
            <input
              id="stt-remote-key"
              type="password"
              bind:value={sttRemoteApiKey}
              class="text-input"
            />
            <button
              class="btn-test-connection"
              type="button"
              onclick={async () => {
                try {
                  await setApiKey('stt_remote_api_key', sttRemoteApiKey);
                  sttRemoteTestMessage = 'Key saved.';
                  sttRemoteTestStatus = 'success';
                  await reinitProviders();
                } catch (err) {
                  sttRemoteTestStatus = 'error';
                  sttRemoteTestMessage = `Failed to save key: ${err}`;
                }
              }}
            >Save key</button>
            <span class="form-hint">Leave blank and click Save to clear.</span>
          </div>
          <div class="form-group">
            <button
              class="btn-test-connection"
              type="button"
              disabled={sttRemoteTestStatus === 'testing'}
              onclick={async () => {
                sttRemoteTestStatus = 'testing';
                sttRemoteTestMessage = '';
                try {
                  const msg = await testSttRemoteConnection(
                    $settings.stt_remote_host || 'localhost',
                    $settings.stt_remote_port || 8080,
                    sttRemoteApiKey || null,
                  );
                  sttRemoteTestStatus = 'success';
                  sttRemoteTestMessage = msg;
                } catch (err: any) {
                  sttRemoteTestStatus = 'error';
                  sttRemoteTestMessage = formatError(err) || 'Connection failed';
                }
              }}
            >{sttRemoteTestStatus === 'testing' ? 'Testing…' : 'Test Connection'}</button>
            {#if sttRemoteTestStatus === 'success'}
              <span class="test-result test-success">✓ {sttRemoteTestMessage}</span>
            {:else if sttRemoteTestStatus === 'error'}
              <span class="test-result test-error">✗ {sttRemoteTestMessage}</span>
            {/if}
          </div>
        {/if}

        <p class="form-hint">Diarization runs on this machine regardless of STT mode — pyannote models below are required for speaker labels.</p>

        <div class="form-group">
          <span class="form-label">Diarization Models (Speaker Identification)</span>
          <span class="form-hint">Both models are required for speaker diarization. Without them, transcripts will not have speaker labels.</span>
          <div class="model-list">
            {#each pyannoteModels as model}
              <div class="model-row">
                <div class="model-info">
                  <span class="model-name">{model.id}</span>
                  <span class="model-desc">{model.description}</span>
                  <span class="model-size">{formatBytes(model.size_bytes)}</span>
                </div>
                <div class="model-actions">
                  {#if model.downloaded}
                    <span class="badge-downloaded">Downloaded</span>
                    <button
                      class="btn-delete-model"
                      onclick={() => handleDeleteModel(model.id)}
                    >
                      Delete
                    </button>
                  {:else if downloadingModel === model.id}
                    <span class="download-progress">
                      {#if downloadProgress[model.id]}
                        {Math.round((downloadProgress[model.id].downloaded / (downloadProgress[model.id].total || 1)) * 100)}%
                      {:else}
                        Starting...
                      {/if}
                    </span>
                  {:else}
                    <button
                      class="btn-download-model"
                      onclick={() => handleDownloadModel(model.id)}
                      disabled={downloadingModel !== null}
                    >
                      Download
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>

        <div class="form-group">
          <label for="sample-rate" class="form-label">Sample Rate</label>
          <select
            id="sample-rate"
            value={$settings.sample_rate}
            onchange={handleSampleRateChange}
          >
            <option value={16000}>16000 Hz</option>
            <option value={44100}>44100 Hz</option>
            <option value={48000}>48000 Hz</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label checkbox-label">
            <input
              type="checkbox"
              checked={$settings.auto_generate_soap}
              onchange={(e: Event) => {
                const checked = (e.target as HTMLInputElement).checked;
                settings.updateField('auto_generate_soap', checked);
              }}
            />
            <span>Auto-generate SOAP after recording</span>
          </label>
          <span class="form-hint">When enabled, transcription and SOAP generation start automatically after you stop recording.</span>
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  .settings-layout {
    display: flex;
    height: 100%;
    min-height: 400px;
  }

  .settings-nav {
    width: 130px;
    flex-shrink: 0;
    background-color: var(--bg-secondary);
    border-right: 1px solid var(--border);
    padding: 8px 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .nav-item {
    width: 100%;
    text-align: left;
    padding: 8px 14px;
    font-size: 13px;
    color: var(--text-secondary);
    border-radius: 0;
    transition: background-color 0.15s ease, color 0.15s ease;
  }

  .nav-item:hover {
    background-color: var(--bg-hover);
    color: var(--text-primary);
  }

  .nav-item.active {
    background-color: var(--bg-active);
    color: var(--accent);
    font-weight: 500;
  }

  .settings-content {
    flex: 1;
    overflow-y: auto;
    padding: 20px;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-light);
    margin-bottom: 4px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .radio-fieldset {
    border: 0;
    padding: 0;
    margin: 0 0 0.75rem 0;
  }
  .radio-fieldset legend {
    padding: 0;
    margin-bottom: 0.25rem;
  }

  .form-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .form-hint {
    font-size: 11px;
    color: var(--text-muted);
  }

  .checkbox-label {
    cursor: pointer;
    user-select: none;
  }

  .checkbox-label input[type='checkbox'] {
    width: auto;
    cursor: pointer;
  }

  .radio-row {
    display: flex;
    gap: 16px;
    align-items: center;
  }

  .radio-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    user-select: none;
    font-size: 13px;
    color: var(--text-primary);
  }

  .radio-label input[type='radio'] {
    width: auto;
    cursor: pointer;
    margin: 0;
  }

  .model-select-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .model-select-row select {
    flex: 1;
  }

  .btn-refresh {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    color: var(--text-secondary);
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease;
  }

  .btn-refresh:hover:not(:disabled) {
    background-color: var(--bg-hover);
    color: var(--text-primary);
  }

  .btn-refresh:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .value-display {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    background-color: var(--accent-light);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
  }

  .range-input {
    width: 100%;
    padding: 0;
    border: none;
    background: none;
    box-shadow: none;
    accent-color: var(--accent);
    cursor: pointer;
    height: 20px;
  }

  .range-input:focus {
    box-shadow: none;
    border-color: transparent;
  }

  .range-labels {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-muted);
  }

  .model-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    gap: 12px;
  }

  .model-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .model-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .model-desc {
    font-size: 11px;
    color: var(--text-muted);
  }

  .model-size {
    font-size: 11px;
    color: var(--text-muted);
  }

  .model-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .badge-downloaded {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--success);
    background-color: color-mix(in srgb, var(--success) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
  }

  .download-progress {
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
  }

  .btn-download-model {
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 500;
    background-color: var(--accent);
    color: var(--text-inverse);
    border-radius: var(--radius-sm);
    transition: background-color 0.15s ease;
  }

  .btn-download-model:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-download-model:disabled {
    opacity: 0.5;
  }

  .btn-delete-model {
    padding: 4px 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--danger, #ef4444);
    background-color: transparent;
    border: 1px solid var(--danger, #ef4444);
    border-radius: var(--radius-sm);
    transition: background-color 0.15s ease;
  }

  .btn-delete-model:hover:not(:disabled) {
    background-color: rgba(239, 68, 68, 0.1);
  }

  .btn-delete-model:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .form-group-divider {
    border-top: 1px solid var(--border);
    margin: 20px 0 16px;
  }

  .subsection-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 4px;
  }

  .subsection-hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 0 0 12px;
    line-height: 1.5;
  }

  .subsection-hint code {
    font-size: 11px;
    background-color: var(--bg-tertiary, #374151);
    padding: 1px 5px;
    border-radius: 3px;
  }

  .port-input {
    max-width: 120px;
  }

  .btn-test-connection {
    padding: 6px 14px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }

  .btn-test-connection:hover:not(:disabled) {
    background-color: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--accent);
  }

  .btn-test-connection:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .test-result {
    font-size: 13px;
    margin-left: 10px;
  }

  .test-success {
    color: #22c55e;
  }

  .test-error {
    color: var(--danger, #ef4444);
  }

</style>
