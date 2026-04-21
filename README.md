# FerriScribe

A privacy-first medical transcription desktop application built with Rust and Svelte. Record doctor-patient encounters, transcribe them locally with speaker diarization, generate SOAP notes, and export to PDF, DOCX, or FHIR.

## Features

- **Local Speech-to-Text** — Whisper (via whisper-rs) with Metal GPU acceleration on macOS. No audio leaves your machine.
- **Speaker Diarization** — Pyannote-based pipeline identifies who is speaking (e.g., Doctor vs. Patient) using ONNX segmentation and speaker embedding models.
- **SOAP Note Generation** — AI-powered generation of Subjective, Objective, Assessment, and Plan notes from transcripts.
- **Medical Document Generation** — Generate referral letters, clinical letters, and synopses.
- **Multi-Provider AI Chat** — OpenAI, Anthropic, Gemini, Groq, Cerebras, Ollama, and LM Studio (local or remote host).
- **Custom Vocabulary** — User-defined find/replace rules applied to transcripts after STT, with word-boundary matching, priority ordering, and import/export compatible with the Python Medical-Assistant `vocabulary.json` format.
- **RAG (Retrieval-Augmented Generation)** — Ingest clinical documents for context-aware AI responses.
- **Agentic Workflows** — Multi-step AI agent orchestration with tool use.
- **Export** — PDF, DOCX, and FHIR R4 (healthcare interoperability standard).
- **Recording Management** — Record, import, search, and organize audio recordings.
- **Secure Key Storage** — API keys stored in the system keychain.
- **Cross-Platform** — macOS, Windows, and Linux.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Frontend | Svelte 5, SvelteKit, TypeScript, Vite |
| Backend | Rust (2024 edition), Tauri v2 |
| STT | whisper-rs (whisper.cpp), ort (ONNX Runtime), knf-rs |
| Database | SQLite (via rusqlite) |
| AI | OpenAI, Anthropic, Gemini, Groq, Cerebras, Ollama, LM Studio |
| Export | PDF, DOCX, FHIR R4 |

## Architecture

FerriScribe is organized as a Cargo workspace with 12 crates:

```
crates/
  core/           — shared types, traits, error handling
  db/             — SQLite database, settings, recordings
  security/       — keychain-based API key storage
  audio/          — microphone capture (cpal)
  ai-providers/   — chat/completion providers
  stt-providers/  — whisper transcription + pyannote diarization
  tts-providers/  — text-to-speech
  agents/         — agentic orchestrator with tool registry
  rag/            — vector store, BM25, graph search, ingestion
  processing/     — transcription pipeline orchestration
  export/         — PDF, DOCX, FHIR export
  translation/    — text translation
src-tauri/        — Tauri app shell, commands, state management
src/              — Svelte 5 frontend
```

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (1.78+)
- [Node.js](https://nodejs.org/) (20+)
- [CMake](https://cmake.org/) and Clang (for whisper.cpp and ONNX Runtime)
- macOS: Xcode Command Line Tools

### Build & Run

```bash
npm install
npm run tauri dev
```

### Model Setup

On first launch, go to **Settings > Audio / STT** and download:

1. **Whisper model** — Choose a size (base ~148 MB to large-v3-turbo ~1.6 GB). Larger models are more accurate.
2. **Diarization models** (for speaker identification):
   - Pyannote segmentation 3.0 (~6 MB)
   - WeSpeaker CAM++ embedding (~28 MB)

Models are downloaded from HuggingFace/GitHub and stored locally.

## Usage

1. **Record** — Click the record button or import an audio file.
2. **Transcribe** — Transcription runs locally with speaker labels. Custom vocabulary corrections (configured in Settings > Custom Vocabulary) are applied automatically after STT.
3. **Generate** — Create SOAP notes, referrals, or clinical letters from transcripts.
4. **Export** — Save as PDF, DOCX, or FHIR R4.
5. **Chat** — Ask questions about the recording or clinical context.

## License

MIT
