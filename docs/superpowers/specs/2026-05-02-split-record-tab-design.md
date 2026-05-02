# Split `RecordTab.svelte` Into Per-Region Components — Design

**Date:** 2026-05-02
**Status:** Approved (ready for implementation plan)
**Sprint:** 2 / B4
**Sibling:** `2026-05-01-split-settings-content-design.md` (B3, shipped as `v0.10.11`)

## Problem

`src/lib/pages/RecordTab.svelte` is 964 LOC and orchestrates the entire recording flow: gathering patient context, kicking off the pipeline, displaying pipeline progress, and showing recording state messages. It mixes ~294 LOC of state and handlers with ~278 LOC of markup across multiple regions, plus ~390 LOC of scoped CSS. Although smaller than the 1,717-LOC `SettingsContent.svelte` we split in B3, it is the largest remaining single-file component in the project and is repeatedly flagged in audits as a maintenance bottleneck.

This release decomposes the file into three per-region child components plus a thin parent that owns the recording lifecycle and orchestration. Behavior is preserved exactly; the change is invisible to users.

## Goals

- Each visually distinct region of `RecordTab.svelte` lives in its own focused file with its own state, handlers, markup, and scoped CSS.
- The parent shrinks to roughly 280 LOC of orchestration: state ownership, recording-lifecycle handlers, composition.
- No regression: every recording interaction works identically after the split.
- Incremental commits: one component per commit so any regression is bisectable.
- No new dependencies, no DB migration, no feature flag, no backend changes.

## Non-goals

- Refactoring `RecordingHeader.svelte` (already a separate component).
- Refactoring the `ConfirmDialog` component or its reuse pattern.
- Adding new functionality to the recording flow.
- Renaming any state vars, handlers, or CSS classes.
- Splitting beyond three children — the existing internal cohesion of the recording flow doesn't justify finer granularity.

## Decisions

| # | Decision |
|---|---|
| Q1 | Granularity: three children — `PatientContextPanel`, `PipelineStatus`, `RecordingStateCards` — plus a thin orchestrating parent. |
| Q2 | Data flow: Svelte 5 `$bindable` for state the parent must access (context fields, copyStatus); callback props for actions the parent owns (`onCancel`, `onRetry`, `onCopySoap`, `onSpeedRead`, `onProcessRecording`, `onUploadAudio`). |
| Q3 | File layout: new `src/lib/pages/record/` subdirectory next to the parent (which lives in `pages/`). Mirrors B3's pattern of co-locating sub-components with their parent. |
| Q4 | Commit cadence: smallest-first (RecordingStateCards → PipelineStatus → PatientContextPanel → version bump). 4 commits total. |
| Q5 | Silence `<ConfirmDialog>` stays in parent. It's tightly coupled to recording-lifecycle handlers (`confirmSilentProcess`, `dismissSilenceDialog`) which live in parent. |

## Target architecture

```
src/lib/pages/
├── RecordTab.svelte                   (~280 LOC after split)
└── record/                             (new subdirectory)
    ├── PatientContextPanel.svelte     (~200 LOC: panel + save-template modal)
    ├── PipelineStatus.svelte          (~150 LOC: stage display + post-action buttons)
    └── RecordingStateCards.svelte     (~100 LOC: idle/recording/paused/stopped/imported messages)
```

**Parent retains:**

- All state declarations: context state (`contextText`, `medicationsText`, `allergiesText`, `conditionsText`, `contextCollapsed`), pipeline state (`pipelineRecordingId`, `silenceDialogOpen`, `silenceDialogRecordingId`, `silenceDialogMessage`), import state (`importedRecordingId`, `importedFilename`, `importing`, `importError`), copy state (`copyStatus`).
- Recording-lifecycle handlers: `handleStartRecording`, `handleStopRecording`, `handleProcessRecording`, `handleRetry`, `handleCancelPipeline`, `handleUploadAudio`, `handleCopySoap`, `handleSpeedRead`.
- Pipeline-launch helpers: `maybeLaunchPipeline`, `warnIfSilent`, `describeSilence`, `confirmSilentProcess`, `dismissSilenceDialog`.
- `<RecordingHeader>` and the silence `<ConfirmDialog>` renders.
- Composition: imports the three new children and renders them in the right places.

**Each child:**

- `$bindable` props for state it mutates (parent's state propagates through `bind:`).
- Callback props for actions implementing the parent's existing handlers.
- Owns its display-only logic (e.g., `stageLabel`, `formatPipelineElapsed`, `nowMs` ticker live in `PipelineStatus`).
- Owns its markup + scoped CSS.

## Per-component prop contracts

### `PatientContextPanel.svelte`

```svelte
<script lang="ts">
  let {
    contextText = $bindable(''),
    medicationsText = $bindable(''),
    allergiesText = $bindable(''),
    conditionsText = $bindable(''),
    contextCollapsed = $bindable(true),
  }: Props = $props();
</script>
```

**Internal to this child** (move from parent):

- State: `selectedTemplate`, `saveModalOpen`, `saveModalName`, `saveModalError`, `saveModalOverwriteConfirm`.
- Derived: `hasActiveContext` (pure function over the five bindable vars).
- Handlers: `applyTemplate`, `openSaveModal`, `closeSaveModal`, `confirmSaveTemplate`.
- Reads stores: `$contextTemplates`.
- Markup: the `<div class="context-panel">` block plus the save-template modal overlay.

### `PipelineStatus.svelte`

```svelte
<script lang="ts">
  let {
    pipelineRecordingId,
    copyStatus = $bindable<'idle' | 'copying' | 'copied'>('idle'),
    onCancel,
    onRetry,
    onCopySoap,
    onSpeedRead,
  }: Props = $props();
</script>
```

**Internal to this child** (move from parent):

- State: `nowMs` (live elapsed-time ticker).
- `$effect` ticking `nowMs` once per second while a pipeline is in flight.
- Pure helpers: `stageLabel`, `formatPipelineElapsed`.
- Reads stores: `$pipeline`.
- Markup: the `<div class="pipeline-status">` block — stage row, elapsed line, post-action buttons (cancel / copy / speed-read / retry).

### `RecordingStateCards.svelte`

```svelte
<script lang="ts">
  let {
    importedRecordingId,
    importedFilename,
    importing,
    importError,
    onProcessRecording,
    onUploadAudio,
  }: Props = $props();
</script>
```

**Internal to this child:** none — pure presentation.

**Reads stores:** `$audio`, `$settings`.

**Markup:** the four `{:else if}` branches that render state-message cards (imported / idle / recording / paused / stopped).

## Composition in the parent

The parent's main content area becomes:

```svelte
<PatientContextPanel
  bind:contextText
  bind:medicationsText
  bind:allergiesText
  bind:conditionsText
  bind:contextCollapsed
/>
<RecordingHeader
  onStart={handleStartRecording}
  onStop={handleStopRecording}
/>
<div class="record-content">
  {#if $pipeline.current && pipelineRecordingId}
    <PipelineStatus
      {pipelineRecordingId}
      bind:copyStatus
      onCancel={handleCancelPipeline}
      onRetry={handleRetry}
      onCopySoap={handleCopySoap}
      onSpeedRead={handleSpeedRead}
    />
  {:else}
    <RecordingStateCards
      {importedRecordingId}
      {importedFilename}
      {importing}
      {importError}
      onProcessRecording={handleProcessRecording}
      onUploadAudio={handleUploadAudio}
    />
  {/if}
</div>
<ConfirmDialog
  open={silenceDialogOpen}
  title="Silent recording detected"
  message={silenceDialogMessage}
  confirmLabel="Process anyway"
  cancelLabel="Cancel"
  danger
  onConfirm={confirmSilentProcess}
  onCancel={dismissSilenceDialog}
/>
```

## CSS migration strategy

Same rule as B3: scoped CSS rules whose selectors target classes used ONLY in this child move with the child; shared rules (e.g., `.btn-primary`, `.btn-secondary`, `.spinner`, `.error-text`, `.field-label`) stay in the parent. When a rule's section ownership is genuinely unclear, leave it in the parent with a `/* TODO: classify */` comment and let the reviewer decide.

After all three extractions, the parent's `<style>` block should contain only:

- Page-level layout (`.record-tab`, `.record-content`).
- Genuinely-shared form-control or button styles used by multiple children.

## Behavior preservation — explicit invariants

- **`bind:` semantics.** `$bindable` props with `bind:` propagate writes both ways. Net behavior is identical to today's "all state in one component."
- **`$effect` lifecycle.** The `nowMs` ticker `$effect` moves into `PipelineStatus.svelte`. It fires only when the component is mounted (i.e., when `$pipeline.current && pipelineRecordingId`). Today's effect runs whenever the parent is mounted but its body is gated on `$pipeline.current`. Net behavior: identical.
- **Save Template modal scope.** The modal currently renders at the parent's bottom. After the move it renders inside `PatientContextPanel.svelte`. The modal already uses an overlay class with `position: fixed` — positioning is viewport-relative regardless of which component renders it.
- **Silence ConfirmDialog stays in parent** so the recording-lifecycle handlers it depends on (`confirmSilentProcess`, `dismissSilenceDialog`) don't need to be plumbed through children.

## Testing

- **No new unit tests required.** Behavior preservation is verified by `npm run check` (TypeScript) and `npx vitest run` (frontend tests). Current 57 vitest tests don't directly test `RecordTab.svelte`, so they pass trivially through the refactor.
- **Manual smoke test after each commit:**
  1. Start a recording. Verify the panel, pipeline status, and state cards all render and behave identically to before.
  2. After the final commit, also verify: silence dialog appears for a quiet recording, "Save as template" opens its modal, copy-SOAP cycle works (idle → copying → copied → idle).
- The implementer does **not** run the dev server in subagent mode — manual smoke is the user's responsibility post-merge; the implementer documents expected smoke-test steps in commit messages.

## Rollout

- 4 commits on one feature branch.
- Version bump to `0.10.12` (invisible-to-user maintenance; patch bump is sufficient).
- No DB migration, no feature flag, no new dependencies, no backend changes.
- External consumers of `RecordTab` (the routing layer in `App.svelte`) untouched — the page's external API is preserved.

## Files touched (anticipated)

**Created:**

- `src/lib/pages/record/RecordingStateCards.svelte`
- `src/lib/pages/record/PipelineStatus.svelte`
- `src/lib/pages/record/PatientContextPanel.svelte`

**Modified:**

- `src/lib/pages/RecordTab.svelte` — shrinks from 964 LOC to ~280 LOC.
- `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, `Cargo.lock` — version bump to `0.10.12`.

**Unchanged:**

- `src/lib/components/RecordingHeader.svelte`, `src/lib/components/ConfirmDialog.svelte` — both still imported by the parent with no API change.
- `App.svelte` and other routing-layer consumers of `RecordTab`.
