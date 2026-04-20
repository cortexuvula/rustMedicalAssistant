<script lang="ts">
  import {
    listVocabularyEntries,
    addVocabularyEntry,
    updateVocabularyEntry,
    deleteVocabularyEntry,
    deleteAllVocabularyEntries,
    testVocabularyCorrection,
    type VocabularyEntry,
    type CorrectionResult,
  } from '../api/vocabulary';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let entries = $state<VocabularyEntry[]>([]);
  let loading = $state(false);
  let filterCategory = $state('all');
  let searchText = $state('');

  // Add/Edit form
  let editing = $state<VocabularyEntry | null>(null);
  let showForm = $state(false);
  let formFind = $state('');
  let formReplace = $state('');
  let formCategory = $state('general');
  let formCaseSensitive = $state(false);
  let formPriority = $state(0);
  let formEnabled = $state(true);
  let formError = $state('');

  // Test area
  let testInput = $state('');
  let testResult = $state<CorrectionResult | null>(null);

  const CATEGORIES = [
    { value: 'general', label: 'General' },
    { value: 'doctor_names', label: 'Doctor Names' },
    { value: 'medication_names', label: 'Medications' },
    { value: 'medical_terminology', label: 'Terminology' },
    { value: 'abbreviations', label: 'Abbreviations' },
  ];

  function categoryLabel(value: string): string {
    return CATEGORIES.find((c) => c.value === value)?.label ?? value;
  }

  async function loadEntries() {
    loading = true;
    try {
      const cat = filterCategory === 'all' ? undefined : filterCategory;
      entries = await listVocabularyEntries(cat);
    } catch (err) {
      console.error('Failed to load vocabulary entries:', err);
    } finally {
      loading = false;
    }
  }

  function filteredEntries(): VocabularyEntry[] {
    if (!searchText.trim()) return entries;
    const q = searchText.toLowerCase();
    return entries.filter(
      (e) =>
        e.find_text.toLowerCase().includes(q) ||
        e.replacement.toLowerCase().includes(q),
    );
  }

  function openAddForm() {
    editing = null;
    formFind = '';
    formReplace = '';
    formCategory = 'general';
    formCaseSensitive = false;
    formPriority = 0;
    formEnabled = true;
    formError = '';
    showForm = true;
  }

  function openEditForm(entry: VocabularyEntry) {
    editing = entry;
    formFind = entry.find_text;
    formReplace = entry.replacement;
    formCategory = entry.category;
    formCaseSensitive = entry.case_sensitive;
    formPriority = entry.priority;
    formEnabled = entry.enabled;
    formError = '';
    showForm = true;
  }

  function closeForm() {
    showForm = false;
    editing = null;
    formError = '';
  }

  async function handleSave() {
    if (!formFind.trim() || !formReplace.trim()) {
      formError = 'Find and replacement text are required.';
      return;
    }
    try {
      if (editing) {
        await updateVocabularyEntry(
          editing.id,
          formFind.trim(),
          formReplace.trim(),
          formCategory,
          formCaseSensitive,
          formPriority,
          formEnabled,
        );
      } else {
        await addVocabularyEntry(
          formFind.trim(),
          formReplace.trim(),
          formCategory,
          formCaseSensitive,
          formPriority,
          formEnabled,
        );
      }
      closeForm();
      await loadEntries();
    } catch (err: any) {
      formError = err?.toString() || 'Failed to save entry.';
    }
  }

  async function handleDelete(entry: VocabularyEntry) {
    if (!confirm(`Delete correction "${entry.find_text}" \u2192 "${entry.replacement}"?`)) return;
    try {
      await deleteVocabularyEntry(entry.id);
      await loadEntries();
    } catch (err) {
      console.error('Failed to delete entry:', err);
    }
  }

  async function handleDeleteAll() {
    if (!confirm(`Delete ALL ${entries.length} vocabulary entries? This cannot be undone.`)) return;
    try {
      await deleteAllVocabularyEntries();
      await loadEntries();
    } catch (err) {
      console.error('Failed to delete all entries:', err);
    }
  }

  async function handleToggleEnabled(entry: VocabularyEntry) {
    try {
      await updateVocabularyEntry(
        entry.id,
        entry.find_text,
        entry.replacement,
        entry.category,
        entry.case_sensitive,
        entry.priority,
        !entry.enabled,
      );
      await loadEntries();
    } catch (err) {
      console.error('Failed to toggle entry:', err);
    }
  }

  async function handleTest() {
    if (!testInput.trim()) return;
    try {
      testResult = await testVocabularyCorrection(testInput);
    } catch (err) {
      console.error('Test failed:', err);
    }
  }

  $effect(() => {
    if (open) {
      loadEntries();
      testResult = null;
    }
  });

  $effect(() => {
    filterCategory;
    if (open) loadEntries();
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="vocab-overlay" onclick={onclose}>
    <div class="vocab-dialog" onclick={(e) => e.stopPropagation()}>
      <div class="vocab-header">
        <h2>Manage Vocabulary</h2>
        <button class="btn-close" onclick={onclose}>&times;</button>
      </div>

      <div class="vocab-toolbar">
        <select bind:value={filterCategory}>
          <option value="all">All Categories</option>
          {#each CATEGORIES as cat}
            <option value={cat.value}>{cat.label}</option>
          {/each}
        </select>
        <input
          type="text"
          placeholder="Search..."
          bind:value={searchText}
        />
        <button class="btn-add" onclick={openAddForm}>+ Add Entry</button>
      </div>

      {#if showForm}
        <div class="vocab-form">
          <h3>{editing ? 'Edit' : 'Add'} Entry</h3>
          {#if formError}
            <div class="form-error">{formError}</div>
          {/if}
          <div class="form-row">
            <label>
              Find Text
              <input type="text" bind:value={formFind} placeholder="e.g. htn" />
            </label>
            <label>
              Replacement
              <input type="text" bind:value={formReplace} placeholder="e.g. hypertension" />
            </label>
          </div>
          <div class="form-row">
            <label>
              Category
              <select bind:value={formCategory}>
                {#each CATEGORIES as cat}
                  <option value={cat.value}>{cat.label}</option>
                {/each}
              </select>
            </label>
            <label>
              Priority
              <input type="number" bind:value={formPriority} min="0" max="100" />
            </label>
          </div>
          <div class="form-row">
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={formCaseSensitive} />
              Case Sensitive
            </label>
            <label class="checkbox-label">
              <input type="checkbox" bind:checked={formEnabled} />
              Enabled
            </label>
          </div>
          <div class="form-actions">
            <button class="btn-save" onclick={handleSave}>Save</button>
            <button class="btn-cancel" onclick={closeForm}>Cancel</button>
          </div>
        </div>
      {/if}

      <div class="vocab-table-wrap">
        {#if loading}
          <p class="loading-text">Loading...</p>
        {:else if filteredEntries().length === 0}
          <p class="empty-text">No vocabulary entries found.</p>
        {:else}
          <table class="vocab-table">
            <thead>
              <tr>
                <th>Find</th>
                <th>Replace With</th>
                <th>Category</th>
                <th>Enabled</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {#each filteredEntries() as entry (entry.id)}
                <tr class:disabled={!entry.enabled}>
                  <td class="mono">{entry.find_text}</td>
                  <td>{entry.replacement}</td>
                  <td>{categoryLabel(entry.category)}</td>
                  <td>
                    <input
                      type="checkbox"
                      checked={entry.enabled}
                      onchange={() => handleToggleEnabled(entry)}
                    />
                  </td>
                  <td class="actions">
                    <button class="btn-edit" onclick={() => openEditForm(entry)}>Edit</button>
                    <button class="btn-delete" onclick={() => handleDelete(entry)}>Del</button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

      <div class="vocab-test">
        <h3>Test Corrections</h3>
        <textarea
          bind:value={testInput}
          placeholder="Paste sample text to test corrections..."
          rows="3"
        ></textarea>
        <button class="btn-test" onclick={handleTest} disabled={!testInput.trim()}>
          Test
        </button>
        {#if testResult}
          <div class="test-result">
            <strong>{testResult.total_replacements} replacement{testResult.total_replacements !== 1 ? 's' : ''}</strong>
            <pre>{testResult.corrected_text}</pre>
          </div>
        {/if}
      </div>

      <div class="vocab-footer">
        <button class="btn-delete-all" onclick={handleDeleteAll} disabled={entries.length === 0}>
          Delete All ({entries.length})
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .vocab-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .vocab-dialog {
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border-radius: 8px;
    width: 90vw;
    max-width: 800px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .vocab-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }
  .vocab-header h2 { margin: 0; font-size: 1.2rem; }
  .btn-close {
    background: none;
    border: none;
    color: var(--text-secondary, #aaa);
    font-size: 1.5rem;
    cursor: pointer;
  }
  .vocab-toolbar {
    display: flex;
    gap: 8px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }
  .vocab-toolbar select,
  .vocab-toolbar input {
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
  }
  .vocab-toolbar input { flex: 1; }
  .btn-add {
    padding: 6px 14px;
    border-radius: 4px;
    border: none;
    background: var(--accent-color, #4a9eff);
    color: white;
    cursor: pointer;
    white-space: nowrap;
  }
  .vocab-form {
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-color, #333);
    background: var(--bg-primary, #111);
  }
  .vocab-form h3 { margin: 0 0 8px; font-size: 0.95rem; }
  .form-error { color: #ff6b6b; margin-bottom: 8px; font-size: 0.85rem; }
  .form-row { display: flex; gap: 12px; margin-bottom: 8px; }
  .form-row label { flex: 1; display: flex; flex-direction: column; gap: 4px; font-size: 0.85rem; }
  .form-row input[type="text"],
  .form-row input[type="number"],
  .form-row select {
    padding: 6px 8px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-secondary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
  }
  .checkbox-label { flex-direction: row !important; align-items: center; gap: 6px !important; }
  .form-actions { display: flex; gap: 8px; margin-top: 8px; }
  .btn-save { padding: 6px 16px; border-radius: 4px; border: none; background: var(--accent-color, #4a9eff); color: white; cursor: pointer; }
  .btn-cancel { padding: 6px 16px; border-radius: 4px; border: 1px solid var(--border-color, #444); background: transparent; color: var(--text-primary, #e0e0e0); cursor: pointer; }
  .vocab-table-wrap {
    flex: 1;
    overflow-y: auto;
    padding: 0 20px;
  }
  .loading-text, .empty-text { text-align: center; color: var(--text-secondary, #888); padding: 24px; }
  .vocab-table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  .vocab-table th { text-align: left; padding: 8px 6px; border-bottom: 1px solid var(--border-color, #333); color: var(--text-secondary, #888); font-weight: 500; }
  .vocab-table td { padding: 6px; border-bottom: 1px solid var(--border-color, #222); }
  .vocab-table tr.disabled { opacity: 0.5; }
  .mono { font-family: monospace; }
  .actions { display: flex; gap: 4px; }
  .btn-edit, .btn-delete { padding: 3px 8px; border-radius: 3px; border: 1px solid var(--border-color, #444); background: transparent; color: var(--text-secondary, #aaa); cursor: pointer; font-size: 0.8rem; }
  .btn-delete { color: #ff6b6b; border-color: #ff6b6b44; }
  .vocab-test {
    padding: 12px 20px;
    border-top: 1px solid var(--border-color, #333);
  }
  .vocab-test h3 { margin: 0 0 8px; font-size: 0.95rem; }
  .vocab-test textarea {
    width: 100%;
    padding: 8px;
    border-radius: 4px;
    border: 1px solid var(--border-color, #444);
    background: var(--bg-primary, #111);
    color: var(--text-primary, #e0e0e0);
    resize: vertical;
    font-family: inherit;
  }
  .btn-test { margin-top: 6px; padding: 6px 14px; border-radius: 4px; border: none; background: var(--accent-color, #4a9eff); color: white; cursor: pointer; }
  .test-result { margin-top: 8px; }
  .test-result pre { background: var(--bg-primary, #111); padding: 8px; border-radius: 4px; white-space: pre-wrap; font-size: 0.85rem; margin-top: 4px; }
  .vocab-footer {
    padding: 12px 20px;
    border-top: 1px solid var(--border-color, #333);
    display: flex;
    justify-content: flex-end;
  }
  .btn-delete-all { padding: 6px 14px; border-radius: 4px; border: 1px solid #ff6b6b44; background: transparent; color: #ff6b6b; cursor: pointer; }
</style>
