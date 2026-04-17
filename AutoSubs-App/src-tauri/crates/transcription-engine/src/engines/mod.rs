//! Transcription engine backends.
//!
//! This module provides different speech recognition backends:
//! - **Whisper**: OpenAI's Whisper model via whisper-rs (GGML format)
//! - **Parakeet**: NVIDIA's NeMo Parakeet model via transcribe-rs (ONNX format)

pub mod whisper;

// Moonshine and Parakeet use transcribe-rs with features that cause a /MT vs /MD
// CRT mismatch on Windows. Gate them out on Windows — whisper handles everything there.
#[cfg(not(target_os = "windows"))]
pub mod moonshine;
#[cfg(not(target_os = "windows"))]
pub mod parakeet;

// Re-export commonly used items
pub use whisper::{create_context, run_transcription_pipeline, SHOULD_CANCEL};

#[cfg(not(target_os = "windows"))]
pub use moonshine::transcribe_moonshine;
#[cfg(not(target_os = "windows"))]
pub use parakeet::transcribe_parakeet;
