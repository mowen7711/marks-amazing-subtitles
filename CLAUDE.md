# AutoSubs - Claude Context

AutoSubs is a cross-platform desktop application that generates subtitles locally using AI transcription models — no cloud, no subscription. Built with Tauri 2 (Rust backend + React/TypeScript frontend).

---

## What It Does

- Transcribes audio/video files using local AI models (Whisper, Parakeet, Moonshine)
- Speaker diarization — automatically labels and colour-codes speakers
- Translates subtitles via Google Translate
- Exports to SRT, plain text, or clipboard
- Deep integration with **DaVinci Resolve** — injects subtitles directly into timelines via Lua scripts over a socket connection
- Per-speaker styling (colour, outline, border) within Resolve
- Voice Activity Detection (VAD) for cleaner segmentation
- GPU acceleration: Metal/CoreML (macOS), Vulkan/DirectML (Windows), Vulkan (Linux)

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
- **transcribe-rs** — Parakeet and Moonshine models
- **pyannote-rs** — speaker diarization
- **FFmpeg** — bundled sidecar, normalises audio to 16kHz mono PCM WAV before transcription
- **hf-hub** — HuggingFace model downloads
- **tracing** — logging

---

## Repository Structure

```
auto-subs/
├── AutoSubs-App/                  # Main Tauri application
│   ├── src/                       # React frontend
│   │   ├── components/
│   │   │   ├── common/            # Shared UI components
│   │   │   ├── dialogs/           # Modal dialogs
│   │   │   ├── settings/          # Settings panels (model picker, language, diarize, etc.)
│   │   │   ├── subtitles/         # Subtitle viewer, editor, speaker settings
│   │   │   └── transcription/     # Transcription panel (main trigger UI)
│   │   ├── contexts/              # Global state
│   │   │   ├── GlobalProvider.tsx
│   │   │   ├── TranscriptContext.tsx   # Subtitle segments & speaker state
│   │   │   ├── ProgressContext.tsx     # Progress tracking (Download/Transcribe/Diarize/Translate)
│   │   │   ├── ModelsContext.tsx       # Available & downloaded models
│   │   │   ├── SettingsContext.tsx     # Persisted app settings
│   │   │   └── ResolveContext.tsx      # DaVinci Resolve integration state
│   │   ├── hooks/                 # Custom React hooks
│   │   ├── api/                   # IPC communication layer
│   │   ├── types/                 # TypeScript types
│   │   └── i18n/                  # Internationalisation strings
│   └── src-tauri/                 # Rust backend
│       ├── src/
│       │   ├── main.rs                  # App init, plugin setup, command registration
│       │   ├── transcription_api.rs     # transcribe_audio(), cancel_transcription() commands
│       │   ├── audio_preprocess.rs      # FFmpeg audio normalisation
│       │   ├── models.rs               # Model cache management
│       │   └── transcript_types.rs     # IPC-serialisable types (Segment, Speaker, Transcript)
│       ├── crates/
│       │   └── transcription-engine/   # Core transcription crate
│       │       └── src/
│       │           ├── engine.rs        # Engine struct, transcribe_audio() pipeline orchestration
│       │           ├── engines/         # whisper.rs, parakeet.rs, moonshine.rs
│       │           ├── model_manager.rs # HuggingFace download & cache
│       │           ├── formatting.rs    # Line-breaking, CPS/CPL limits, language presets
│       │           ├── translate.rs     # Google Translate integration
│       │           ├── vad.rs           # Voice Activity Detection
│       │           └── speaker.rs       # Speaker ID assignment
│       └── resources/                  # DaVinci Resolve integration
│           ├── AutoSubs.lua             # Resolve entry point script
│           ├── Testing-AutoSubs.lua     # Dev version
│           └── modules/
│               ├── autosubs_core.lua    # Core Lua module — UI, timeline, IPC (~57KB)
│               ├── ljsocket.lua         # Socket communication
│               └── dkjson.lua           # JSON parsing
├── flatpak/                       # Flatpak packaging
├── Mac-Package/                   # macOS packaging config
├── Docs/
│   ├── ResolveDocs.txt            # DaVinci Resolve API reference
│   └── FusionDocs.txt             # Fusion scripting reference
└── README.md
```

---

## Key Concepts

### Transcription Pipeline
1. User selects file → frontend emits `transcribe_audio()` IPC command
2. Rust preprocesses audio via FFmpeg (→ 16kHz mono WAV)
3. Transcription engine runs chosen model locally
4. Optional diarization via Pyannote
5. Optional translation via Google Translate
6. Formatter applies language presets + line-breaking + CPS/CPL constraints
7. Results stream back to frontend as `Segment` objects via IPC events
8. User edits subtitles (rename speakers, adjust timings, edit text)
9. Export as SRT/text, or inject into DaVinci Resolve timeline

### State Management
All global state lives in React contexts under `src/contexts/`:
- `TranscriptContext` — subtitle segments and speaker data
- `ProgressContext` — real-time transcription progress
- `ModelsContext` — available/cached models
- `SettingsContext` — persisted user settings
- `ResolveContext` — DaVinci Resolve connection and timeline info

### DaVinci Resolve Integration
Communication happens over a local socket. `autosubs_core.lua` runs inside Resolve's Fusion scripting environment and connects to the running AutoSubs app. The Lua script handles timeline extraction, audio export, and subtitle injection.

### Model Storage
- macOS: `~/Library/Caches/com.autosubs/models`
- Models downloaded automatically from HuggingFace on first use
- Multiple sizes: tiny, base, small, medium, large, xlarge

---

## Building & Running

### Development
```bash
cd AutoSubs-App
npm install
npm run tauri dev
```

### Platform Builds
```bash
npm run build:mac:arm64    # macOS Apple Silicon (CoreML + Metal)
npm run build:mac:x86_64   # macOS Intel (Metal)
npm run build:win          # Windows (Vulkan + DirectML)
npm run build:linux        # Linux (Vulkan)
```

### Prerequisites
- Node.js 18+
- Rust stable toolchain
- macOS 13.3+ for macOS builds

---

## Notes

- FFmpeg is bundled as a Tauri sidecar binary — do not rely on system FFmpeg
- The `transcription-engine` crate at `src-tauri/crates/transcription-engine/` is the core logic and is independent of Tauri; it can be used as a standalone library
- `autosubs_core.lua` is large (~57KB) and handles most of the Resolve integration complexity
- GPU acceleration is compile-time feature-flagged — check `Cargo.toml` features before editing build scripts
- DTW (Dynamic Time Warping) is used for more accurate word-level timestamps

---

## Fork: Marks Amazing Subtitles

This is a personal fork (`mowen7711/marks-amazing-subtitles`) of the upstream `tmoroney/auto-subs`.
Push to fork with: `git push myfork main`

### What differs from upstream
- App renamed to **Marks Amazing Subtitles** (`productName`, identifier, Lua script name)
- Windows CI build (`.github/workflows/build-windows.yml`) — upstream has no Windows build
- Console window enabled on Windows (removed `windows_subsystem = "windows"`) for live log output
- Per-job work logs written to `logs/jobs/` on each transcription
- All `println!`/`eprintln!` replaced with structured `tracing` logging
- `tauri.windows.conf.json` — overrides `titleBarStyle` to `Visible` on Windows
- NSIS wrapper installs VC++ redist + DaVinci Resolve Lua bridge (`install_path.txt`)

### Windows Build — Critical Notes
- Always use `--no-default-features` for Windows CI — keeps ort **statically linked**
- Do NOT use `--features windows-cpu` (directml) without bundling `onnxruntime.dll` — `ort/directml` loads it dynamically at startup and calls `process::exit` silently if missing
- `--features windows` (Vulkan) requires Vulkan SDK in CI — cmake build is fragile, avoid until fixed
- The `plugins.updater` key must exist in `tauri.conf.json` — if absent/null, the updater plugin panics on Windows at startup (macOS tolerates it)
- `makensis.exe` is not pre-installed on GitHub Actions runners — find it under `%LOCALAPPDATA%\tauri\` where Tauri downloads its own copy

### Logging
- `src-tauri/src/logging.rs` — `tracing` setup + `JobLog` struct
- `JobLog::new()` / `job.step()` / `job.finish()` / `job.fail()` — records each pipeline step
- Logs land in Tauri's app log dir (`logs/autosubs.log.*`) and `logs/jobs/`
- In-memory ring buffer (20,000 lines) via `get_backend_logs` Tauri command

---

## Pending Features

### 1. Inaudible segment filtering
**Status:** Not yet implemented.
When the transcription engine cannot detect audio clearly it emits segments with text like
`[inaudible]`, `(inaudible)`, or similar. **These segments must be dropped** — no subtitle
should be generated. Do not display "inaudible" to the user.

**Where to implement:** `transcription_api.rs`, after the engine returns segments and before
building the `Transcript`. Filter out any segment whose `text` (trimmed, lowercased) matches
patterns like `[inaudible]`, `(inaudible)`, `inaudible`, `[unintelligible]`, etc.

### 2. Voice sampling (pre-transcription speaker filtering)
**Status:** Backend fully implemented. Frontend integration status unknown.
Users record or provide short audio clips of specific speakers beforehand. During transcription,
only segments whose speaker embedding matches one of the provided samples (above a configurable
similarity threshold) are included in the output.

**Backend API:** `FrontendTranscribeOptions.voice_samples: Option<Vec<FrontendVoiceSample>>`
Each sample has `{ label: String, path: String }`. The backend normalises each sample to
mono 16kHz WAV and passes them to the engine as `voice_sample_paths`.
`voice_similarity_threshold: Option<f32>` controls how strict the matching is.

Check the frontend (`src/`) to see if the UI for uploading/managing voice samples exists yet.
