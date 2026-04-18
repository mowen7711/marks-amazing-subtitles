use crate::audio_preprocess as audio;
use crate::models::get_cache_dir;
use crate::transcript_types::{ColorModifier, Sample, Segment, Speaker, Transcript, WordTimestamp};
use eyre::Result;
use serde::{Deserialize, Serialize};
use serde_json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::Instant;
use tauri::{command, AppHandle, Emitter, Manager, Runtime};
use transcription_engine::{Engine, EngineConfig, TranscribeOptions, Callbacks, Segment as WDSegment, ProgressType, PostProcessConfig, process_segments, TextDensity, VoiceSamplePath};

// Frontend-compatible progress data type
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LabeledProgress {
    pub progress: i32,
    #[serde(rename = "type")]
    pub progress_type: Option<String>,
    pub label: Option<String>,
}

impl From<(&i32, &Option<ProgressType>, &Option<String>)> for LabeledProgress {
    fn from((progress, progress_type, label): (&i32, &Option<ProgressType>, &Option<String>)) -> Self {
        Self {
            progress: *progress,
            progress_type: progress_type.as_ref().map(|t| format!("{:?}", t)),
            label: label.clone(),
        }
    }
}


// Global cancellation state (public so main.rs can access it for exit handling)
pub static SHOULD_CANCEL: Mutex<bool> = Mutex::new(false);

// Latest progress value and type updated from callbacks
static LATEST_PROGRESS: AtomicI32 = AtomicI32::new(0);
static LATEST_PROGRESS_TYPE: Mutex<Option<ProgressType>> = Mutex::new(None);
static LATEST_PROGRESS_LABEL: Mutex<Option<String>> = Mutex::new(None);

static NORMALIZED_AUDIO_COUNTER: AtomicU64 = AtomicU64::new(0);

// Utility function for rounding to n decimal places
fn round_to_places(val: f64, places: u32) -> f64 {
    let factor = 10f64.powi(places as i32);
    (val * factor).trunc() / factor
}

/// A voice sample supplied by the frontend — the file at `path` will be
/// normalised by the Tauri layer before the engine processes it.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendVoiceSample {
    pub label: String,
    pub path: String,
}

// --- Frontend Options Struct ---
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendTranscribeOptions {
    pub audio_path: String,
    pub offset: Option<f64>,
    pub model: String, // e.g., "tiny", "base", "small", "medium", "large"
    pub lang: Option<String>,
    pub translate: Option<bool>,
    pub target_language: Option<String>,
    pub enable_dtw: Option<bool>,
    pub enable_gpu: Option<bool>,
    pub enable_diarize: Option<bool>,
    pub max_speakers: Option<usize>,
    pub density: Option<TextDensity>,
    pub max_lines: Option<usize>,
    /// Voice samples for speaker filtering. When non-empty, only segments
    /// whose speaker embedding matches a sample are transcribed.
    pub voice_samples: Option<Vec<FrontendVoiceSample>>,
    pub voice_similarity_threshold: Option<f32>,
}

#[command]
pub async fn cancel_transcription() -> Result<(), String> {
    tracing::info!(target: "autosubs", "Cancellation requested");
    if let Ok(mut should_cancel) = SHOULD_CANCEL.lock() {
        *should_cancel = true;
        tracing::debug!(target: "autosubs", "Cancellation flag set");
    } else {
        return Err("Failed to acquire cancellation lock".to_string());
    }
    Ok(())
}

#[command]
pub async fn transcribe_audio<R: Runtime>(
    app: AppHandle<R>,
    options: FrontendTranscribeOptions,
) -> Result<Transcript, String> {
    let start_time = Instant::now();
    tracing::info!(
        target: "autosubs",
        model = %options.model,
        lang = ?options.lang,
        diarize = ?options.enable_diarize,
        translate = ?options.translate,
        "Transcription requested"
    );

    // Reset progress and cancellation state
    LATEST_PROGRESS.store(0, Ordering::Relaxed);
    if let Ok(mut progress_type_lock) = LATEST_PROGRESS_TYPE.lock() {
        *progress_type_lock = None;
    }
    if let Ok(mut progress_label_lock) = LATEST_PROGRESS_LABEL.lock() {
        *progress_label_lock = None;
    }
    if let Ok(mut should_cancel) = SHOULD_CANCEL.lock() {
        *should_cancel = false;
    }
    tracing::debug!(target: "autosubs", "Cancellation flag reset");

    // Create job log — records each pipeline step to logs/jobs/
    let mut job = crate::logging::new_job_log(&app, format!("Transcription [{}]", options.model));
    job.step("Options", &format!(
        "model={} lang={} diarize={} translate={} target_lang={} gpu={} dtw={} max_lines={:?} density={:?} voice_samples={}",
        options.model,
        options.lang.as_deref().unwrap_or("auto"),
        options.enable_diarize.unwrap_or(false),
        options.translate.unwrap_or(false),
        options.target_language.as_deref().unwrap_or("none"),
        options.enable_gpu.unwrap_or(true),
        options.enable_dtw.unwrap_or(true),
        options.max_lines,
        options.density,
        options.voice_samples.as_ref().map_or(0, |v| v.len()),
    ));

    let emit_app = app.clone();
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
    let emitter_handle = tokio::spawn(async move {
        let mut last_progress = -1;
        let mut last_progress_type: Option<ProgressType> = None;
        let mut last_progress_label: Option<String> = None;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let progress = LATEST_PROGRESS.load(Ordering::Relaxed).clamp(0, 100);
                    let progress_type = LATEST_PROGRESS_TYPE.lock().unwrap().clone();
                    let progress_label = LATEST_PROGRESS_LABEL.lock().unwrap().clone();

                    if progress != last_progress || progress_type != last_progress_type || progress_label != last_progress_label {
                        let labeled_progress = LabeledProgress::from((&progress, &progress_type, &progress_label));
                        let _ = emit_app.emit("labeled-progress", labeled_progress);
                        last_progress = progress;
                        last_progress_type = progress_type;
                        last_progress_label = progress_label;
                    }
                }
                _ = &mut stop_rx => {
                    break;
                }
            }
        }
    });

    // --- Audio Normalization ---
    let audio_input_name = std::path::Path::new(&options.audio_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let norm_start = Instant::now();

    let raw_norm_path = if should_normalize(options.audio_path.clone().into()) {
        job.step("Audio normalization", &format!("input={}", audio_input_name));
        create_normalized_audio(app.clone(), options.audio_path.clone().into(), None)
            .await
            .map_err(|e| format!("Failed to normalize audio: {}", e))?
    } else {
        tracing::debug!(target: "autosubs", "Skip normalize");
        options.audio_path.clone().into()
    };

    job.step("Audio normalization done", &format!(
        "elapsed={:.2}s output={}",
        norm_start.elapsed().as_secs_f64(),
        raw_norm_path.display()
    ));

    // Rename the main audio to a stable name so voice-sample normalization
    // (which also writes to normalized_audio.wav) cannot overwrite it.
    let main_audio_dest = raw_norm_path.with_file_name("normalized_main_audio.wav");
    let audio_path = if raw_norm_path != main_audio_dest {
        match std::fs::rename(&raw_norm_path, &main_audio_dest) {
            Ok(()) => main_audio_dest,
            Err(e) => {
                tracing::warn!(target: "autosubs", "Could not rename main audio, using original path: {}", e);
                raw_norm_path
            }
        }
    } else {
        raw_norm_path
    };
    tracing::info!(target: "autosubs", "Normalized audio path: {}", audio_path.display());

    // --- Normalise voice sample files ---
    let mut normalised_voice_samples: Vec<VoiceSamplePath> = Vec::new();
    if let Some(ref frontend_samples) = options.voice_samples {
        for (i, sample) in frontend_samples.iter().enumerate() {
            match create_normalized_audio(
                app.clone(),
                sample.path.clone().into(),
                None,
            ).await {
                Ok(norm_path) => {
                    let sample_out = norm_path.with_file_name(format!("voice_sample_{}.wav", i));
                    if let Err(e) = std::fs::rename(&norm_path, &sample_out) {
                        tracing::warn!(target: "autosubs", "Could not rename sample {}: {}", i, e);
                        normalised_voice_samples.push(VoiceSamplePath {
                            label: sample.label.clone(),
                            path: norm_path.to_string_lossy().to_string(),
                        });
                    } else {
                        normalised_voice_samples.push(VoiceSamplePath {
                            label: sample.label.clone(),
                            path: sample_out.to_string_lossy().to_string(),
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!(target: "autosubs", "Failed to normalise voice sample '{}': {}", sample.label, e);
                }
            }
        }
    }

    job.step("Voice samples", &format!(
        "{} sample(s) normalised, {} failed",
        normalised_voice_samples.len(),
        options.voice_samples.as_ref().map_or(0, |v| v.len()).saturating_sub(normalised_voice_samples.len())
    ));

    // Clone app handle for segment callback and wrap in Arc for thread-safe sharing
    let segment_emit_app = Arc::new(app.clone());
    let segment_emit_app_clone = Arc::clone(&segment_emit_app);

    // Run transcription using the transcription-engine crate
    let engine_start = Instant::now();
    let res = async move {
        // Get the proper cache directory for models
        let cache_dir = get_cache_dir(app.clone())
            .map_err(|e| format!("Failed to get cache directory: {}", e))?;

        tracing::info!(target: "autosubs", "Cache directory: {}", cache_dir.display());

        // Create engine config with proper cache directory
        let engine_config = EngineConfig {
            cache_dir: cache_dir.clone(),
            enable_dtw: options.enable_dtw.or(Some(true)),
            enable_flash_attn: Some(true),
            use_gpu: options.enable_gpu.or(Some(true)),
            gpu_device: None,
            vad_model_path: None,
            diarize_segment_model_path: None,
            diarize_embedding_model_path: None,
        };
        tracing::info!(
            target: "autosubs",
            "Engine config: dtw={:?} flash_attn={:?} gpu={:?} cache={}",
            engine_config.enable_dtw,
            engine_config.enable_flash_attn,
            engine_config.use_gpu,
            cache_dir.display()
        );

        let mut engine = Engine::new(engine_config);

        // Map frontend options to crate options
        let mut transcribe_options = TranscribeOptions::default();
        transcribe_options.model = options.model.clone();
        transcribe_options.lang = options.lang.clone().or(Some("auto".into()));
        transcribe_options.enable_vad = Some(true);
        transcribe_options.enable_diarize = options.enable_diarize;
        transcribe_options.max_speakers = match options.max_speakers {
            Some(0) => None,
            other => other,
        };
        if options.translate.unwrap_or(false) {
            if let Some(target) = options.target_language {
                if target == "en" {
                    transcribe_options.whisper_to_english = Some(true);
                    transcribe_options.translate_target = None;
                } else {
                    transcribe_options.whisper_to_english = Some(false);
                    transcribe_options.translate_target = Some(target);
                }
            } else {
                transcribe_options.whisper_to_english = Some(true);
                transcribe_options.translate_target = None;
            }
        } else {
            transcribe_options.whisper_to_english = Some(false);
            transcribe_options.translate_target = None;
        }

        // Voice sample filtering
        transcribe_options.voice_sample_paths = if normalised_voice_samples.is_empty() {
            None
        } else {
            Some(normalised_voice_samples)
        };
        transcribe_options.voice_similarity_threshold = options.voice_similarity_threshold;

        // Set up callbacks
        let segment_callback = move |segment: &WDSegment| {
            tracing::debug!(target: "autosubs", "New segment: {}", segment.text);
            let _ = segment_emit_app_clone.emit("new-segment", segment.text.clone());
        };

        let callbacks = Callbacks {
            progress: Some(&|percent: i32, progress_type: ProgressType, label: &str| {
                tracing::debug!(
                    target: "autosubs",
                    "{}: {}% - {:?}",
                    label, percent, progress_type
                );
                LATEST_PROGRESS.store(percent, Ordering::Relaxed);
                if let Ok(mut progress_type_lock) = LATEST_PROGRESS_TYPE.lock() {
                    *progress_type_lock = Some(progress_type.clone());
                }
                if let Ok(mut progress_label_lock) = LATEST_PROGRESS_LABEL.lock() {
                    *progress_label_lock = Some(label.to_string());
                }
            }),
            new_segment_callback: Some(&segment_callback),
            is_cancelled: Some(Box::new(|| {
                if let Ok(should_cancel) = SHOULD_CANCEL.lock() {
                    *should_cancel
                } else {
                    false
                }
            })),
        };

        // Check for cancellation before starting transcription
        if let Ok(should_cancel) = SHOULD_CANCEL.lock() {
            if *should_cancel {
                return Err("Transcription cancelled".to_string());
            }
        }

        tracing::info!(target: "autosubs", "Calling transcription engine: model={}", transcribe_options.model);

        let segments = engine
            .transcribe_audio(
                &audio_path.to_string_lossy(),
                transcribe_options,
                options.max_lines,
                options.density,
                Some(callbacks),
            )
            .await
            .map_err(|e| {
                if let Ok(should_cancel) = SHOULD_CANCEL.lock() {
                    if *should_cancel {
                        return "Transcription cancelled".to_string();
                    }
                }
                format!("Transcription failed: {}", e)
            })?;

        tracing::info!(target: "autosubs", "Engine returned {} raw segments", segments.len());

        // Check for cancellation after transcription completes
        if let Ok(should_cancel) = SHOULD_CANCEL.lock() {
            if *should_cancel {
                return Err("Transcription cancelled".to_string());
            }
        }

        // Convert engine segments to app's Segment format
        let mut app_segments: Vec<Segment> = segments
            .iter()
            .map(|seg| {
                let words = seg.words.as_ref().map(|words| {
                    words
                        .iter()
                        .map(|w| WordTimestamp {
                            word: w.text.clone(),
                            start: w.start,
                            end: w.end,
                            probability: w.probability,
                        })
                        .collect()
                });

                Segment {
                    speaker_id: seg.speaker_id.clone(),
                    start: seg.start,
                    end: seg.end,
                    text: seg.text.clone(),
                    words,
                }
            })
            .collect();

        if options.enable_diarize.unwrap_or(false) {
            let total = app_segments.len();
            let unknown = app_segments
                .iter()
                .filter(|s| s.speaker_id.as_deref().unwrap_or("").trim() == "?")
                .count();

            if total > 0 && unknown == total {
                tracing::warn!(
                    target: "autosubs",
                    "Diarization enabled but all {} segments have unknown speaker_id ('?'). Check model availability and options.max_speakers.",
                    total
                );
            } else {
                tracing::info!(
                    target: "autosubs",
                    "Diarization: {}/{} segments have identified speakers",
                    total - unknown, total
                );
            }
        }

        // Apply offset if provided
        if let Some(offset) = options.offset {
            tracing::debug!(target: "autosubs", "Applying timestamp offset: {}s", offset);
            for segment in app_segments.iter_mut() {
                segment.start = round_to_places(segment.start + offset, 3);
                segment.end = round_to_places(segment.end + offset, 3);
                if let Some(words) = &mut segment.words {
                    for word in words.iter_mut() {
                        word.start = round_to_places(word.start + offset, 3);
                        word.end = round_to_places(word.end + offset, 3);
                    }
                }
            }
        }

        // Aggregate speakers if diarization was enabled
        let (speakers, segments) = if options.enable_diarize.unwrap_or(false) {
            aggregate_speakers_from_segments(&app_segments)
        } else {
            (Vec::new(), app_segments)
        };

        Ok::<Transcript, String>(Transcript {
            processing_time_sec: 0, // Set below
            segments,
            speakers,
        })
    }
    .await;

    let engine_elapsed = engine_start.elapsed().as_secs_f64();

    // Stop emitter and wait for it to finish
    let _ = stop_tx.send(());
    let _ = emitter_handle.await;

    match res {
        Ok(mut transcript) => {
            transcript.processing_time_sec = start_time.elapsed().as_secs();

            // Log result summary
            use std::collections::BTreeMap;
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            for seg in transcript.segments.iter() {
                let key = seg
                    .speaker_id
                    .as_deref()
                    .unwrap_or("<none>")
                    .trim()
                    .to_string();
                *counts.entry(key).or_insert(0) += 1;
            }
            tracing::info!(
                target: "autosubs",
                "Transcription complete: segments={} speakers={} speaker_counts={:?} elapsed={:.2}s",
                transcript.segments.len(),
                transcript.speakers.len(),
                counts,
                transcript.processing_time_sec
            );

            if std::env::var("AUTOSUBS_DEBUG_TRANSCRIPT").ok().as_deref() == Some("1") {
                match serde_json::to_string_pretty(&transcript) {
                    Ok(json) => tracing::debug!(target: "autosubs", "Final transcript JSON:\n{}", json),
                    Err(e) => tracing::warn!(target: "autosubs", "Failed to serialize transcript for debug: {}", e),
                }
            }

            job.step("Engine complete", &format!(
                "elapsed={:.2}s segments={} speakers={}",
                engine_elapsed,
                transcript.segments.len(),
                transcript.speakers.len()
            ));
            job.finish(&format!(
                "segments={} speakers={} total_time={}s",
                transcript.segments.len(),
                transcript.speakers.len(),
                transcript.processing_time_sec
            ));

            Ok(transcript)
        }
        Err(e) => {
            tracing::error!(target: "autosubs", "Transcription error after {:.2}s: {}", engine_elapsed, e);
            job.step("Engine failed", &format!("elapsed={:.2}s error={}", engine_elapsed, e));
            job.fail(&e);
            Err(e)
        }
    }
}


/// Always normalize audio to ensure it's mono 16kHz WAV for the transcription engine
fn should_normalize(_source: PathBuf) -> bool {
    true
}


// This function must now be `async` because it calls the async `normalize` function.
pub async fn create_normalized_audio<R: Runtime>(
    app: AppHandle<R>,
    source: PathBuf,
    additional_ffmpeg_args: Option<Vec<String>>,
) -> Result<PathBuf> {
    tracing::debug!(target: "autosubs", "normalize {:?}", source.display());

    let path_resolver = app.path();

    let cache_dir = path_resolver
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir());

    let out_path = if cfg!(test) {
        let n = NORMALIZED_AUDIO_COUNTER.fetch_add(1, Ordering::Relaxed);
        cache_dir.join(format!("normalized_audio_{}.wav", n))
    } else {
        cache_dir.join("normalized_audio.wav")
    };

    tracing::info!(target: "autosubs", "Normalizing audio: {} -> {}", source.display(), out_path.display());

    audio::normalize(app, source, out_path.clone(), additional_ffmpeg_args)
        .await
        .map_err(|e| eyre::eyre!("Failed to normalize audio: {}", e))?;

    Ok(out_path)
}


/// Aggregates speakers from transcript segments, similar to the frontend logic
fn aggregate_speakers_from_segments(segments: &[Segment]) -> (Vec<Speaker>, Vec<Segment>) {
    use std::collections::HashMap;

    let mut speaker_info: HashMap<String, (usize, f64, f64)> = HashMap::new();
    let mut next_index: usize = 0;

    for segment in segments.iter() {
        if let Some(ref speaker_id) = segment.speaker_id {
            let trimmed = speaker_id.trim();
            if trimmed.is_empty() || trimmed == "?" {
                continue;
            }

            let raw_id = if let Some(rest) = trimmed.strip_prefix("Speaker ") {
                rest.trim().to_string()
            } else {
                trimmed.to_string()
            };

            if !speaker_info.contains_key(&raw_id) {
                speaker_info.insert(raw_id, (next_index, segment.start, segment.end));
                next_index += 1;
            }
        }
    }

    let updated_segments = segments.to_vec();

    let mut speakers = Vec::new();
    let mut speaker_list: Vec<(String, (usize, f64, f64))> = speaker_info.into_iter().collect();
    speaker_list.sort_by_key(|(_, (index, _, _))| *index);

    for (raw_id, (_, start, end)) in speaker_list {
        speakers.push(Speaker {
            name: format!("Speaker {}", raw_id),
            sample: Sample { start, end },
            fill: ColorModifier::default(),
            outline: ColorModifier::default(),
            border: ColorModifier::default(),
        });
    }

    (speakers, updated_segments)
}

// --- Frontend Formatting Options Struct ---
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendFormattingOptions {
    pub language: Option<String>,
    pub max_lines: Option<usize>,
    pub text_density: Option<String>,
}

/// Reformat subtitles with new formatting options without re-transcribing.
/// Takes the raw word-level data and applies formatting rules to produce new segments.
#[command]
pub async fn reformat_subtitles(
    segments: Vec<Segment>,
    options: FrontendFormattingOptions,
) -> Result<Vec<Segment>, String> {
    tracing::info!(
        target: "autosubs",
        "Reformat requested: lang={:?} max_lines={:?} density={:?}",
        options.language, options.max_lines, options.text_density
    );

    // Convert app segments to engine segments (WDSegment)
    let engine_segments: Vec<WDSegment> = segments
        .iter()
        .map(|seg| {
            let words = seg.words.as_ref().map(|words| {
                words
                    .iter()
                    .map(|w| transcription_engine::WordTimestamp {
                        text: w.word.clone(),
                        start: w.start,
                        end: w.end,
                        probability: w.probability,
                    })
                    .collect()
            });

            WDSegment {
                start: seg.start,
                end: seg.end,
                text: seg.text.clone(),
                words,
                speaker_id: seg.speaker_id.clone(),
            }
        })
        .collect();

    let lang = options.language.as_deref().unwrap_or("en");
    let mut config = PostProcessConfig::for_language(lang);

    if let Some(ref density_str) = options.text_density {
        let density: TextDensity = match density_str.to_lowercase().as_str() {
            "less" => TextDensity::Less,
            "more" => TextDensity::More,
            "single" => TextDensity::Single,
            _ => TextDensity::Standard,
        };
        config.apply_density(density);
    }
    if let Some(ml) = options.max_lines {
        config.max_lines = ml;
    }

    let formatted = process_segments(&engine_segments, &config);

    let result: Vec<Segment> = formatted
        .iter()
        .map(|seg| {
            let words = seg.words.as_ref().map(|words| {
                words
                    .iter()
                    .map(|w| WordTimestamp {
                        word: w.text.clone(),
                        start: w.start,
                        end: w.end,
                        probability: w.probability,
                    })
                    .collect()
            });

            Segment {
                start: seg.start,
                end: seg.end,
                text: seg.text.clone(),
                words,
                speaker_id: seg.speaker_id.clone(),
            }
        })
        .collect();

    tracing::info!(target: "autosubs", "Reformat complete: {} -> {} segments", segments.len(), result.len());

    Ok(result)
}
