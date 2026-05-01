# Split `SettingsContent.svelte` Into Per-Section Components — Design

**Date:** 2026-05-01
**Status:** Approved (ready for implementation plan)
**Sprint:** 2 / B3

## Problem

`src/lib/components/SettingsContent.svelte` is 1,717 LOC and renders four mutually-exclusive sections (General, Prompts, AI Models, Audio/STT) via an `activeSection` state. The file mixes ~500 LOC of state and handlers, ~680 LOC of markup across four conditional branches, and ~520 LOC of scoped CSS. It is the largest Svelte file in the project and is repeatedly flagged in audits as a maintenance bottleneck — every settings change forces a maintainer to scan a file that does five different jobs at once.

This release splits the monolith into four per-section child components plus a thin parent that owns only the tab-nav state and conditional rendering. Behavior is preserved exactly; the change is invisible to users.

## Goals

- Each settings section lives in its own focused file with its own state, handlers, markup, and scoped CSS.
- The parent `SettingsContent.svelte` shrinks to roughly 80 LOC.
- No regression: every settings interaction works identically after the split.
- Incremental commits: one per section so any regression is bisectable.
- No new dependencies, no DB migration, no feature flag.

## Non-goals

- Adding new settings categories.
- Refactoring the tab-nav UX (keyboard shortcuts, mobile collapse, etc.).
- Reworking how settings are persisted — still goes through the existing `$settings` store.
- Splitting `RecordTab.svelte` (B4 — separate cycle).

## Decisions

| # | Decision |
|---|---|
| Q1 | File layout: new `src/lib/components/settings/` subdirectory with section files named without prefix (`General.svelte`, `Prompts.svelte`, `Models.svelte`, `Audio.svelte`). Inside the dir, the prefix is redundant. |
| Q2 | Commit cadence: one section per commit (5 commits total, including version bump). Each commit leaves the parent functional. |
| Q3 | Inter-component communication: each section reads global stores directly (`$settings`, `$theme`, `$contextTemplates`); no props, no event bubbling. Matches the current monolith's pattern. |
| Q4 | Lifecycle hooks: each section owns its own `onMount` / `onDestroy`. The download-progress event-listener (currently in the parent) moves into `Audio.svelte` since only it consumes the events. |
| Q5 | Dialog ownership: `<VocabularyDialog>` and `<ContextTemplateDialog>` (currently rendered at the parent's bottom) move into `General.svelte` since that section opens them. |
| Q6 | CSS strategy: each section owns the scoped rules that style its own markup; rules that style elements in multiple sections (`.form-group`, `.section-title`, `.form-label`, `.btn-primary`) stay in the parent. |

## Target architecture

```
src/lib/components/
├── SettingsContent.svelte           (~80 LOC after split)
│       ├── activeSection state ('general' | 'prompts' | 'models' | 'audio')
│       ├── Tab-nav rendering
│       └── Conditional <General /> / <Prompts /> / <Models /> / <Audio />
│
└── settings/                         (new subdirectory)
    ├── General.svelte                (~250 LOC)
    ├── Prompts.svelte                (~150 LOC)
    ├── Models.svelte                 (~370 LOC)
    └── Audio.svelte                  (~600 LOC)
```

Each section component:
- Owns its `$state` declarations (extracted from the current parent block).
- Owns its handlers (extracted from current parent functions).
- Owns its scoped `<style>` rules (CSS that only applies to its markup moves with it).
- Owns its `onMount` / `onDestroy` lifecycle hooks where needed.
- Owns its dialogs (in `General.svelte`'s case).

The parent retains:
- The `activeSection` `$state` and the tab-nav buttons that switch it.
- The four `{#if activeSection === ...}` branches importing one component each.
- `<style>` rules that style the tab nav itself plus shared form-control styles used across multiple sections.

## Per-section split mapping

### `settings/General.svelte`

**State to move:** `vocabDialogOpen`, `vocabCount`, `ctxTemplateDialogOpen`, `ctxTemplateCount`.

**Handlers to move:** `handleThemeChange`, `handleAutosaveChange`, `handleAutosaveIntervalChange`, `handleBrowseStoragePath`, `handleResetStoragePath`, `loadVocabCount`, `handleImportVocabulary`, `handleExportVocabulary`, `handleVocabDialogClose`, `handleImportCtxTemplates`, `handleExportCtxTemplates`, `handleCtxTemplateDialogClose`.

**Markup to move:** lines 507-619 of current file (General settings + Custom Vocabulary subsection + Context Templates subsection).

**Dialogs to move:** `<VocabularyDialog>` and `<ContextTemplateDialog>` — currently at the parent's bottom — render inside this component instead.

**onMount:** the existing `loadVocabCount()` call lifts here.

### `settings/Prompts.svelte`

**State to move:** `PROMPT_TYPES` const, `activePromptKey`, `promptEditorText`, `promptIsCustom`, `promptDirty`, `promptLoading`, `promptSaveStatus`.

**Handlers to move:** `loadPromptEditor`, `handlePromptSelect`, `handlePromptSave`, `handlePromptReset`.

**Markup to move:** lines 620-697 of current file.

**onMount:** the initial `loadPromptEditor('soap')` call lifts here.

### `settings/Models.svelte`

**State to move:** `availableModels`, `modelsLoading`, `modelMemory`, `lmstudioTestStatus`, `lmstudioTestMessage`, `ollamaTestStatus`, `ollamaTestMessage`.

**Handlers to move:** `fetchModelsForProvider`, `handleAiProviderChange`, `handleAiModelChange`, `handleTemperatureChange`, `handleLmStudioHostChange`, `handleLmStudioPortChange`, `handleTestLmStudioConnection`, plus equivalent Ollama host/port handlers.

**Markup to move:** lines 698-896 of current file.

**onMount:** any AI-provider-related model fetch on mount lifts here.

### `settings/Audio.svelte`

**State to move:** `audioDevices`, `devicesLoading`, `whisperModels`, `pyannoteModels`, `modelsRefreshing`, `downloadingModel`, `downloadProgress`, `sttMode`, `sttRemoteTestStatus`, `sttRemoteTestMessage`, `sttRemoteApiKey`, `progressUnlisten`.

**Handlers to move:** `fetchAudioDevices`, `fetchWhisperModels`, `fetchPyannoteModels`, `handleDownloadModel`, `handleDeleteModel`, `formatBytes`, `handleWhisperModelChange`, `handleInputDeviceChange`, `handleSampleRateChange`, plus STT-remote test handlers.

**Markup to move:** lines 897-1190 of current file.

**Lifecycle:** the download-progress event-listener (currently in parent's `onMount` + `onDestroy`) moves entirely into this component.

### Parent `SettingsContent.svelte`

**Retains:** `activeSection` state + tab-nav handler + the four `{#if activeSection === ...}` branches each rendering one child component.

**Removes:** every state / handler / markup / CSS rule listed for the four sections.

**External API preserved:** `SettingsDialog.svelte` and `SettingsPage.svelte` continue to render `<SettingsContent />` unchanged — no callers of this component need updating.

## CSS migration strategy

When extracting a section, walk the current `<style>` block (lines 1197-1717) and move every rule whose selector matches a class used only inside that section's markup. Rules that style elements appearing in multiple sections stay in the parent.

If during extraction a rule's section ownership is genuinely unclear, the implementer **leaves it in the parent** and adds a `/* TODO: classify */` comment in the diff so the reviewer can decide. This avoids forcing the implementer into judgment calls under time pressure.

After all four sections are extracted, the parent's `<style>` block should contain only:
- Tab-nav styles (`.settings-nav`, `.nav-button`, etc.).
- Layout styles for `.settings-content` and `.settings-section` (wrapping containers).
- Genuinely-shared form-control rules used by multiple sections.

## Behavior preservation

The refactor must not change observable behavior. Two specific concerns to call out:

**1. Download-progress event listener lifecycle.** Today's parent registers the listener on parent mount and tears down on parent unmount, so the listener is alive whenever Settings is open regardless of which tab is active. After the move into `Audio.svelte`, the listener only registers when the user is *on* the Audio tab. This is fine in practice — download progress only matters when the Audio tab is rendering it — but worth calling out so the reviewer can confirm. The alternative (keep the listener in the parent and have Audio read from a shared store) is rejected as over-engineered for this use case.

**2. Modal Escape stacking (v0.10.8 fix).** The `<VocabularyDialog>` and `<ContextTemplateDialog>` capture-phase Escape handlers must continue to work after the dialogs move into `General.svelte`. The fix is component-scoped (registered inside each dialog's `onMount`), so the move is transparent — but the manual smoke test should re-verify it.

## Testing

- **No new unit tests required.** Behavior preservation is verified by the existing `npm run check` (TypeScript) and `npx vitest run` (frontend tests). The current 57 vitest tests don't directly test `SettingsContent.svelte`, so they pass trivially through the refactor.
- **Manual smoke test after each commit:**
  1. Open Settings (modal or page).
  2. Click each tab — General, Prompts, AI Models, Audio — confirm the right section renders.
  3. Open the Vocabulary and Context Template dialogs from General; press Escape; confirm only the inner dialog closes.
  4. On Audio, kick off a small Whisper-model download to verify the progress-event listener still works after its move.
- The implementer does **not** run the dev server in subagent mode — manual smoke is the user's responsibility; the implementer documents the expected smoke-test steps in commit messages.

## Rollout

- One PR per section commit + a final version-bump commit (5 commits total) on the same feature branch.
- Version bump to `0.10.11` (invisible-to-user maintenance; patch bump is sufficient).
- No DB migration, no feature flag, no new dependencies.
- `SettingsDialog.svelte` and `SettingsPage.svelte` remain unchanged — the external API of `<SettingsContent />` is preserved.

## Files touched (anticipated)

**Created:**
- `src/lib/components/settings/General.svelte`
- `src/lib/components/settings/Prompts.svelte`
- `src/lib/components/settings/Models.svelte`
- `src/lib/components/settings/Audio.svelte`

**Modified:**
- `src/lib/components/SettingsContent.svelte` — shrinks to ~80 LOC.
- `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, `Cargo.lock` — version bump to `0.10.11`.

**Unchanged:**
- `src/lib/dialogs/SettingsDialog.svelte` and `src/lib/pages/SettingsPage.svelte` — both still render `<SettingsContent />` with no API change.
- `src/lib/components/VocabularyDialog.svelte` and `src/lib/components/ContextTemplateDialog.svelte` — only their import location changes (now imported by `General.svelte` instead of `SettingsContent.svelte`).
