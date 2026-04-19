# Marks Amazing Subtitles — Claude Context

Personal fork of [tmoroney/auto-subs](https://github.com/tmoroney/auto-subs).
Fork repo: `mowen7711/marks-amazing-subtitles` — push with `git push myfork main`.

Tauri 2 desktop app (Rust backend + React/TypeScript frontend) that transcribes audio/video
and generates subtitles directly inside DaVinci Resolve. No cloud, no subscription — fully local.

---

## What It Does

- Transcribes audio/video using local AI models (Whisper, Parakeet, Moonshine)
- Speaker diarization — labels and colour-codes speakers automatically
- Voice sampling — filter transcription to only recognised voices
- Translates subtitles via Google Translate
- Exports to SRT, plain text, or clipboard
- Deep integration with **DaVinci Resolve** — injects subtitles directly into timelines via Lua scripts over a local socket
- Per-speaker styling (colour, outline, border) within Resolve
- Voice Activity Detection (VAD) for cleaner segmentation
- GPU acceleration: Metal/CoreML (macOS), CPU-only for Windows currently (Vulkan/DirectML builds fragile in CI)

---

## Tech Stack

### Frontend
- **React 18 + TypeScript**, built with **Vite**
- **Tailwind CSS** + **Radix UI** (headless primitives)
- **React Context API** for state (no Redux)
- **react-i18next** for internationalisation
- **Tauri 2** IPC for communicating with the Rust backend

### Backend (Rust)
- **Tauri 2** — desktop framework, IPC, plugins (fs, http, dialogs, clipboard, store, updater, shell)
- **Tokio** — async runtime
- **whisper-rs** — Whisper transcription
- **transcribe-rs** — Parakeet and Moonshine models (macOS/Linux only — CRT conflict on Windows)
- **pyannote-rs** — speaker diarization
- **ort** (ONNX Runtime) — ML inference runtime
- **FFmpeg** — bundled sidecar, normalises audio to 16kHz mono PCM WAV before transcription
- **hf-hub** — HuggingFace model downloads
- **tracing** / **tracing-appender** / **tracing-subscriber** — structured logging

---

## Repository Structure

```
marks-amazing-subtitles/
├── .github/
│   ├── windows-wrapper.nsi            # NSIS wrapper: VC++ redist + app + Lua bridge
│   └── workflows/
│       └── build-windows.yml          # GitHub Actions Windows CI build
├── AutoSubs-App/
│   ├── src/                           # React frontend
│   │   ├── components/
│   │   │   ├── common/                # Shared UI components
│   │   │   ├── dialogs/               # Modal dialogs
│   │   │   ├── settings/              # Settings panels
│   │   │   │   └── diarize-selector.tsx  # Voice filter UI (samples, threshold)
│   │   │   ├── subtitles/             # Subtitle viewer, editor, speaker settings
│   │   │   └── transcription/
│   │   │       └── transcription-panel.tsx  # Main transcription trigger + IPC call
│   │   ├── contexts/
│   │   │   ├── GlobalProvider.tsx
│   │   │   ├── TranscriptContext.tsx  # Subtitle segments & speaker state
│   │   │   ├── ProgressContext.tsx    # Real-time transcription progress
│   │   │   ├── ModelsContext.tsx      # Available & downloaded models
│   │   │   ├── SettingsContext.tsx    # Persisted settings (incl. voice samples)
│   │   │   └── ResolveContext.tsx     # DaVinci Resolve connection state
│   │   ├── hooks/
│   │   ├── api/                       # IPC communication layer
│   │   ├── types/interfaces.ts        # TypeScript types (VoiceSample, TranscriptionOptions, etc.)
│   │   └── i18n/                      # Internationalisation strings
│   └── src-tauri/
│       ├── src/
│       │   ├── main.rs                # App init, plugins, updater, exit handling, console window
│       │   ├── transcription_api.rs   # transcribe_audio(), cancel_transcription(), reformat_subtitles()
│       │   ├── audio_preprocess.rs    # FFmpeg wrapper — mono 16kHz PCM WAV conversion
│       │   ├── logging.rs             # tracing setup, in-memory ring buffer, JobLog
│       │   ├── models.rs              # Model download & cache management
│       │   └── transcript_types.rs    # IPC-serialisable types (Segment, Speaker, Transcript)
│       ├── crates/
│       │   └── transcription-engine/
│       │       └── src/
│       │           ├── engine.rs          # Engine struct, transcribe_audio() pipeline
│       │           ├── engines/           # whisper.rs, parakeet.rs, moonshine.rs
│       │           ├── model_manager.rs   # HuggingFace download & cache
│       │           ├── formatting.rs      # Line-breaking, noise filtering, language presets
│       │           ├── translate.rs       # Google Translate integration
│       │           ├── vad.rs             # Voice Activity Detection
│       │           └── speaker.rs         # Speaker ID assignment
│       ├── resources/
│       │   └── MarksAmazingSubs.lua   # DaVinci Resolve entry point script
│       ├── tauri.conf.json            # Main Tauri config
│       └── tauri.windows.conf.json    # Windows overrides (titleBarStyle: Visible)
├── Docs/
│   ├── ResolveDocs.txt                # DaVinci Resolve API reference
│   └── FusionDocs.txt                 # Fusion scripting reference
└── CLAUDE.md
```

---

## Key Concepts

### Transcription Pipeline
1. User selects file → frontend calls `transcribe_audio()` IPC command
2. Rust normalises audio via FFmpeg (→ 16kHz mono WAV)
3. Voice samples (if provided) are also normalised
4. Transcription engine runs chosen model locally
5. Optional diarization via Pyannote
6. Optional translation via Google Translate
7. Formatter applies language presets + line-breaking + CPS/CPL constraints + noise filtering
8. Results returned to frontend as `Transcript` with `Segment[]` and `Speaker[]`
9. User edits subtitles (rename speakers, adjust timings, edit text)
10. Export as SRT/text, or inject into DaVinci Resolve timeline

### State Management
All global state lives in React contexts under `src/contexts/`:
- `TranscriptContext` — subtitle segments and speaker data
- `ProgressContext` — real-time transcription progress
- `ModelsContext` — available/cached models
- `SettingsContext` — persisted user settings (includes voice samples)
- `ResolveContext` — DaVinci Resolve connection and timeline info

### DaVinci Resolve Integration
Communication happens over a local socket (port 56003). `MarksAmazingSubs.lua` runs inside
Resolve's Fusion scripting environment, reads `install_path.txt` to locate the app, and
handles timeline extraction, audio export, and subtitle injection.

Lua bridge files installed to:
`%APPDATA%\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\`

`install_path.txt` written to:
`%APPDATA%\...\MarksAmazingSubs\install_path.txt` → contains path to `autosubs.exe`

### Model Storage
- macOS: `~/Library/Caches/com.marks-amazing-subtitles/models`
- Windows: `%LOCALAPPDATA%\marks-amazing-subtitles\models`
- Downloaded automatically from HuggingFace on first use
- Sizes: tiny, base, small, medium, large, xlarge

---

## Building & Running

### Development (macOS)
```bash
cd AutoSubs-App
npm install
npm run tauri dev
```

### Production (macOS)
```bash
cd AutoSubs-App
npm run tauri build    # uses default mac-aarch feature (CoreML + Metal)
```

### Windows (CI only — GitHub Actions)
Triggered via release or `workflow_dispatch` on `build-windows.yml`.
```bash
npm run tauri build -- -- --no-default-features
```
The NSIS wrapper (`.github/windows-wrapper.nsi`) wraps the output and adds:
- VC++ 2015–2022 redistributable
- `install_path.txt` for the Lua bridge
- `MarksAmazingSubs.lua` into Resolve's scripts folder

---

## Cargo Feature Flags

| Flag | Effect | CI status |
|------|--------|-----------|
| `mac-aarch` (default) | CoreML + Metal — Apple Silicon | ✅ works |
| `mac-x86_64` | Metal only — Intel Mac | ✅ works |
| `windows` | Vulkan + DirectML | ❌ Vulkan cmake build fails in CI |
| `windows-cpu` | DirectML only | ❌ requires `onnxruntime.dll` at runtime (silent exit if missing) |
| `linux` | Vulkan | untested |
| _(none)_ `--no-default-features` | CPU-only, static ort | ✅ used for Windows CI |

**Windows CI rule:** always use `--no-default-features` — static ort, no DLL dependencies.

---

## Windows — Critical Notes

| Issue | Cause | Fix |
|-------|-------|-----|
| App silently did not open | `plugins.updater` null in config | Added `"plugins": { "updater": { "pubkey": "", "endpoints": [] } }` to `tauri.conf.json` |
| `msvcp140_1.dll not found` | Missing VC++ runtime | Bundle `vc_redist.x64.exe` in NSIS wrapper |
| App silently exits with `directml` feature | `ort/directml` loads `onnxruntime.dll` dynamically, not found → `process::exit` | Use `--no-default-features` |
| `--features windows` build fails | whisper.cpp Vulkan cmake build broken in CI | Avoid until fixed |
| `makensis.exe` not found | Not pre-installed on runners | Find under `%LOCALAPPDATA%\tauri\` (Tauri downloads its own copy) |
| `titleBarStyle: Overlay` risk | May cause silent window failure on some Windows versions | Overridden to `Visible` in `tauri.windows.conf.json` |
| `plugins.updater` null on Windows | macOS tolerates missing config; Windows panics | Must have `plugins.updater` entry in `tauri.conf.json` |

---

## Logging

- All backend output uses `tracing` — no `println!`/`eprintln!`
- Log files: Tauri app log dir, rolling daily (`logs/autosubs.log.*`)
- In-memory ring buffer: 20,000 lines, accessible via `get_backend_logs` Tauri command
- Per-job logs: `logs/jobs/` — each transcription writes a timestamped file via `JobLog`
- Console window open on Windows (no `windows_subsystem = "windows"`) — live output visible

### JobLog usage
```rust
let mut job = crate::logging::new_job_log(&app, "Transcription [small]");
job.step("Audio normalization", "input=file.mp4");
job.step("Engine complete", "elapsed=12.3s segments=42");
job.finish("segments=42 speakers=2 total_time=15s");
// or job.fail("error message");
// Drop without finish/fail → writes job_incomplete_*.log
```

---

## Features

### Voice sampling
Users provide short audio clips of specific speakers. Only segments matching a sample voice
(above a similarity threshold) are included in the transcript.

- **UI:** `diarize-selector.tsx` — "Voice Filter" toggle, file picker, editable labels, remove button, "Match Sensitivity" slider (0.5–0.95). Only active when diarization is also enabled.
- **State:** `SettingsContext` — `voiceFilterEnabled`, `voiceSamples[]`, `voiceSimilarityThreshold`
- **Backend:** `transcription_api.rs` normalises each sample to mono 16kHz WAV → `voice_sample_paths` → engine

### Inaudible segment filtering
When the engine cannot detect audio clearly, the segment is **dropped** — no subtitle generated.

- **Location:** `crates/transcription-engine/src/formatting.rs` → `is_noise_token()`
- Drops: `[inaudible]`, `(inaudible)`, `blank_audio`, `silence`, `music`, `laughter`, `unintelligible`, `indistinct`, and bracket/paren variants
- Automatic — no user toggle

---

## Notes

- FFmpeg is a bundled sidecar binary — do not rely on system FFmpeg
- `transcription-engine` is independent of Tauri and can be used as a standalone library
- On Windows, `transcribe-rs` (Parakeet/Moonshine) is compiled with `default-features = false` to avoid a CRT conflict (esaxx_rs links `/MT`, rest uses `/MD`)
- DTW (Dynamic Time Warping) is used for accurate word-level timestamps
- The updater plugin is wired up in `main.rs` with download progress + deferred install, but `createUpdaterArtifacts: false` means no update artifacts are currently published
