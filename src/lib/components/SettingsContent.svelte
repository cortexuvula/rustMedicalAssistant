<script lang="ts">
  import { onMount } from 'svelte';
  import { settings } from '../stores/settings';
  import { theme } from '../stores/theme';
  import {
    testLmStudioConnection,
    testSttRemoteConnection,
    testOllamaConnection,
    setApiKey,
    getApiKey,
  } from '../api/settings';
  import { listModels, setActiveProvider, reinitProviders, type ModelInfo } from '../api/chat';
  import { listAudioDevices } from '../api/audio';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type { AudioDevice } from '../types';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onDestroy } from 'svelte';
  import { listWhisperModels, listPyannoteModels, downloadModel, deleteModel, type ModelInfo as WhisperModelInfo } from '../api/models';
  import VocabularyDialog from './VocabularyDialog.svelte';
  import { getVocabularyCount, importVocabularyJson, exportVocabularyJson } from '../api/vocabulary';
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import ContextTemplateDialog from './ContextTemplateDialog.svelte';
  import {
    importContextTemplatesJson,
    exportContextTemplatesJson,
  } from '../api/contextTemplates';
  import { contextTemplates } from '../stores/contextTemplates';
  import { getDefaultPrompt, type DocType } from '../api/prompts';
  import { toasts } from '../stores/toasts';
  import { formatError } from '../types/errors';

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
  let vocabDialogOpen = $state(false);
  let vocabCount = $state<[number, number]>([0, 0]);
  let ctxTemplateDialogOpen = $state(false);
  let ctxTemplateCount = $derived($contextTemplates.length);

  type PromptInfo = {
    key: DocType;
    label: string;
    configField: 'custom_soap_prompt' | 'custom_referral_prompt' | 'custom_letter_prompt' | 'custom_synopsis_prompt';
    placeholders: { token: string; description: string }[];
  };

  const PROMPT_TYPES: PromptInfo[] = [
    {
      key: 'soap',
      label: 'SOAP Note',
      configField: 'custom_soap_prompt',
      placeholders: [
        { token: '{icd_label}', description: 'ICD code header line (from ICD version setting)' },
        { token: '{icd_instruction}', description: 'Inline ICD reference phrase' },
        { token: '{template_guidance}', description: 'SOAP template hint (FollowUp, NewPatient, etc.)' },
      ],
    },
    {
      key: 'referral',
      label: 'Referral Letter',
      configField: 'custom_referral_prompt',
      placeholders: [
        { token: '{recipient_type}', description: 'e.g. Cardiologist, Orthopaedics' },
        { token: '{urgency}', description: 'routine, urgent, emergency' },
      ],
    },
    {
      key: 'letter',
      label: 'Patient Letter',
      configField: 'custom_letter_prompt',
      placeholders: [
        { token: '{letter_type}', description: 'e.g. results, instructions, follow-up' },
      ],
    },
    {
      key: 'synopsis',
      label: 'Clinical Synopsis',
      configField: 'custom_synopsis_prompt',
      placeholders: [],
    },
  ];

  let activePromptKey = $state<DocType>('soap');
  let promptEditorText = $state<string>('');
  let promptIsCustom = $state<boolean>(false);
  let promptDirty = $state<boolean>(false);
  let promptLoading = $state<boolean>(false);
  let promptSaveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');

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
      loadVocabCount(),
      contextTemplates.load(),
    ]);
    const labels = ['fetchModelsForProvider', 'fetchAudioDevices', 'fetchWhisperModels', 'fetchPyannoteModels', 'loadVocabCount', 'contextTemplates.load'];
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

  async function loadVocabCount() {
    try {
      vocabCount = await getVocabularyCount();
    } catch (err) {
      console.error('Failed to load vocabulary count:', err);
    }
  }

  async function handleImportVocabulary() {
    const selected = await openDialog({
      multiple: false,
      title: 'Import Vocabulary JSON',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await importVocabularyJson(selected as string);
        alert(`Imported ${count} vocabulary entries.`);
        await loadVocabCount();
      } catch (err: any) {
        alert(`Import failed: ${err}`);
      }
    }
  }

  async function handleExportVocabulary() {
    const selected = await saveDialog({
      title: 'Export Vocabulary JSON',
      defaultPath: 'vocabulary.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await exportVocabularyJson(selected);
        alert(`Exported ${count} vocabulary entries.`);
      } catch (err: any) {
        alert(`Export failed: ${err}`);
      }
    }
  }

  function handleVocabDialogClose() {
    vocabDialogOpen = false;
    loadVocabCount();
  }

  async function handleImportCtxTemplates() {
    const selected = await openDialog({
      multiple: false,
      title: 'Import Context Templates JSON',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await importContextTemplatesJson(selected as string);
        alert(`Imported ${count} context templates.`);
        await contextTemplates.load();
      } catch (err: any) {
        alert(`Import failed: ${err}`);
      }
    }
  }

  async function handleExportCtxTemplates() {
    const selected = await saveDialog({
      title: 'Export Context Templates JSON',
      defaultPath: 'context_templates.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      try {
        const count = await exportContextTemplatesJson(selected);
        alert(`Exported ${count} context templates.`);
      } catch (err: any) {
        alert(`Export failed: ${err}`);
      }
    }
  }

  function handleCtxTemplateDialogClose() {
    ctxTemplateDialogOpen = false;
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

  async function loadPromptEditor(docType: DocType) {
    promptLoading = true;
    promptDirty = false;
    promptSaveStatus = 'idle';
    try {
      const info = PROMPT_TYPES.find((p) => p.key === docType)!;
      const customValue = $settings?.[info.configField] as string | null | undefined;
      if (customValue && customValue.length > 0) {
        promptEditorText = customValue;
        promptIsCustom = true;
      } else {
        promptEditorText = await getDefaultPrompt(docType);
        promptIsCustom = false;
      }
    } catch (e) {
      console.error('Failed to load prompt editor:', e);
      promptEditorText = '';
      promptIsCustom = false;
    } finally {
      promptLoading = false;
    }
  }

  async function handlePromptSelect(docType: DocType) {
    if (promptDirty) {
      const confirmed = confirm('You have unsaved changes. Discard them?');
      if (!confirmed) return;
    }
    activePromptKey = docType;
  }

  async function handlePromptSave() {
    const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)!;
    promptSaveStatus = 'saving';
    try {
      await settings.updateField(info.configField, promptEditorText);
      promptIsCustom = true;
      promptDirty = false;
      promptSaveStatus = 'saved';
      setTimeout(() => { promptSaveStatus = 'idle'; }, 1500);
    } catch (e) {
      console.error('Failed to save custom prompt:', e);
      promptSaveStatus = 'error';
    }
  }

  async function handlePromptReset() {
    const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)!;
    if (promptIsCustom && !confirm('Clear the custom prompt and restore the default?')) return;
    try {
      await settings.updateField(info.configField, null);
      promptEditorText = await getDefaultPrompt(activePromptKey);
      promptIsCustom = false;
      promptDirty = false;
      promptSaveStatus = 'idle';
    } catch (e) {
      console.error('Failed to reset prompt:', e);
      promptSaveStatus = 'error';
    }
  }

  $effect(() => {
    if (activeSection === 'prompts') {
      loadPromptEditor(activePromptKey);
    }
  });

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

        <h3 class="section-title" style="margin-top: 24px">Custom Vocabulary</h3>
        <p class="section-desc">Automatically correct words in transcripts after speech-to-text.</p>

        <div class="form-group">
          <label class="toggle-label">
            <input
              type="checkbox"
              checked={$settings.vocabulary_enabled}
              onchange={() => settings.updateField('vocabulary_enabled', !$settings.vocabulary_enabled)}
            />
            <span>Enable vocabulary corrections</span>
          </label>
        </div>

        <div class="form-group">
          <span class="form-label">
            {vocabCount[0]} entries ({vocabCount[1]} enabled)
          </span>
          <div class="vocab-buttons">
            <button class="btn-browse" onclick={() => { vocabDialogOpen = true; }}>
              Manage Vocabulary
            </button>
            <button class="btn-browse" onclick={handleImportVocabulary}>
              Import JSON
            </button>
            <button class="btn-browse" onclick={handleExportVocabulary}>
              Export JSON
            </button>
          </div>
        </div>

        <h3 class="section-title" style="margin-top: 24px">Context Templates</h3>
        <p class="section-desc">Reusable snippets of clinical context that can be applied to the Patient Context field on the Record tab.</p>

        <div class="form-group">
          <span class="form-label">
            {ctxTemplateCount} template{ctxTemplateCount === 1 ? '' : 's'} saved
          </span>
          <div class="vocab-buttons">
            <button class="btn-browse" onclick={() => { ctxTemplateDialogOpen = true; }}>
              Manage Templates
            </button>
            <button class="btn-browse" onclick={handleImportCtxTemplates}>
              Import JSON
            </button>
            <button class="btn-browse" onclick={handleExportCtxTemplates}>
              Export JSON
            </button>
          </div>
        </div>
      </section>

    {:else if activeSection === 'prompts'}
      <section class="settings-section prompts-section">
        <h2>Prompts</h2>
        <p class="section-description">
          View and customize the system prompts sent to the AI for each document type.
          Placeholder tokens are substituted at generation time.
        </p>

        <div class="prompts-layout">
          <aside class="prompts-sidebar">
            {#each PROMPT_TYPES as pt}
              <button
                class="prompts-nav-item"
                class:active={activePromptKey === pt.key}
                onclick={() => handlePromptSelect(pt.key)}
              >
                {pt.label}
              </button>
            {/each}
          </aside>

          <div class="prompts-editor">
            {#if promptLoading}
              <div class="prompts-loading">Loading…</div>
            {:else}
              {@const info = PROMPT_TYPES.find((p) => p.key === activePromptKey)}
              <h3>{info?.label}</h3>

              <textarea
                class="prompt-textarea"
                bind:value={promptEditorText}
                oninput={() => (promptDirty = true)}
                rows="20"
                spellcheck="false"
              ></textarea>

              {#if info && info.placeholders.length > 0}
                <details class="prompts-placeholders">
                  <summary>Available placeholders</summary>
                  <ul>
                    {#each info.placeholders as ph}
                      <li>
                        <code>{ph.token}</code> — {ph.description}
                      </li>
                    {/each}
                  </ul>
                </details>
              {/if}

              <div class="prompts-status">
                Using: <strong>{promptIsCustom ? 'custom' : 'default'}</strong>
                {#if promptDirty}<span class="dirty-indicator"> (unsaved changes)</span>{/if}
              </div>

              <div class="prompts-actions">
                <button
                  class="btn btn-primary"
                  onclick={handlePromptSave}
                  disabled={!promptDirty || promptSaveStatus === 'saving'}
                >
                  {promptSaveStatus === 'saving' ? 'Saving…' : promptSaveStatus === 'saved' ? 'Saved' : 'Save as custom'}
                </button>
                <button
                  class="btn"
                  onclick={handlePromptReset}
                  disabled={!promptIsCustom && !promptDirty}
                >
                  Reset to default
                </button>
              </div>
              {#if promptSaveStatus === 'error'}
                <p class="error-message">Failed to save. See console for details.</p>
              {/if}
            {/if}
          </div>
        </div>
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

<VocabularyDialog open={vocabDialogOpen} onclose={handleVocabDialogClose} />
<ContextTemplateDialog open={ctxTemplateDialogOpen} onclose={handleCtxTemplateDialogClose} />

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

  .vocab-buttons {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .prompts-layout {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 1.25rem;
    align-items: start;
    margin-top: 1rem;
  }

  .prompts-sidebar {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    border-right: 1px solid var(--border);
    padding-right: 0.75rem;
  }

  .prompts-nav-item {
    text-align: left;
    padding: 0.5rem 0.75rem;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.9rem;
  }

  .prompts-nav-item:hover {
    background: var(--bg-hover);
  }

  .prompts-nav-item.active {
    background: var(--accent-light);
    border-color: var(--accent);
  }

  .prompts-editor {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .prompt-textarea {
    width: 100%;
    font-family: var(--font-mono, monospace);
    font-size: 0.85rem;
    line-height: 1.4;
    padding: 0.75rem;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 6px;
    resize: vertical;
    min-height: 400px;
  }

  .prompts-placeholders {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
  }

  .prompts-placeholders summary {
    cursor: pointer;
    font-weight: 500;
  }

  .prompts-placeholders ul {
    margin: 0.5rem 0 0;
    padding-left: 1.25rem;
  }

  .prompts-placeholders code {
    background: var(--bg-code);
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
    font-size: 0.85rem;
  }

  .prompts-status {
    font-size: 0.9rem;
    color: var(--text-secondary);
  }

  .prompts-status .dirty-indicator {
    color: var(--warning);
  }

  .prompts-actions {
    display: flex;
    gap: 0.5rem;
  }

  .prompts-loading {
    padding: 2rem;
    text-align: center;
    color: var(--text-secondary);
  }
</style>
