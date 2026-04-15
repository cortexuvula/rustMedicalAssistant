<script lang="ts">
  import { onMount } from 'svelte';
  import { settings } from '../stores/settings';
  import { theme } from '../stores/theme';
  import { listApiKeys, setApiKey } from '../api/settings';
  import { listModels, setActiveProvider, reinitProviders, type ModelInfo } from '../api/chat';
  import { listAudioDevices } from '../api/audio';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type { AudioDevice } from '../types';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';
  import { listWhisperModels, downloadModel, deleteModel, type ModelInfo as WhisperModelInfo } from '../api/models';

  type Section = 'general' | 'apikeys' | 'models' | 'audio';
  let activeSection = $state<Section>('general');

  const API_PROVIDERS = [
    { id: 'openai', label: 'OpenAI' },
    { id: 'anthropic', label: 'Anthropic' },
    { id: 'gemini', label: 'Gemini' },
    { id: 'groq', label: 'Groq' },
    { id: 'cerebras', label: 'Cerebras' },
  ];

  let storedKeys = $state<string[]>([]);
  let apiKeyInputs = $state<Record<string, string>>({});
  let saveStatus = $state<Record<string, 'idle' | 'saving' | 'saved' | 'error'>>({});

  for (const p of API_PROVIDERS) {
    apiKeyInputs[p.id] = '';
    saveStatus[p.id] = 'idle';
  }

  let availableModels = $state<ModelInfo[]>([]);
  let modelsLoading = $state(false);
  let modelMemory = $state<Record<string, string>>({});

  let audioDevices = $state<AudioDevice[]>([]);
  let devicesLoading = $state(false);

  let whisperModels = $state<WhisperModelInfo[]>([]);
  let modelsRefreshing = $state(false);
  let downloadingModel = $state<string | null>(null);
  let downloadProgress = $state<Record<string, { downloaded: number; total: number }>>({});
  let progressUnlisten: UnlistenFn | null = null;

  async function fetchAudioDevices() {
    devicesLoading = true;
    try {
      audioDevices = await listAudioDevices();
    } catch (e) {
      console.error('Failed to list audio devices:', e);
      audioDevices = [];
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

  async function handleDownloadModel(modelId: string) {
    downloadingModel = modelId;
    try {
      await downloadModel(modelId);
      await fetchWhisperModels();
    } catch (e) {
      console.error(`Failed to download model ${modelId}:`, e);
    } finally {
      downloadingModel = null;
    }
  }

  async function handleDeleteModel(modelId: string) {
    try {
      await deleteModel(modelId);
      await fetchWhisperModels();
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
    const [keys] = await Promise.allSettled([
      listApiKeys(),
      fetchModelsForProvider($settings.ai_provider),
      fetchAudioDevices(),
      fetchWhisperModels(),
    ]);
    if (keys.status === 'fulfilled') {
      storedKeys = keys.value;
    } else {
      console.error('Failed to list API keys:', keys.reason);
    }
  });

  // Listen for model download progress events
  listen<{ model_id: string; downloaded_bytes: number; total_bytes: number }>(
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
  ).then((unlisten) => {
    progressUnlisten = unlisten;
  });

  onDestroy(() => {
    progressUnlisten?.();
  });

  async function handleSaveApiKey(provider: string) {
    const key = apiKeyInputs[provider]?.trim();
    if (!key) return;

    saveStatus[provider] = 'saving';
    try {
      await setApiKey(provider, key);
      if (!storedKeys.includes(provider)) {
        storedKeys = [...storedKeys, provider];
      }
      apiKeyInputs[provider] = '';
      saveStatus[provider] = 'saved';
      // Rebuild AI + STT provider chains so new keys take effect immediately
      await reinitProviders();
      setTimeout(() => { saveStatus[provider] = 'idle'; }, 2000);
    } catch (err) {
      console.error(`Failed to save API key for ${provider}:`, err);
      saveStatus[provider] = 'error';
      setTimeout(() => { saveStatus[provider] = 'idle'; }, 3000);
    }
  }

  async function handleThemeChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value as 'light' | 'dark';
    theme.set(value);
    await settings.updateField('theme', value);
  }

  async function handleAutosaveChange(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    await settings.updateField('autosave_enabled', checked);
  }

  async function handleAutosaveIntervalChange(e: Event) {
    const value = parseInt((e.target as HTMLInputElement).value, 10);
    if (!isNaN(value) && value >= 10 && value <= 600) {
      await settings.updateField('autosave_interval_secs', value);
    }
  }

  async function handleBrowseStoragePath() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: 'Select Recording Storage Folder',
    });
    if (selected) {
      await settings.updateField('storage_path', selected);
    }
  }

  async function handleResetStoragePath() {
    await settings.updateField('storage_path', null);
  }

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
    { id: 'apikeys', label: 'API Keys' },
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
      <section class="settings-section">
        <h3 class="section-title">General</h3>

        <div class="form-group">
          <label for="theme-select" class="form-label">Theme</label>
          <select
            id="theme-select"
            value={$settings.theme}
            onchange={handleThemeChange}
          >
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </div>

        <div class="form-group">
          <label class="form-label checkbox-label">
            <input
              type="checkbox"
              checked={$settings.autosave_enabled}
              onchange={handleAutosaveChange}
            />
            <span>Enable Autosave</span>
          </label>
        </div>

        <div class="form-group">
          <label for="autosave-interval" class="form-label">
            Autosave Interval (seconds)
          </label>
          <input
            id="autosave-interval"
            type="number"
            min="10"
            max="600"
            value={$settings.autosave_interval_secs}
            onchange={handleAutosaveIntervalChange}
            disabled={!$settings.autosave_enabled}
          />
          <span class="form-hint">Between 10 and 600 seconds</span>
        </div>

        <div class="form-group">
          <span class="form-label">Recording Storage Folder</span>
          <div class="storage-path-row">
            <span class="storage-path-display">
              {$settings.storage_path || 'Default (application data)'}
            </span>
            <button class="btn-browse" onclick={handleBrowseStoragePath}>
              Browse
            </button>
            {#if $settings.storage_path}
              <button class="btn-reset" onclick={handleResetStoragePath}>
                Reset
              </button>
            {/if}
          </div>
          <span class="form-hint">Choose where audio recordings are saved. New recordings will use this folder.</span>
        </div>
      </section>

    {:else if activeSection === 'apikeys'}
      <section class="settings-section">
        <h3 class="section-title">API Keys</h3>
        <p class="section-desc">Keys are stored securely in the system keychain.</p>

        {#each API_PROVIDERS as provider}
          <div class="form-group api-key-row">
            <div class="api-key-label-row">
              <span class="form-label">{provider.label}</span>
              {#if storedKeys.includes(provider.id)}
                <span class="badge-stored">Stored</span>
              {/if}
            </div>
            <div class="api-key-input-row">
              <input
                type="password"
                placeholder={storedKeys.includes(provider.id) ? '••••••••••••' : `Enter ${provider.label} API key`}
                bind:value={apiKeyInputs[provider.id]}
              />
              <button
                class="btn-save"
                onclick={() => handleSaveApiKey(provider.id)}
                disabled={!apiKeyInputs[provider.id]?.trim() || saveStatus[provider.id] === 'saving'}
              >
                {#if saveStatus[provider.id] === 'saving'}
                  Saving…
                {:else if saveStatus[provider.id] === 'saved'}
                  Saved!
                {:else if saveStatus[provider.id] === 'error'}
                  Error
                {:else}
                  Save
                {/if}
              </button>
            </div>
          </div>
        {/each}
      </section>

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
            <option value="openai">OpenAI</option>
            <option value="anthropic">Anthropic</option>
            <option value="gemini">Gemini</option>
            <option value="groq">Groq</option>
            <option value="cerebras">Cerebras</option>
            <option value="ollama">Ollama</option>
            <option value="lmstudio">LM Studio</option>
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

  .section-desc {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: -8px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
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

  .storage-path-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .storage-path-display {
    flex: 1;
    font-size: 12px;
    color: var(--text-muted);
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .btn-browse,
  .btn-reset {
    flex-shrink: 0;
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background-color 0.15s ease;
  }

  .btn-browse {
    background-color: var(--accent);
    color: var(--text-inverse);
  }

  .btn-browse:hover {
    background-color: var(--accent-hover);
  }

  .btn-reset {
    color: var(--text-secondary);
    background-color: var(--bg-tertiary, #374151);
    border: 1px solid var(--border);
  }

  .btn-reset:hover {
    background-color: var(--bg-hover);
    color: var(--text-primary);
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

  .api-key-row {
    gap: 6px;
  }

  .api-key-label-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .badge-stored {
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

  .api-key-input-row {
    display: flex;
    gap: 8px;
  }

  .api-key-input-row input {
    flex: 1;
  }

  .btn-save {
    flex-shrink: 0;
    padding: 6px 14px;
    background-color: var(--accent);
    color: var(--text-inverse);
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 500;
    transition: background-color 0.15s ease, opacity 0.15s ease;
    white-space: nowrap;
  }

  .btn-save:hover:not(:disabled) {
    background-color: var(--accent-hover);
  }

  .btn-save:disabled {
    opacity: 0.5;
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
</style>
