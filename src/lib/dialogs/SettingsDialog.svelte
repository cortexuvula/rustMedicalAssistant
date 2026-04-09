<script lang="ts">
  import { onMount } from 'svelte';
  import Modal from '../components/Modal.svelte';
  import { settings } from '../stores/settings';
  import { theme } from '../stores/theme';
  import { listApiKeys, setApiKey } from '../api/settings';

  interface Props {
    open: boolean;
  }

  let { open = $bindable() }: Props = $props();

  type Section = 'general' | 'apikeys' | 'models' | 'audio';
  let activeSection = $state<Section>('general');

  // API Keys state
  const API_PROVIDERS = [
    { id: 'openai', label: 'OpenAI' },
    { id: 'anthropic', label: 'Anthropic' },
    { id: 'gemini', label: 'Gemini' },
    { id: 'groq', label: 'Groq' },
    { id: 'cerebras', label: 'Cerebras' },
    { id: 'deepgram', label: 'Deepgram' },
    { id: 'elevenlabs', label: 'ElevenLabs' },
    { id: 'modulate', label: 'Modulate' },
  ];

  let storedKeys = $state<string[]>([]);
  let apiKeyInputs = $state<Record<string, string>>({});
  let saveStatus = $state<Record<string, 'idle' | 'saving' | 'saved' | 'error'>>({});

  // Initialize save status for all providers
  for (const p of API_PROVIDERS) {
    apiKeyInputs[p.id] = '';
    saveStatus[p.id] = 'idle';
  }

  onMount(async () => {
    try {
      storedKeys = await listApiKeys();
    } catch (err) {
      console.error('Failed to list API keys:', err);
    }
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
      setTimeout(() => {
        saveStatus[provider] = 'idle';
      }, 2000);
    } catch (err) {
      console.error(`Failed to save API key for ${provider}:`, err);
      saveStatus[provider] = 'error';
      setTimeout(() => {
        saveStatus[provider] = 'idle';
      }, 3000);
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

  async function handleAiProviderChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('ai_provider', value);
  }

  async function handleTemperatureChange(e: Event) {
    const value = parseFloat((e.target as HTMLInputElement).value);
    await settings.updateField('temperature', value);
  }

  async function handleSttProviderChange(e: Event) {
    const value = (e.target as HTMLSelectElement).value;
    await settings.updateField('stt_provider', value);
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

<Modal {open} title="Settings" onClose={() => (open = false)}>
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
            </select>
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
            <label for="stt-provider" class="form-label">STT Provider</label>
            <select
              id="stt-provider"
              value={$settings.stt_provider}
              onchange={handleSttProviderChange}
            >
              <option value="deepgram">Deepgram</option>
              <option value="groq">Groq Whisper</option>
              <option value="elevenlabs">ElevenLabs</option>
              <option value="modulate">Modulate</option>
              <option value="whisper-local">Whisper (Local)</option>
            </select>
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
        </section>
      {/if}
    </div>
  </div>
</Modal>

<style>
  .settings-layout {
    display: flex;
    height: 100%;
    min-height: 400px;
  }

  .settings-nav {
    width: 120px;
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
    padding: 8px 12px;
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

  /* API Keys */
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

  /* Temperature range */
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
</style>
