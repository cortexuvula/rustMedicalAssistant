# RSVP Speed Reader

## Goal

Port the Rapid Serial Visual Presentation (RSVP) speed-reading feature from the legacy Python medical assistant to the Rust/Tauri/Svelte rewrite so clinicians can quickly review generated documents word-by-word with Optimal Recognition Point (ORP) highlighting.

Reference implementation: `/Users/cortexuvula/Development/Medical-Assistant/src/ui/dialogs/rsvp_dialog.py`, `rsvp_section_picker.py`.

## Scope

### In

- **SOAP reader** — launched from the SOAP view on RecordTab / EditorTab / GenerateTab. Routes through a section picker that detects Subjective / Objective / Assessment / Plan / Differential Diagnosis / Follow Up / Clinical Synopsis and lets the user read just the sections they want.
- **Generic reader** for the Referral Letter, Patient Letter, and Clinical Synopsis views. Straight top-to-bottom read with no section picker.
- **Keyboard launch** shortcut `Cmd/Ctrl+Shift+R` when a readable document is in view.
- Settings persistence: WPM, font size, chunk size, theme, show-context, audio-cue, auto-start, remembered sections.

### Out

- Transcript RSVP (explicitly excluded — speaker labels break the word stream).
- Standalone paste / text-file / PDF / OCR input.
- Cross-session resume-where-you-left-off.

## Architecture

Almost entirely a frontend feature. No new Tauri commands; the Rust side only gains new fields on `AppConfig` for persistence.

```
┌────────────────────────────────────────────────────────────┐
│  RecordTab / EditorTab / GenerateTab  ("Speed Read" btn)   │
│  Global keybinding handler           (Cmd/Ctrl+Shift+R)    │
└──────────────────────┬─────────────────────────────────────┘
                       │ rsvp.openSoap(text) | openGeneric(text, kind)
                       ▼
      ┌────────────────────────────────────────┐
      │  src/lib/stores/rsvp.ts                │
      │  (pickerOpen, readerOpen, text, kind)  │
      └────┬───────────────────────────┬───────┘
           │ kind === 'soap' && sections│ else
           ▼                           ▼
 ┌─────────────────────────┐   ┌────────────────────────┐
 │ RsvpSectionPicker.svelte│   │  RsvpReader.svelte     │
 │  checkboxes + start btn │   │  word stage + controls │
 └────────────┬────────────┘   └────────────────────────┘
              │ start
              ▼
    RsvpReader.svelte (with joined selected text)
```

Both components mount once at the `App.svelte` level.

## Components

### `src/lib/rsvp/engine.ts` — pure logic

All stateless, all Vitest-testable. Mirror of the Python behavior that the legacy unit tests already assert.

| Function | Signature | Behavior |
|---|---|---|
| `preprocessSoap` | `(text: string) => string` | Strips `ICD-10: X00.0`, `ICD-9: 000.0` style codes, "Not discussed" lines, leading bullets/dashes on each line. |
| `detectSections` | `(text: string) => Section[]` | Case-insensitive scan for the seven SOAP headers. Returns `{name, startWordIndex, endWordIndex, wordCount}`. Returns `[]` if none found. |
| `tokenize` | `(text: string) => Token[]` | Splits into `{word, kind: 'word' \| 'clause' \| 'sentence' \| 'header'}`. `clause` = trailing `,;:`; `sentence` = trailing `.!?`; `header` = the word ends with `:` and matches a section name (case-insensitive). |
| `orpIndex` | `(word: string) => number` | Legacy rule, after stripping trailing punctuation: length 1–3 → 0, 4–5 → 1, 6–9 → 2, 10+ → 3. |
| `delayMs` | `(token: Token, baseMs: number) => number` | `baseMs × {word:1.0, clause:1.5, sentence:2.5, header:3.0}`. |
| `baseDelayMs` | `(wpm: number) => number` | `60_000 / wpm`. |

### `src/lib/stores/rsvp.ts` — orchestration

Svelte store exposing:

```ts
type DocKind = 'soap' | 'referral' | 'letter' | 'synopsis';

interface RsvpState {
  picker: { open: boolean; text: string; sections: Section[] };
  reader: { open: boolean; text: string; kind: DocKind };
}

openSoap(text: string): void    // may route through picker
openGeneric(text: string, kind: DocKind): void
startReading(text: string, kind: DocKind): void   // called by picker
closeAll(): void
```

`openSoap` calls `preprocessSoap`, then `detectSections`. If sections are found, opens the picker. If not, calls `startReading` directly. Early-returns with a `toasts.error("Nothing to read.")` on empty/whitespace-only input.

### `RsvpReader.svelte` — main dialog

Modal overlay (reuses the existing `Modal` pattern but with a larger `max-width` and `max-height` for the word stage). Three stacked regions:

1. **Context line** — muted, monospace, `max-width: 80ch`, shows 200 chars of sentence around the current word with ellipses on both sides when truncated. Only rendered when `settings.rsvp_show_context` is true.
2. **Word stage** — flex-centered. Font size drives everything. Three `<span>`s: `pre`, `orp`, `post`. ORP span colored `var(--danger, #ef4444)` in dark theme, `#E53935` in light. A dashed guide line drawn via `::before` pseudo-element, aligned to the ORP span's left edge.
3. **Controls** — two rows.
   - Top: play/pause, prev/next word, home/end, section-jump buttons (rendered only when SOAP sections exist).
   - Bottom: WPM slider (50–2000, step 25), font slider (24–96), chunk size 1/2/3 toggle group, theme toggle, context toggle, audio-cue toggle, fullscreen, close.

Below the controls: progress bar (percent), `X / Y words`, ETA `MM:SS` computed from `(remaining * 60000) / wpm / 1000`.

**Keyboard map** (matching Python):

| Key | Action |
|---|---|
| `Space` | play/pause |
| `←` / `→` | previous / next word (respects chunk size) |
| `Home` | restart from index 0, reset elapsed-time stats |
| `End` | skip to last word |
| `↑` / `↓` | WPM ± 25 (clamped 50–2000) |
| `1` / `2` / `3` | chunk size |
| `T` | toggle dark theme |
| `F11` | toggle fullscreen |
| `Esc` | exit fullscreen first, then close on second press |

**Tick loop:** `setTimeout` recomputed each step. The timer is cleared on pause, fullscreen toggle, and close; rescheduled on resume. The timer does not run while the dialog is closed.

**Auto-start:** when `rsvp_auto_start` is true, schedule the first tick 500 ms after mount.

**Audio cue:** when enabled, a 800 Hz / 100 ms beep via Web Audio `AudioContext` fires when the reader crosses a section boundary while playing.

**Chunk mode rendering:** when `chunk_size > 1`, the stage shows N words with a single space between. Only the middle word (index `floor(N/2)`) gets the ORP split; others render flat. Font size shrinks by 8 px (matching legacy) to fit three words.

### `RsvpSectionPicker.svelte`

Rendered only when `rsvp.picker.open` is true. Lists each detected section with a checkbox and `~NNN words`. All-checked by default (or pre-checked to `rsvp_remembered_sections` if `rsvp_remember_sections` is true). "Remember selection" checkbox writes both flags back to settings when the user clicks Start. The Start button assembles selected section text joined by `\n\n` and calls `rsvp.startReading(text, 'soap')`.

## Entry points

Each document view gets a `Speed Read` button next to the existing `Copy` button:

| View | Currently has Copy button at | Speed Read button added |
|---|---|---|
| `RecordTab.svelte` (pipeline completed) | the post-completion actions row | same row, left of Copy |
| `EditorTab.svelte` (any doc type) | editor header | same row |
| `GenerateItem.svelte` (Generate tab) | item's done-group | same group |

Plus a global keybinding on `window` for `Cmd/Ctrl+Shift+R`. The handler walks the current-tab context (via a `currentDocument` Svelte store that the three views update on mount/unmount) and opens the appropriate reader. No-op when no readable document is in view.

## Settings

New fields appended to `AppConfig` in `crates/core/src/types/settings.rs`. They follow the existing flat-field pattern (same as `lmstudio_host`, `lmstudio_port`, `vocabulary_enabled`):

| Field | Type | Default | Notes |
|---|---|---|---|
| `rsvp_wpm` | `u32` | `300` | clamped 50..=2000 on read |
| `rsvp_font_size` | `u32` | `48` | clamped 24..=96 |
| `rsvp_chunk_size` | `u8` | `1` | clamped 1..=3 |
| `rsvp_dark_theme` | `bool` | `true` | Independent of app-wide theme so the reader can run light on dark app and vice versa. |
| `rsvp_show_context` | `bool` | `false` | |
| `rsvp_audio_cue` | `bool` | `false` | |
| `rsvp_auto_start` | `bool` | `true` | |
| `rsvp_remember_sections` | `bool` | `false` | |
| `rsvp_remembered_sections` | `Vec<String>` | `[]` | Section names the user last ticked. JSON-serialized in the DB same as `tags`. |

`AppConfig::migrate` fills these defaults on older rows. Loaded through the existing `settings` store; updated through the existing `settings.updateField(...)` path.

Default-values test added to `crates/core/src/types/settings.rs`.

## Data flow

```
User clicks "Speed Read" on SOAP
    → rsvp.openSoap(text)
        → preprocessSoap(text)
        → detectSections(clean)
        → picker.open = true, picker.sections = [...]

User picks sections, clicks Start
    → rsvp.startReading(selectedText, 'soap')
        → reader.open = true

RsvpReader on mount
    → load RSVP settings
    → tokenize(text) → Token[]
    → if auto_start: setTimeout(tick, 500)

tick() {
    render word at current index with ORP split
    if token is a header boundary: optional beep
    index += chunk_size
    if index >= tokens.length: show completion screen, stop
    else: setTimeout(tick, delayMs(token, baseDelay))
}

Pause: clearTimeout(handle)
Resume: tick()
Close: clearTimeout + reader.open = false
```

## Error handling

| Failure | Response |
|---|---|
| Empty or whitespace-only text at `openSoap`/`openGeneric` | Early-return, `toasts.error("Nothing to read.")` |
| Settings fail to load | Fall back to the hardcoded defaults, log a warning, render dialog normally. |
| Zero tokens after tokenize (e.g., only punctuation) | Empty-state card: "Nothing to read." + Close button. |
| Section picker called with zero detected sections | Skip the picker entirely, call `startReading` with the original text. |
| User resizes window during a chunk render | Pure CSS — flex layout reflows; no manual redraw needed. |

## Testing

Vitest suite at `src/lib/rsvp/engine.test.ts` — mirrors the Python unit-test surface:

- `preprocessSoap`: strips ICD-10 codes, strips ICD-9 codes, strips "Not discussed" lines, strips leading bullets.
- `detectSections`: finds each of the seven SOAP headers case-insensitively; returns `[]` when none; tolerates leading dashes/bullets on the header line.
- `tokenize`: classifies trailing punctuation into word / clause / sentence / header; preserves word ordering; handles sections embedded mid-document.
- `orpIndex`: all four length classes.
- `delayMs`: all four multipliers.
- `baseDelayMs`: `60_000 / wpm`.

Rust side: one `AppConfig` round-trip test covering the new fields and their migration defaults.

Component tests out of scope for v1 (no existing Svelte test harness in the project; adding one is its own project).

## Out of scope, called out

- No PDF / OCR / text-file upload.
- No standalone "paste arbitrary text" input.
- No RSVP on transcripts.
- No cross-session position memory.
- No in-reader text editing or annotation.
- No server-side progress tracking.

## Risks & open questions

- **Keybinding conflict:** `Cmd/Ctrl+Shift+R` is used by some browsers for hard-reload. Inside the Tauri webview this is safe in production — the browser shortcut is not wired — but a developer in dev mode may trigger both. Acceptable; the worst case is a wasted reload.
- **Audio cue in restrictive webviews:** the Tauri webview should allow Web Audio unconditionally; no user-gesture requirement in a desktop shell. If it turns out to need one, the first cue after opening the dialog may be silent; subsequent ones should work since the user has clicked "Start".
- **Section detection robustness:** SOAP notes from the LLM occasionally use `**Subjective:**` (markdown bold). The detection regex should tolerate `*` and `_` emphasis characters around the header name. Noted for the implementation plan.
