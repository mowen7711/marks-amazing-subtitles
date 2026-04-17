use crate::types::{DiarizeOptions, LabeledProgressFn, ProgressType, SpeechSegment, VoiceSample};
use eyre::{Result, eyre};

/// Sentinel speaker ID assigned to segments that don't match any voice sample.
/// These are filtered out by the engine before transcription.
pub const FILTERED_SPEAKER: &str = "__filtered__";

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

pub fn label_speakers(
    speech_segments: &mut [SpeechSegment],
    diarize_options: &DiarizeOptions,
    voice_samples: Option<&[VoiceSample]>,
    voice_similarity_threshold: f32,
    progress_callback: Option<&LabeledProgressFn>,
    is_cancelled: Option<&(dyn Fn() -> bool + Send + Sync)>,
) -> Result<()> {
    if speech_segments.is_empty() {
        return Ok(());
    }

    let total_segments = speech_segments.len();

    let mut embedding_manager = pyannote_rs::EmbeddingManager::new(diarize_options.max_speakers);
    let mut extractor = pyannote_rs::EmbeddingExtractor::new(&diarize_options.embedding_model_path)
        .map_err(|e| eyre!("{:?}", e))?;

    // Pre-compute embeddings for voice samples if provided
    let sample_embeddings: Option<Vec<(String, Vec<f32>)>> = match voice_samples {
        Some(samples) if !samples.is_empty() => {
            let mut embeddings = Vec::new();
            for sample in samples {
                match extractor.compute(&sample.samples) {
                    Ok(emb) => embeddings.push((sample.label.clone(), emb)),
                    Err(e) => tracing::warn!(
                        "Failed to compute embedding for voice sample '{}': {:?}",
                        sample.label, e
                    ),
                }
            }
            if embeddings.is_empty() { None } else { Some(embeddings) }
        }
        _ => None,
    };

    for (i, seg) in speech_segments.iter_mut().enumerate() {
        if let Some(is_cancelled) = is_cancelled {
            if is_cancelled() {
                return Err(eyre!("Cancelled"));
            }
        }

        let embedding_result = extractor.compute(&seg.samples);
        let speaker = match embedding_result {
            Ok(embedding_vec) => {
                if let Some(ref samples_emb) = sample_embeddings {
                    // Voice filter mode: find the closest matching sample
                    let best = samples_emb.iter()
                        .map(|(label, emb)| (label.as_str(), cosine_similarity(&embedding_vec, emb)))
                        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                    match best {
                        Some((label, similarity)) if similarity >= voice_similarity_threshold => {
                            tracing::debug!(
                                "Segment [{:.1}s-{:.1}s] matched voice sample '{}' (similarity: {:.3})",
                                seg.start, seg.end, label, similarity
                            );
                            label.to_string()
                        }
                        Some((_, similarity)) => {
                            tracing::debug!(
                                "Segment [{:.1}s-{:.1}s] filtered out (best similarity: {:.3} < threshold {:.3})",
                                seg.start, seg.end, similarity, voice_similarity_threshold
                            );
                            FILTERED_SPEAKER.to_string()
                        }
                        None => FILTERED_SPEAKER.to_string(),
                    }
                } else {
                    // Normal clustering mode (existing behaviour)
                    if embedding_manager.get_all_speakers().len() == diarize_options.max_speakers {
                        embedding_manager
                            .get_best_speaker_match(embedding_vec)
                            .map(|r| r.to_string())
                            .unwrap_or("?".into())
                    } else {
                        embedding_manager
                            .search_speaker(embedding_vec, diarize_options.threshold)
                            .map(|r| r.to_string())
                            .unwrap_or("?".into())
                    }
                }
            }
            Err(e) => {
                tracing::error!("speaker embedding failed: {:?}", e);
                if sample_embeddings.is_some() { FILTERED_SPEAKER.to_string() } else { "?".into() }
            }
        };

        seg.speaker_id = Some(speaker);

        if let Some(cb) = progress_callback {
            let pct = ((i + 1) as f64 / total_segments as f64 * 100.0) as i32;
            cb(pct, ProgressType::Diarize, "progressSteps.diarize");
        }
    }

    Ok(())
}
