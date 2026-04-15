# Streamlined Recording-to-SOAP Pipeline

**Date:** 2026-04-15
**Status:** Approved

## Problem

The current workflow from recording audio to producing a SOAP note requires 7 manual steps across 4 tabs: stop recording, click transcribe, wait, navigate to recordings tab, select recording, navigate to generate tab, click generate SOAP. This is too cumbersome for a clinical workflow where the physician may have multiple patients in quick succession.

## Design

### Overview

Turn the Record tab into the primary workspace by adding a context panel and a background processing pipeline. After stopping a recording, transcription and SOAP generation run automatically in the background (when enabled), allowing the user to immediately start recording the next patient. A toast notification appears when the SOAP note is ready.

The existing tabs (Recordings, Generate, Editor) remain unchanged as the detailed/full-control workflow.

### Record Tab Layout

Three zones stacked vertically:

**1. Context Panel (top, collapsible)**
- Text area labeled "Patient Context (optional)" with placeholder: "Paste chart notes, medications, history..."
- Collapsible via toggle so it doesn't dominate when not needed
- Content persists per recording: when a recording stops, context is snapshot and saved to that recording's metadata. When a new recording starts, the context area clears.
- Editable before, during, and after recording

**2. Recording Controls (middle, unchanged)**
- Start/Stop/Pause/Cancel buttons
- Waveform visualization
- Timer

**3. Pipeline Status (bottom, replaces current post-recording UI)**
- After recording stops, shows a compact progress indicator with stages: Transcribing → Generating SOAP → Done
- Each stage shows a spinner while active, checkmark when complete
- When SOAP is done, shows a "View SOAP Note" link and a "Copy" button inline
- If auto-generate is OFF, shows a "Process Recording" button instead of auto-starting
- Error states shown inline with retry option

### Background Pipeline

**New backend command: `process_recording`**

A single Tauri command that chains transcription and SOAP generation:

```
process_recording(recording_id, context?, template?) -> Result<String, String>
```

Internally:
1. Transcribe the recording (reuses existing `transcribe_recording` logic)
2. Generate SOAP note from the transcript (reuses existing `generate_soap` logic)
3. Return the SOAP note text

Emits progress events with recording ID so the frontend can track multiple pipelines:
```
pipeline-progress { recording_id, stage: "transcribing" | "generating_soap" | "completed" | "failed", error? }
```

**Concurrency:**
- Pipeline runs as an async task, non-blocking
- Multiple pipelines can run concurrently (one per recording)
- Each pipeline is isolated, identified by recording ID
- Starting a new recording does not interfere with running pipelines

**Context binding:**
- When the pipeline starts, it snapshots the current context text area content and binds it to that recording
- The context area clears when a new recording starts (not when the previous one stops), so you can still add/edit context in the gap between stopping and starting the next recording
- With auto-generate ON, context is snapshot at recording stop time — paste context before or during recording for best results
- With auto-generate OFF, context can be edited freely after recording before manually triggering the pipeline

### Notifications

**Toast notifications:**
- Pipeline completion: "SOAP note ready for [patient name / filename]" with a "View" button that navigates to the SOAP editor tab with that recording selected
- Auto-dismisses after 8 seconds, manually dismissable
- Pipeline failure: error toast persists until dismissed, includes "Retry" button

**Recording badges (unchanged):**
- T/S/R badges on RecordingsTab update naturally as the pipeline writes data

**Pipeline status on Record tab:**
- Shows status of the most recent recording's pipeline only
- Older/concurrent pipelines viewable via Recordings tab processing status

### Settings

**New setting: `auto_generate_soap: bool`**
- Toggle in Settings → Audio / STT section: "Auto-generate SOAP after recording"
- Default: OFF (opt-in)
- When ON: pipeline auto-starts on recording stop
- When OFF: Record tab shows "Process Recording" button after stopping

**SOAP template:**
- Pipeline uses whatever template is set in existing settings (default: follow-up)
- To use a different template for a specific recording, use the Generate tab to regenerate after auto-generation completes

### User Workflow (Common Case)

**With auto-generate ON (2-3 actions):**
1. (Optional) Paste context into context panel
2. Record → Stop
3. Pipeline runs in background; start next recording immediately if needed
4. Toast appears → Click "View" → Copy SOAP

**With auto-generate OFF (3-4 actions):**
1. (Optional) Paste context into context panel
2. Record → Stop
3. Click "Process Recording"
4. Toast appears → Click "View" → Copy SOAP

### What Doesn't Change

- Recordings tab: browse and select recordings, view badges
- Generate tab: full control over template, context, regeneration, referral/letter generation
- Editor tabs: view and copy documents
- All existing backend commands remain unchanged; `process_recording` composes them internally
