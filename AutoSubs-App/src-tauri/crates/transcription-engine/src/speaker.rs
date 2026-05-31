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

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 { v.to_vec() } else { v.iter().map(|x| x / norm).collect() }
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

    // Pre-compute embeddings for voice samples if provided.
    //
    // For each label we keep:
    //   - a L2-normalized centroid (mean direction on the unit sphere) — stable representative
    //   - all individual L2-normalized embeddings — for best-of-N matching
    //
    // Matching uses max(centroid_sim, max(individual_sims)) so a segment only needs to
    // resemble *any* of the provided samples, not just the average.  Pre-normalizing before
    // averaging means high-energy recordings don't bias the centroid.
    struct LabelEmbeddings {
        centroid: Vec<f32>,
        individuals: Vec<Vec<f32>>,
    }
    let sample_embeddings: Option<Vec<(String, LabelEmbeddings)>> = match voice_samples {
        Some(samples) if !samples.is_empty() => {
            let mut by_label: std::collections::HashMap<String, Vec<Vec<f32>>> = std::collections::HashMap::new();
            for sample in samples {
                match extractor.compute(&sample.samples) {
                    Ok(emb) => by_label.entry(sample.label.clone()).or_default().push(l2_normalize(&emb)),
                    Err(e) => tracing::warn!(
                        "Failed to compute embedding for voice sample '{}': {:?}",
                        sample.label, e
                    ),
                }
            }
            let embeddings: Vec<(String, LabelEmbeddings)> = by_label.into_iter()
                .filter_map(|(label, normed_embs)| {
                    if normed_embs.is_empty() { return None; }
                    let dim = normed_embs[0].len();
                    let mut centroid = vec![0.0f32; dim];
                    for emb in &normed_embs {
                        for (c, v) in centroid.iter_mut().zip(emb.iter()) {
                            *c += v;
                        }
                    }
                    // Re-normalize the centroid so it sits on the unit sphere
                    let centroid = l2_normalize(&centroid);
                    tracing::debug!("Voice sample '{}': centroid from {} sample(s)", label, normed_embs.len());
                    Some((label, LabelEmbeddings { centroid, individuals: normed_embs }))
                })
                .collect();
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
            Ok(raw_embedding) => {
                let embedding_vec = if sample_embeddings.is_some() {
                    // Already normalized per-label; normalize segment too for consistent dot products
                    l2_normalize(&raw_embedding)
                } else {
                    raw_embedding
                };
                if let Some(ref samples_emb) = sample_embeddings {
                    // Voice filter mode: best-of-N matching.
                    // For each label compute max(centroid_sim, max(individual_sims)) so the
                    // segment only needs to resemble any one of the provided samples.
                    let best = samples_emb.iter()
                        .map(|(label, le)| {
                            let centroid_sim = cosine_similarity(&embedding_vec, &le.centroid);
                            let best_individual = le.individuals.iter()
                                .map(|ind| cosine_similarity(&embedding_vec, ind))
                                .fold(f32::NEG_INFINITY, f32::max);
                            let sim = centroid_sim.max(best_individual);
                            (label.as_str(), sim)
                        })
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
