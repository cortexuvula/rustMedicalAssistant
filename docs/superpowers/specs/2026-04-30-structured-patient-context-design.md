# Structured Patient Context for SOAP Generation — Design

**Date:** 2026-04-30
**Status:** Approved (ready for implementation plan)
**Related fix:** prompt loosening for background-sourced facts (already shipped — see `crates/processing/src/soap_generator.rs`).

## Problem

When a clinician enters background information (medications, allergies, known conditions) into the freeform "Additional Context" textarea on the Generate tab, those facts often fail to appear in the resulting SOAP note. The proximate cause was contradictory rules in the SOAP system prompt; that has been fixed. The deeper issue is that the freeform textarea offers no signal to the model that the contents are physician-supplied ground truth versus narrative scratchpad — and no structured representation in storage we can re-use elsewhere.

This spec adds an opt-in structured-context surface on top of the existing freeform textarea: a small set of typed fields the clinician can fill in, persisted on the recording, and rendered into the SOAP prompt as a separately-labeled "authoritative" block.

## Goals

- Clinicians can supply medications, allergies, and known conditions as structured per-recording data.
- The SOAP prompt clearly distinguishes physician-supplied authoritative facts from freeform narrative.
- Existing recordings and the existing freeform textarea continue to work unchanged.
- No DB schema migration required.

## Non-goals

- Per-patient profile / cross-recording carry-forward (Q1 chose per-recording only).
- Surgical / family / social history as structured fields (Q2 chose only meds/allergies/conditions).
- Per-medication structured sub-fields like name/dose/frequency/route (Q3 chose plain strings).
- Auto-parsing existing freeform notes into structured fields.
- Removing the now-redundant "Medications" template chip from the freeform area.

## Decisions

| # | Decision |
|---|---|
| Q1 | Scope: per-recording only — no `patients` table, no carry-forward across recordings. |
| Q2 | Fields: medications, allergies, conditions only. |
| Q3 | Granularity: each list item is a plain string (e.g. `"Lisinopril 10mg PO daily"`). |
| Q4 | UX layout: structured fields live inside the existing "Additional Context" collapsible, above the freeform textarea. |
| Q5 | Field rendering: one textarea per category, one item per line. |
| Q6 | Prompt rendering: structured data becomes a separate "Patient record" block; freeform notes remain in the existing "Supplementary background" block. |
| Q7 | Storage: inside the existing `recordings.metadata` JSON column, reusing the existing `medical_core::types::PatientContext` struct. |
| Q8 | IPC: add `patient_context: Option<PatientContext>` parameter to the `generate_soap` Tauri command, mirroring the existing freeform `context` parameter flow. |

## Data model

A new optional field on `recordings.metadata`, reusing `PatientContext` from `crates/core/src/types/agent.rs:52`:

```jsonc
// recording.metadata
{
  "context": "freeform notes string (existing, unchanged)",
  "patient_context": {           // NEW
    "patient_name": null,        // unused for this feature; kept for struct fidelity
    "prior_soap_notes": [],      // unused for this feature; kept for struct fidelity
    "medications": ["Lisinopril 10mg PO daily", "Metformin 500mg PO BID"],
    "conditions": ["Type 2 diabetes", "Hypertension"],
    "allergies": ["Penicillin (rash)"]
  }
}
```

- No DB migration. The `metadata` column already stores arbitrary JSON.
- Old recordings without `metadata.patient_context` render with empty list inputs in the UI.
- Existing freeform `metadata.context` is **not** parsed into structured fields.
- `PatientContext`'s unused fields (`patient_name`, `prior_soap_notes`) stay null/empty; they aren't surfaced in the UI but keep the struct shape stable for the agent orchestrator, which already consumes the same struct.
- `PatientContext` fields gain `#[serde(default)]` so the frontend can omit `patient_name` and `prior_soap_notes` from the payload it sends; this also keeps deserialization forward-compatible if fields are added later.

## Backend — IPC and prompt

### `generate_soap` command

`src-tauri/src/commands/generation.rs`:

```rust
pub async fn generate_soap(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    recording_id: String,
    template: Option<String>,
    context: Option<String>,                    // existing freeform
    patient_context: Option<PatientContext>,    // NEW
) -> AppResult<String>
```

### Validation

Mirrors the existing freeform-context bounds, applied **before** emitting the `started` progress event:

- Total characters across all items in the structured payload capped at the same `MAX_CONTEXT_CHARS` constant used today for freeform context (defined in `src-tauri/src/commands/generation.rs`).
- Each list capped at 50 items.
- Single item capped at 500 characters.
- Cap violations return `AppError::Other` with a clear message — same shape as today's freeform-context overflow.

The 50-item and 500-character per-item caps are deliberately generous against realistic clinical entries and exist to reject pathological input, not to constrain normal use. Freeform context is *truncated* on overflow because narrative content is expected to be long; structured items are *rejected* because a 500-char single med entry indicates a malformed input rather than legitimate data.

### Prompt rendering

`crates/processing/src/soap_generator.rs::build_user_prompt` gains a third argument: `patient_context: Option<&PatientContext>`. When present and non-empty, it is rendered as a **separate authoritative block** that comes after the transcript and before the freeform "Supplementary background" block:

```
Patient record (physician-supplied authoritative facts — use these to populate
historical Subjective fields. Treat as ground truth for medications, allergies,
and known conditions; never let them alter today's Objective findings,
Assessment, or Plan):
- Medications:
  - Lisinopril 10mg PO daily
  - Metformin 500mg PO BID
- Allergies:
  - Penicillin (rash)
- Known conditions:
  - Type 2 diabetes
  - Hypertension

Supplementary background (use ONLY to add context to what was discussed in
the transcript above — do NOT let this override or substitute for transcript
content):
[freeform notes here]
```

### System-prompt update

`default_soap_prompt` (`crates/processing/src/soap_generator.rs`) gains one sentence after Rule 4 explaining that "Patient record" entries are authoritative for the historical Subjective fields they populate, and that the existing rule about not altering today's Assessment/Plan still applies. The fabrication-discipline rules and the recently-loosened background sourcing rules are otherwise unchanged.

### Persistence

After a successful generation, `generate_soap_inner` writes `metadata.patient_context` alongside `metadata.context` (today's behavior). An all-empty `PatientContext` (every list empty, no `patient_name`, no `prior_soap_notes`) is treated as `None` and not persisted.

### Sanitization

Each list entry runs through the existing `sanitize_prompt` helper to strip injection patterns (script tags, "ignore previous instructions", etc.). We do **not** truncate items — we reject the whole payload when caps are exceeded.

## Frontend — UI and data flow

### Type definitions

`src/lib/types/index.ts`:

```ts
export interface PatientContext {
  patient_name: string | null;
  prior_soap_notes: string[];
  medications: string[];
  conditions: string[];
  allergies: string[];
}

// Recording.metadata is typed-narrowing-friendly:
//   metadata?.patient_context?: PatientContext
```

### API layer

`src/lib/api/generation.ts`:

```ts
export async function generateSoap(
  recordingId: string,
  template?: string,
  context?: string,
  patientContext?: PatientContext,   // NEW
): Promise<string>
```

### `GenerateTab.svelte`

Inside the existing collapsible "Additional Context" panel:

```
[▾] Additional Context [Active]
  ─────────────────────────────────
  Medications (one per line)
  ┌────────────────────────────┐
  │ Lisinopril 10mg PO daily   │
  │ Metformin 500mg PO BID     │
  └────────────────────────────┘

  Allergies (one per line)
  ┌────────────────────────────┐
  │ Penicillin (rash)          │
  └────────────────────────────┘

  Known conditions (one per line)
  ┌────────────────────────────┐
  │ Type 2 diabetes            │
  │ Hypertension               │
  └────────────────────────────┘

  Notes
  [existing freeform textarea + template chips]
  [Clear notes]
```

**Behavior:**

- The "Active" badge becomes a `$derived` state that's true if **any** of medications / allergies / conditions / freeform notes are non-empty.
- The existing `$effect` that loads from `selectedRecording` extends to also load `metadata.patient_context` into the three list strings (joined with `\n` for display in their textareas). The existing `lastContextRecordingId` guard already gates reloads to recording-id changes only; the new structured fields load on the same boundary.
- On generate: parse each textarea by line — trim whitespace, drop empty lines. Build a `PatientContext` object (`patient_name: null`, `prior_soap_notes: []`, the three lists). If all three lists are empty, pass `undefined` for `patientContext`. Pass freeform `context` as today.
- The line-parser (`splitLines(text: string): string[]` — trim each, drop empties, normalize `\r\n`) is extracted to `src/lib/utils/text.ts` so it can be unit-tested in isolation from the Svelte component.
- The five existing template chips (Follow-up, New Patient, Lab Results, Medications, Referral Info) still drop their canned text into the **freeform Notes** textarea, not the structured fields. The Medications chip becomes slightly redundant but remains in place for now.

### Loading existing recordings

Old recordings without `metadata.patient_context` show empty list inputs; their existing freeform notes still load into the Notes textarea exactly as today.

## Error handling

| Scenario | Behavior |
|---|---|
| `patient_context` totals exceed cap | `generate_soap` returns `AppError::Other("Patient context too large: …")` *before* emitting `started`, same shape as today's freeform-context overflow. |
| Any list has more than 50 items | Rejected with a similar bounded error. |
| Single item > 500 chars | Rejected with `"Patient context entry too long: …"`. |
| All-empty `PatientContext` arrives at backend | Treated as `None`; no "Patient record" block in prompt; not persisted to metadata. |
| Old recording without `metadata.patient_context` | Frontend renders empty inputs; backend treats as `None`. |
| Frontend sends invalid struct shape | Tauri/serde returns a deserialization error to the frontend; surfaced via existing `formatError` path. |
| Generation fails after structured data was sent | `metadata.patient_context` is **not** persisted on failure (same as today's freeform `context`); user re-edits and retries. |

## Testing

### Unit tests

`crates/processing/src/soap_generator.rs`:

- `build_user_prompt_includes_patient_record_block_when_provided` — verifies the new block renders with all three lists.
- `build_user_prompt_omits_patient_record_when_all_empty` — verifies an all-empty `PatientContext` produces no block.
- `patient_record_block_appears_before_supplementary_background` — verifies ordering.
- `default_soap_prompt_treats_patient_record_as_authoritative` — verifies the system prompt has the new sentence about the Patient record block.
- Sanitization: items containing injection patterns are stripped per existing `sanitize_prompt` rules.

`src-tauri/src/commands/generation.rs`:

- Bounds checks reject oversized payloads / long items / overlong lists before emitting `started`.
- Successful generation persists `metadata.patient_context` alongside `metadata.context`.
- All-empty `PatientContext` does **not** create a `metadata.patient_context` key.

### Integration test

End-to-end test that mocks the AI provider and asserts the rendered prompt body contains the structured block when `patient_context` is supplied.

### Frontend tests

vitest tests for the extracted `splitLines` helper (trim / drop-empty / mixed `\r\n` round-trip) and a small assembly test that builds a `PatientContext` payload from three textareas and asserts an all-empty set yields `undefined` rather than an object with three empty arrays.

### Manual verification (golden path)

1. Start the Tauri dev server, open an existing recording with a transcript.
2. In the Generate tab, expand Additional Context, paste a 3-line med list, 1 allergy, 2 conditions; leave notes empty.
3. Click Generate SOAP. Verify the resulting note's "Current medications" / "Allergies" / "Past medical history" reflect the structured input.
4. Re-open the recording — verify the structured fields round-trip from `metadata.patient_context`.
5. Repeat with notes also non-empty — verify both blocks reach the model and the freeform content lands appropriately (PMH narrative, etc.).

## Rollout

- One PR. No feature flag — additive change with no breaking surface (new optional parameter, new metadata key).
- No DB migration; the `metadata` column already accepts the new shape.
- Existing recordings continue to work; their old `metadata.context` is unchanged.
- Bump version to `0.10.6` in `Cargo.toml` / `package.json` per existing convention.

## Files touched (anticipated)

- `crates/processing/src/soap_generator.rs` — extend `build_user_prompt` signature; add "Patient record" block rendering; tweak `default_soap_prompt`; add tests.
- `src-tauri/src/commands/generation.rs` — extend `generate_soap` signature, add validation, persist `patient_context`; add tests.
- `src/lib/api/generation.ts` — extend `generateSoap` to forward `patient_context`.
- `src/lib/types/index.ts` — `PatientContext` interface; refine `Recording.metadata` typing.
- `src/lib/pages/GenerateTab.svelte` — three list textareas inside the Additional Context panel, load/save logic, derived "Active" badge, payload assembly.
- `src/lib/utils/text.ts` (new) — extracted `splitLines` helper plus its vitest spec.
- `crates/core/src/types/agent.rs` — add `#[serde(default)]` on `PatientContext` fields so the frontend may omit `patient_name` and `prior_soap_notes`.
- `Cargo.toml` / `package.json` — version bump to `0.10.6`.
