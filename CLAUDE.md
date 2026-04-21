# Marks Amazing Subtitles — Claude Context

Personal fork of [tmoroney/auto-subs](https://github.com/tmoroney/auto-subs).
Fork repo: `mowen7711/marks-amazing-subtitles` — push with `git push myfork main`.

Tauri 2 desktop app (Rust backend + React/TypeScript frontend) that transcribes audio/video
and generates subtitles directly inside DaVinci Resolve. No cloud, no subscription — fully local.

**Target platform: Windows 11 + DaVinci Resolve 20** (macOS builds supported but untested by owner).

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
│   │   │   │   └── diagnostics-dialog.tsx  # Settings → Diagnostics panel
│   │   │   ├── settings/              # Settings panels
│   │   │   │   └── diarize-selector.tsx    # Voice filter UI (samples, threshold)
│   │   │   ├── subtitles/             # Subtitle viewer, editor, speaker settings
│   │   │   └── transcription/
│   │   │       └── transcription-panel.tsx  # Main transcription trigger + IPC call
│   │   ├── contexts/
│   │   │   ├── GlobalProvider.tsx
│   │   │   ├── TranscriptContext.tsx  # Subtitle segments & speaker state
│   │   │   ├── ProgressContext.tsx    # Real-time transcription progress
│   │   │   ├── ModelsContext.tsx      # Available & downloaded models
│   │   │   ├── SettingsContext.tsx    # Persisted settings (incl. voice samples)
│   │   │   └── ResolveContext.tsx     # DaVinci Resolve connection state + lastConnectionError
│   │   ├── hooks/
│   │   ├── api/                       # IPC communication layer
│   │   ├── types/interfaces.ts        # TypeScript types (VoiceSample, TranscriptionOptions, etc.)
│   │   └── i18n/                      # Internationalisation strings
│   └── src-tauri/
│       ├── src/
│       │   ├── main.rs                # App init, plugins, updater, exit handling, console window
│       │   ├── transcription_api.rs   # transcribe_audio(), cancel_transcription(), reformat_subtitles()
│       │   ├── audio_preprocess.rs    # FFmpeg wrapper — mono 16kHz PCM WAV conversion
│       │   ├── logging.rs             # tracing setup, in-memory ring buffer, JobLog, get_lua_log, get_app_diagnostics
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
│       │   ├── MarksAmazingSubs.lua   # DaVinci Resolve entry point script
│       │   └── modules/
│       │       ├── autosubs_core.lua  # Core server + Resolve API logic
│       │       ├── libavutil.lua      # Timecode helpers (wraps avutil via FFI)
│       │       ├── ljsocket.lua       # TCP socket library
│       │       └── dkjson.lua         # JSON library
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
- `ResolveContext` — DaVinci Resolve connection, timeline info, `lastConnectionError`, `connectionAttempts`

### DaVinci Resolve Integration
Communication happens over a local socket (port 56003). `MarksAmazingSubs.lua` runs inside
Resolve's Fusion scripting environment, reads `install_path.txt` to locate the app, and
handles timeline extraction, audio export, and subtitle injection.

Lua bridge files installed to:
`%APPDATA%\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\`

`install_path.txt` written to:
`%APPDATA%\...\MarksAmazingSubs\install_path.txt` → contains path to install dir (e.g. `C:\Users\<user>\AppData\Local\Marks Amazing Subtitles`)

The actual executable is `autosubs.exe` (from `package.json` `"name": "autosubs"`) — NOT `Marks Amazing Subtitles.exe`.

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
| install_path.txt written with wrong path | windows-wrapper.nsi hardcoded `Programs\` subdirectory | Tauri installs to `%LOCALAPPDATA%\<productName>` (no Programs subdir) |
| Lua script finds wrong exe name | Lua was looking for `Marks Amazing Subtitles.exe` | Binary is `autosubs.exe` (from package.json name, not productName) |

---

## Logging

- All backend output uses `tracing` — no `println!`/`eprintln!`
- Log files: Tauri app log dir, rolling daily (`logs/autosubs.log.*`)
- In-memory ring buffer: 20,000 lines, accessible via `get_backend_logs` Tauri command
- Per-job logs: `logs/jobs/` — each transcription writes a timestamped file via `JobLog`
- Console window open on Windows (no `windows_subsystem = "windows"`) — live output visible in the terminal
- **Stdout layer added** — all `tracing` output appears live in the console window (was blank before)

### Diagnostics (Settings → Diagnostics)
- `DiagnosticsDialog` (`src/components/dialogs/diagnostics-dialog.tsx`) — shows Lua launch log, backend log tail, Resolve connection status/errors, app version/platform
- `get_lua_log` Tauri command — reads `%TEMP%\MarksAmazingSubs_launch.log`
- `get_app_diagnostics` Tauri command — returns version, log dir, platform/arch
- `ResolveContext` tracks `lastConnectionError` (timestamped) and `connectionAttempts`
- Lua launch log written to `%TEMP%\MarksAmazingSubs_launch.log` — check this first when the script doesn't launch the app

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

## DaVinci Resolve Integration

### Connection Flow
- At app startup, `ResolveContext` fetches timeline info from the Lua server once.
- If Resolve is not running yet, it **polls every 5 seconds** until connected — the "Add to Timeline" button appears automatically once the Lua server responds.
- Once connected, polling stops; `refresh()` can be called manually (triggered when the track selector opens).

### Add to Timeline Flow
1. Completion step → "Add to Timeline" button (only visible when `timelineInfo.timelineId` is set)
2. `AddToTimelineDialog` — 2–3 step wizard: choose template, optional speaker styling, choose output track
3. `handleAddToTimeline()` (`transcription-panel.tsx`) → `pushToTimeline()` (`ResolveContext`) → `addSubtitlesToTimeline()` (`resolve-api.ts`)
4. HTTP POST to `http://localhost:56003/` with `{func: "AddSubtitles", filePath, templateName, trackIndex, conflictMode}`
5. Lua `AddSubtitles()` in `autosubs_core.lua` writes subtitle clips to the timeline

### Error handling
- `addSubtitlesToTimeline()` checks the Lua response: throws if `message` starts with `"Job failed"` or `result === false`
- Errors propagate from `handleAddToTimeline` (no try/catch) → caught by the dialog → shown as red error text inside the dialog

### Transcript storage
- Transcripts saved to `~/Documents/MarksAmazingSubs-Transcripts/<name>__<id>.json`
- Full path constructed by `getTranscriptPath(filename)` and sent to Lua as `filePath`
- Lua reads transcript files using `io.open`

---

## Lua Scripting — Critical Notes

DaVinci Resolve keeps Lua state between script invocations (same process). This has several implications:

### Module caching
`require()` caches modules in `package.loaded`. `MarksAmazingSubs.lua` clears all four modules before each run:
```lua
package.loaded["autosubs_core"] = nil
package.loaded["libavutil"] = nil
package.loaded["ljsocket"] = nil
package.loaded["dkjson"] = nil
```
Without this, editing a module file has no effect until Resolve restarts.

### Resolve/Fusion API objects at require() time
`resolve`, `Resolve()`, `fusion`, `Fusion()` are **not accessible inside `require()`** — they return nil when called at module load time. They are only available in the active script execution context.

**Pattern used throughout:** access via `rawget(_G, "resolve")` / `rawget(_G, "fusion")` rather than calling `Resolve()` / `Fusion()`, and initialise from within `Init()` rather than at module top level:
```lua
-- In autosubs_core.lua Init():
resolve = rawget(_G, "resolve") or (type(rawget(_G, "Resolve")) == "function" and Resolve())
projectManager = resolve:GetProjectManager()
project = projectManager:GetCurrentProject()
mediaPool = project:GetMediaPool()

-- In libavutil.lua load_library():
local fu = rawget(_G, "fusion") or (type(rawget(_G, "Fusion")) == "function" and Fusion())
```

### Stale project/mediaPool in AddSubtitles
`project` and `mediaPool` captured at `Init()` time become stale if the user switches timelines or transcription takes a long time. `AddSubtitles()` re-fetches them at the start of every call:
```lua
project = projectManager:GetCurrentProject()
mediaPool = project:GetMediaPool()
```

### ffi.cdef conflicts
`ffi.cdef` definitions persist across module reloads (LuaJIT keeps them globally). Defining the same type twice causes errors. `MarksAmazingSubs.lua` no longer calls `ffi.cdef` at all — it uses plain `io.open` for reading `install_path.txt`. Only `autosubs_core.lua` and `libavutil.lua` call `ffi.cdef`.

### Fusion tool names — do NOT rename
These string constants reference Resolve's internal Fusion tool names and must stay exactly as-is:
- `ANIMATED_CAPTION = "AutoSubs Caption"` — Fusion comp tool name
- `comp:FindTool("AutoSubs")` — Fusion comp tool name
- `assets_path = join_path(resources_path, "AutoSubs")` — resource folder name inside the app

### Install paths (Windows)
- App installed to: `%LOCALAPPDATA%\Marks Amazing Subtitles\`
- Executable: `autosubs.exe` (NOT `Marks Amazing Subtitles.exe`)
- `install_path.txt` at: `%APPDATA%\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\MarksAmazingSubs\install_path.txt`
- Contents of `install_path.txt`: the install dir path (e.g. `C:\Users\sophi\AppData\Local\Marks Amazing Subtitles`)
- Lua launch log: `%TEMP%\MarksAmazingSubs_launch.log` — first place to check when the script doesn't work

---

## Notes

- FFmpeg is a bundled sidecar binary — do not rely on system FFmpeg
- `transcription-engine` is independent of Tauri and can be used as a standalone library
- On Windows, `transcribe-rs` (Parakeet/Moonshine) is compiled with `default-features = false` to avoid a CRT conflict (esaxx_rs links `/MT`, rest uses `/MD`)
- DTW (Dynamic Time Warping) is used for accurate word-level timestamps
- The updater plugin is wired up in `main.rs` with download progress + deferred install, but `createUpdaterArtifacts: false` means no update artifacts are currently published
