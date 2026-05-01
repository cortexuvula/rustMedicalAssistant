# Auto-Pipeline Carries Structured Patient Context — Design

**Date:** 2026-05-01
**Status:** Approved (ready for implementation plan)
**Sprint:** 2 / B2
**Builds on:** v0.10.6 (`docs/superpowers/specs/2026-04-30-structured-patient-context-design.md`)

## Problem

In v0.10.6 we added a structured patient-context surface (medications, allergies, known conditions) to the GenerateTab and threaded it through the manual `generate_soap` flow. The auto-pipeline path — `process_recording` Tauri command, invoked when the clinician stops recording with `auto_generate_soap` enabled — was deliberately left passing `None` for `patient_context` to keep the v0.10.6 PR focused. As a result, structured fields entered anywhere in the UI today are silently ignored when the auto-pipeline kicks off.

This release completes the v0.10.6 promise: structured patient context becomes available on the auto-pipeline path by mirroring the GenerateTab structured-context panel onto RecordTab and threading the new payload through `pipeline.launch` → `processRecording` → `process_recording` → the existing `generate_soap` invocation.

## Goals

- Clinicians can enter structured medications/allergies/conditions on RecordTab before/during a recording.
- Auto-pipeline (`auto_generate_soap = true`) and manual "Process Recording" both honor the structured fields.
- Existing freeform-context behavior is unchanged.
- Validation happens at pipeline entry, before transcription, so malformed input is rejected before any expensive work runs.
- No DB schema migration.

## Non-goals

- Auto-loading `metadata.patient_context` into RecordTab when re-opening an existing recording (RecordTab is the new-recording entry surface).
- Cross-tab in-progress state sharing (RecordTab and GenerateTab each own their unsaved state; metadata is the post-pipeline connector).
- Extending the existing context-template feature to structured fields (templates remain freeform-only).
- Per-patient profile / cross-recording carry-forward (still deferred per v0.10.6 Q1).

## Decisions

| # | Decision |
|---|---|
| Q1 | UX shape: mirror the v0.10.6 GenerateTab structured-context panel onto RecordTab. Auto-pipeline picks up whatever was typed at pipeline-launch time. |
| Q2 | Layout: structured fields go above the freeform Notes textarea, inside the existing collapsible "Patient Context" panel on RecordTab. The existing template picker / "Save as template" button remain operating on the freeform Notes only. |
| Q3 | Validation strategy: reuse `validate_patient_context` from `commands/generation.rs` at the entry of `process_recording`, before transcription. Bump the helper from `fn` to `pub(super) fn`. |
| Q4 | Persistence: unchanged from v0.10.6. `generate_soap_inner` writes `metadata.patient_context` on SOAP success. Pipeline failure does not persist context. |
| Q5 | Helper extraction: extract the existing in-line `buildPatientContext` closure from GenerateTab into `src/lib/utils/patient_context.ts`. Both RecordTab and GenerateTab call the same helper. Pure function with vitest tests. |

## Data flow

```
RecordTab.svelte
  ├─ medicationsText, allergiesText, conditionsText  (new $state)
  └─ on launch (auto-pipeline OR manual "Process Recording" button):
              │
              ▼
   buildPatientContext(meds, allergies, conditions)  (extracted shared helper)
   → PatientContext | undefined
              │
              ▼
   pipeline.launch(rid, ctx, template, patientContext)   (extended store API)
              │
              ▼
   processRecording(rid, ctx, template, patientContext)   (extended IPC wrapper)
              │
              ▼
   process_recording(...)  Tauri command   (extended with patient_context: Option<PatientContext>)
              │
              ├─ super::generation::validate_patient_context(pc)?   (caps reused; rejects pre-transcribe)
              ├─ Stage 1: transcription   (unchanged)
              └─ Stage 2: super::generation::generate_soap(... patient_context)   (was None)
                          │
                          └─ generate_soap_inner writes metadata.patient_context on success
```

## Frontend changes

### `src/lib/utils/patient_context.ts` (new)

Extracts the existing in-line `buildPatientContext` closure from GenerateTab into a shared utility:

```typescript
import type { PatientContext } from '../types';
import { splitLines } from './text';

/**
 * Build a `PatientContext` payload from three line-per-item textarea values.
 * Returns `undefined` when every list is empty so the backend stores nothing
 * and renders no Patient record block.
 */
export function buildPatientContext(
  medicationsText: string,
  allergiesText: string,
  conditionsText: string,
): PatientContext | undefined {
  const medications = splitLines(medicationsText);
  const allergies = splitLines(allergiesText);
  const conditions = splitLines(conditionsText);
  if (medications.length === 0 && allergies.length === 0 && conditions.length === 0) {
    return undefined;
  }
  return {
    patient_name: null,
    prior_soap_notes: [],
    medications,
    allergies,
    conditions,
  };
}
```

### `src/lib/utils/patient_context.test.ts` (new)

vitest spec covering: all-empty returns `undefined`; single-list populated; multi-list populated; whitespace / CRLF / blank-line normalization (delegated to `splitLines`, but verified end-to-end via the helper).

### `src/lib/pages/GenerateTab.svelte` (DRY refactor)

Replace the inline `buildPatientContext` closure with a call to the extracted helper. ~10 LOC removed.

### `src/lib/pages/RecordTab.svelte` (substantive change)

Add three textareas inside the existing `<div class="context-panel">` collapsible (currently labeled "Patient Context"), above the freeform Notes textarea:

```
[▼] Patient Context [Active]   ← new $derived "Active" badge mirroring GenerateTab
  Medications (one per line)
  ┌──────────────────────────┐
  │ Lisinopril 10mg PO daily │
  └──────────────────────────┘

  Allergies (one per line)
  ┌──────────────────────────┐
  │ Penicillin (rash)        │
  └──────────────────────────┘

  Known conditions (one per line)
  ┌──────────────────────────┐
  │ Type 2 diabetes          │
  └──────────────────────────┘

  Notes
  [existing template-picker + Save-as-template button]
  [existing freeform textarea]
```

- Three new `$state` vars: `medicationsText`, `allergiesText`, `conditionsText`.
- `hasActiveContext` derived from all four input fields (matches GenerateTab pattern).
- Existing template picker and "Save as template" remain operating on the freeform Notes only.
- The `(optional)` hint next to the toggle label is replaced by the derived `Active` badge so the indicator is consistent with GenerateTab.
- On every place that calls `pipeline.launch(...)` or `pipeline.retry(...)` (auto-pipeline launch via `handleStopRecording → maybeLaunchPipeline`, manual "Process Recording" via `handleProcessRecording`, the silent-recording confirmation, retry button), pass `buildPatientContext(medicationsText, allergiesText, conditionsText)` as the new fourth arg.

### `src/lib/stores/pipeline.ts`

`launch(recordingId, context?, template?)` becomes `launch(recordingId, context?, template?, patientContext?)`. Same for `retry(...)`. Forwards the new arg to the IPC wrapper.

### `src/lib/api/...` — `processRecording` invoke wrapper

Add `patientContext?: PatientContext` parameter; pass `patientContext: patientContext ?? null` in the invoke args (same pattern as v0.10.6 `generateSoap`).

## Backend changes

### `src-tauri/src/commands/generation.rs`

Bump `validate_patient_context` from `fn validate_patient_context(...)` to `pub(super) fn validate_patient_context(...)` so `commands/pipeline.rs` (sibling module) can reuse it. No behavior change.

### `src-tauri/src/commands/pipeline.rs`

Extend the `process_recording` Tauri command:

```rust
#[tauri::command]
pub async fn process_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    context: Option<String>,
    template: Option<String>,
    patient_context: Option<PatientContext>,   // NEW
) -> AppResult<String>
```

Validate the structured payload at the entry, before any pipeline work:

```rust
if let Some(ref pc) = patient_context {
    super::generation::validate_patient_context(pc)?;
}
```

Thread `patient_context` into the existing `generate_soap` call (which v0.10.6 Task 6 set to `None` as a placeholder):

```rust
let soap_result = super::generation::generate_soap(
    app.clone(),
    state.clone(),
    recording_id.clone(),
    template,
    context,
    patient_context,   // was None
)
.await;
```

Imports added:

```rust
use medical_core::types::PatientContext;
```

## Persistence

Unchanged from v0.10.6. `generate_soap_inner` writes `metadata.patient_context` on SOAP success. Pipeline failure does not persist context.

## Error handling

| Scenario | Behavior |
|---|---|
| `patient_context` exceeds caps (50 items / 500 chars / `MAX_CONTEXT_CHARS` total) | `process_recording` returns `AppError::Other(...)` from the early `validate_patient_context` call — before `progress: "transcribing"` is emitted. Same shape as the existing freeform-context overflow rejection. |
| All-empty `PatientContext` | `RecordTab.buildPatientContext()` returns `undefined` → IPC sends `null` → Rust sees `None` → no validation, no Patient record block in SOAP prompt, no metadata write. |
| Pipeline failure (transcription / SOAP gen) after structured context was supplied | `metadata.patient_context` is **not** persisted on failure. User can retry via `pipeline.retry(...)` with the same context still in RecordTab state. |
| User navigates away from RecordTab pre-pipeline | In-progress structured fields are lost (each tab owns its state; no shared store). Matches freeform behavior. |

## Testing

### Frontend (vitest)

- **`src/lib/utils/patient_context.test.ts` (new)** — covers the extracted helper:
  - All three textareas empty → `undefined`
  - Single list populated → returns object with that list, others empty
  - Mixed populated → all three lists carry through
  - Whitespace / CRLF normalization end-to-end via `splitLines`
- Existing 51 vitest tests still pass; total bumps to ~55.

### Backend (Rust)

- **`commands/pipeline.rs` unit test:** `process_recording_rejects_oversized_patient_context` — constructs a `PatientContext` with one 501-char medication item and verifies the call site invokes `validate_patient_context` and surfaces its `Err`. The validator's own behavior is already covered by the 5 tests added in v0.10.6 Task 5.
- Existing 480 backend tests must continue to pass; total bumps to 481.

### Manual verification (golden path on dev server)

1. Start the Tauri dev server, open RecordTab.
2. Expand "Patient Context" panel; type 2 medications (one per line), 1 allergy, 1 condition. Verify the "Active" badge lights up.
3. Start recording, speak briefly, stop. With `auto_generate_soap = true`, the auto-pipeline kicks off.
4. After completion, verify the SOAP note's "Current medications", "Allergies", and "Past medical history" sections reflect the structured input.
5. Switch to GenerateTab for the same recording — verify the structured fields are populated from `metadata.patient_context` (round-trip).
6. Repeat with `auto_generate_soap = false`, click "Process Recording" — same outcome.
7. Paste a 501-char med into Medications, click "Process Recording" — verify a clear error toast appears and transcription does not start.

## Rollout

- One PR. No feature flag — additive change with new optional parameter, new metadata population in an existing key.
- No DB migration; `metadata.patient_context` already exists from v0.10.6.
- Existing recordings unaffected.
- Version bump to `0.10.10`.

## Files touched (anticipated)

- `src/lib/utils/patient_context.ts` (new) — extracted `buildPatientContext` helper.
- `src/lib/utils/patient_context.test.ts` (new) — vitest spec.
- `src/lib/pages/GenerateTab.svelte` — DRY refactor to use the extracted helper.
- `src/lib/pages/RecordTab.svelte` — three structured textareas, `hasActiveContext` derived, `buildPatientContext()` call at pipeline-launch sites.
- `src/lib/stores/pipeline.ts` — `launch()` / `retry()` gain `patientContext` parameter.
- `src/lib/api/pipeline.ts` — `processRecording` IPC wrapper gains `patientContext` parameter.
- `src-tauri/src/commands/generation.rs` — `validate_patient_context` bumped to `pub(super)`.
- `src-tauri/src/commands/pipeline.rs` — `process_recording` gains `patient_context` parameter; validates at entry; threads to `generate_soap`.
- `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`, `Cargo.lock` — version bump to `0.10.10`.
