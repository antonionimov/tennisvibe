use crate::models::project::{
    AnalysisResultRecord, AnalysisSegmentRecord, AnalysisSummaryRecord, ProjectRecord,
};
use chrono::Utc;
use hound::{SampleFormat, WavReader};
use serde_json::json;
use std::{cmp::Ordering, path::Path};

const FRAME_SEC: f64 = 0.02;
const HOP_SEC: f64 = 0.01;
const WINDOW_SEC: f64 = 0.25;
const MIN_HIT_GAP_SEC: f64 = 0.12;
const CLUSTER_GAP_SEC: f64 = 3.0;
const MERGE_GAP_SEC: f64 = 1.1;
const MIN_SEGMENT_SEC: f64 = 1.2;
const PRE_ROLL_SEC: f64 = 0.8;
const POST_ROLL_SEC: f64 = 0.75;

pub struct AnalyzerOutput {
    pub result: AnalysisResultRecord,
    pub hit_count: usize,
}

pub fn analyze_audio_project(
    project: &ProjectRecord,
    project_dir: &Path,
    proxy_path: &Path,
    audio_path: &Path,
    sensitivity: f64,
) -> Result<AnalyzerOutput, String> {
    let wav = read_wav(audio_path)?;
    let duration_sec = if wav.sample_rate == 0 {
        0.0
    } else {
        wav.samples.len() as f64 / wav.sample_rate as f64
    };

    let features = extract_features(&wav.samples, wav.sample_rate, sensitivity);
    let segments = build_segments(&features.hit_times_sec, &features.hit_strengths, duration_sec, sensitivity);

    let total_duration = segments.iter().map(|segment| segment.duration_sec).sum::<f64>();
    let average_confidence = if segments.is_empty() {
        0.0
    } else {
        segments.iter().map(|segment| segment.confidence).sum::<f64>() / segments.len() as f64
    };

    let summary = AnalysisSummaryRecord {
        duration_sec: round3(duration_sec),
        window_sec: WINDOW_SEC,
        segment_count: segments.len(),
        total_candidate_duration_sec: round3(total_duration),
        average_confidence: round4(average_confidence),
        sensitivity: round3(sensitivity),
    };

    let result = AnalysisResultRecord {
        project_id: project.project_id.clone(),
        created_at: Utc::now().to_rfc3339(),
        sensitivity: round3(sensitivity),
        source_video_path: project.source_video_path.clone(),
        proxy_path: proxy_path.display().to_string(),
        audio_path: audio_path.display().to_string(),
        summary,
        segments,
        debug: json!({
            "rule_engine": "rust_audio_hit_segmentation_v0",
            "audio_sample_rate": wav.sample_rate,
            "audio_hit_count": features.hit_times_sec.len(),
            "analysis_frame_sec": FRAME_SEC,
            "analysis_hop_sec": HOP_SEC,
            "cluster_gap_sec": CLUSTER_GAP_SEC,
            "merge_gap_sec": MERGE_GAP_SEC,
            "notes": "Rust MVP analyzer: energy/peak transient detection plus simple hit-cluster segmentation.",
            "project_dir": project_dir.display().to_string(),
        }),
    };

    Ok(AnalyzerOutput {
        result,
        hit_count: features.hit_times_sec.len(),
    })
}

struct WavData {
    sample_rate: u32,
    samples: Vec<f32>,
}

struct AudioFeatures {
    hit_times_sec: Vec<f64>,
    hit_strengths: Vec<f64>,
}

fn read_wav(path: &Path) -> Result<WavData, String> {
    let mut reader = WavReader::open(path)
        .map_err(|error| format!("打开 wav 失败 {}: {error}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let mono = match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Int, 16) => {
            let raw: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
            interleaved_to_mono_i16(&raw.map_err(|error| format!("读取 wav 样本失败: {error}"))?, channels)
        }
        (SampleFormat::Int, 24 | 32) => {
            let raw: Result<Vec<i32>, _> = reader.samples::<i32>().collect();
            interleaved_to_mono_i32(&raw.map_err(|error| format!("读取 wav 样本失败: {error}"))?, channels, spec.bits_per_sample)
        }
        (SampleFormat::Float, 32) => {
            let raw: Result<Vec<f32>, _> = reader.samples::<f32>().collect();
            interleaved_to_mono_f32(&raw.map_err(|error| format!("读取 wav 样本失败: {error}"))?, channels)
        }
        _ => {
            return Err(format!(
                "暂不支持的 wav 格式: sample_format={:?}, bits_per_sample={}",
                spec.sample_format, spec.bits_per_sample
            ))
        }
    };

    Ok(WavData {
        sample_rate: spec.sample_rate,
        samples: normalize_audio(mono),
    })
}

fn interleaved_to_mono_i16(values: &[i16], channels: usize) -> Vec<f32> {
    values
        .chunks(channels)
        .map(|frame| frame.iter().map(|sample| *sample as f32 / i16::MAX as f32).sum::<f32>() / frame.len() as f32)
        .collect()
}

fn interleaved_to_mono_i32(values: &[i32], channels: usize, bits_per_sample: u16) -> Vec<f32> {
    let scale = ((1i64 << (bits_per_sample.saturating_sub(1) as u32)) - 1) as f32;
    values
        .chunks(channels)
        .map(|frame| frame.iter().map(|sample| *sample as f32 / scale).sum::<f32>() / frame.len() as f32)
        .collect()
}

fn interleaved_to_mono_f32(values: &[f32], channels: usize) -> Vec<f32> {
    values
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

fn normalize_audio(mut samples: Vec<f32>) -> Vec<f32> {
    if samples.is_empty() {
        return samples;
    }

    let mut magnitudes: Vec<f32> = samples.iter().map(|value| value.abs()).collect();
    magnitudes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let index = ((magnitudes.len() as f32 - 1.0) * 0.995).round() as usize;
    let scale = magnitudes.get(index).copied().unwrap_or(1.0).max(1e-6);

    for sample in &mut samples {
        *sample = (*sample / scale).clamp(-1.0, 1.0);
    }

    samples
}

fn extract_features(samples: &[f32], sample_rate: u32, sensitivity: f64) -> AudioFeatures {
    if samples.is_empty() || sample_rate == 0 {
        return AudioFeatures {
            hit_times_sec: Vec::new(),
            hit_strengths: Vec::new(),
        };
    }

    let frame_size = ((sample_rate as f64 * FRAME_SEC).round() as usize).max(1);
    let hop_size = ((sample_rate as f64 * HOP_SEC).round() as usize).max(1);
    let frames = build_frames(samples, frame_size, hop_size);
    if frames.is_empty() {
        return AudioFeatures {
            hit_times_sec: Vec::new(),
            hit_strengths: Vec::new(),
        };
    }

    let rms_values: Vec<f64> = frames
        .iter()
        .map(|frame| {
            let energy = frame.iter().map(|value| {
                let v = *value as f64;
                v * v
            }).sum::<f64>() / frame.len() as f64;
            energy.sqrt()
        })
        .collect();

    let peak_values: Vec<f64> = frames
        .iter()
        .map(|frame| frame.iter().map(|value| value.abs() as f64).fold(0.0, f64::max))
        .collect();

    let baseline = rolling_median(&rms_values, ((1.2 / HOP_SEC).round() as usize).max(3));
    let max_peak = peak_values.iter().copied().fold(0.0, f64::max).max(1e-6);
    let min_peak = peak_values.iter().copied().fold(f64::INFINITY, f64::min);
    let peak_range = (max_peak - min_peak).max(1e-6);

    let threshold_boost = (0.58 - sensitivity.clamp(0.1, 1.0)) * 0.06;
    let transient_threshold = 0.18 + threshold_boost;

    let mut hit_times_sec = Vec::new();
    let mut hit_strengths = Vec::new();

    for index in 1..frames.len().saturating_sub(1) {
        let rms = rms_values[index];
        let prev = rms_values[index - 1];
        let next = rms_values[index + 1];
        let local_peak = peak_values[index];
        let base = baseline[index].max(1e-5);
        let over_base = ((rms / base) - 1.0).max(0.0);
        let peak_norm = ((local_peak - min_peak) / peak_range).clamp(0.0, 1.0);
        let rise = (rms - prev).max(0.0);
        let fall = (rms - next).max(0.0);
        let shape_gate = rise > 0.002 && (rise + fall) > 0.004;
        let transient = (over_base * 0.65 + peak_norm * 0.35).clamp(0.0, 1.6);

        if !shape_gate || transient < transient_threshold || rms < prev || rms < next {
            continue;
        }

        let time_sec = index as f64 * HOP_SEC + FRAME_SEC * 0.5;
        let strength = (transient / 1.2).clamp(0.0, 1.0);

        if let Some(last_time) = hit_times_sec.last_mut() {
            if time_sec - *last_time < MIN_HIT_GAP_SEC {
                if let Some(last_strength) = hit_strengths.last_mut() {
                    if strength > *last_strength {
                        *last_time = round3(time_sec);
                        *last_strength = round4(strength);
                    }
                }
                continue;
            }
        }

        hit_times_sec.push(round3(time_sec));
        hit_strengths.push(round4(strength));
    }

    AudioFeatures {
        hit_times_sec,
        hit_strengths,
    }
}

fn build_frames(samples: &[f32], frame_size: usize, hop_size: usize) -> Vec<&[f32]> {
    let mut frames = Vec::new();
    let mut cursor = 0usize;

    while cursor + frame_size <= samples.len() {
        frames.push(&samples[cursor..cursor + frame_size]);
        cursor += hop_size;
    }

    if frames.is_empty() && !samples.is_empty() {
        frames.push(samples);
    }

    frames
}

fn rolling_median(values: &[f64], kernel: usize) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    let radius = kernel.max(3) / 2;
    let mut output = Vec::with_capacity(values.len());

    for index in 0..values.len() {
        let start = index.saturating_sub(radius);
        let end = (index + radius + 1).min(values.len());
        let mut window = values[start..end].to_vec();
        window.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        output.push(window[window.len() / 2]);
    }

    output
}

fn build_segments(
    hit_times_sec: &[f64],
    hit_strengths: &[f64],
    duration_sec: f64,
    sensitivity: f64,
) -> Vec<AnalysisSegmentRecord> {
    if hit_times_sec.is_empty() {
        return Vec::new();
    }

    let mut clusters: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
    let mut current_times = vec![hit_times_sec[0]];
    let mut current_strengths = vec![hit_strengths[0]];

    for (&time_sec, &strength) in hit_times_sec.iter().zip(hit_strengths.iter()).skip(1) {
        if time_sec - *current_times.last().unwrap_or(&time_sec) <= CLUSTER_GAP_SEC {
            current_times.push(time_sec);
            current_strengths.push(strength);
            continue;
        }

        clusters.push((current_times, current_strengths));
        current_times = vec![time_sec];
        current_strengths = vec![strength];
    }
    clusters.push((current_times, current_strengths));

    let mut segments = Vec::new();
    for (times, strengths) in clusters {
        let start_hit = *times.first().unwrap_or(&0.0);
        let end_hit = *times.last().unwrap_or(&start_hit);
        let hit_count = times.len();
        let mean_strength = strengths.iter().sum::<f64>() / hit_count as f64;
        let dynamic_post_roll = (POST_ROLL_SEC + (hit_count.min(8) as f64 - 1.0).max(0.0) * 0.03)
            .clamp(POST_ROLL_SEC, 1.0);
        let start_sec = (start_hit - PRE_ROLL_SEC).max(0.0);
        let end_sec = (end_hit + dynamic_post_roll).min(duration_sec.max(end_hit + dynamic_post_roll));
        let segment_duration = (end_sec - start_sec).max(0.0);

        if segment_duration < MIN_SEGMENT_SEC {
            continue;
        }

        let confidence = ((mean_strength * 0.6)
            + ((hit_count as f64 / 6.0).min(1.0) * 0.25)
            + (sensitivity.clamp(0.1, 1.0) * 0.15))
            .clamp(0.0, 1.0);

        segments.push(AnalysisSegmentRecord {
            segment_id: String::new(),
            start_sec: round3(start_sec),
            end_sec: round3(end_sec),
            duration_sec: round3(segment_duration),
            confidence: round4(confidence),
            score: round4(confidence),
            reasons: vec!["audio_hit_cluster".to_string()],
            metrics: json!({
                "hit_count": hit_count,
                "hit_strength_mean": round4(mean_strength),
                "first_hit_sec": round3(start_hit),
                "last_hit_sec": round3(end_hit),
            }),
        });
    }

    merge_adjacent_segments(&mut segments);

    for (index, segment) in segments.iter_mut().enumerate() {
        segment.segment_id = format!("segment-{index:03}", index = index + 1);
    }

    segments
}

fn merge_adjacent_segments(segments: &mut Vec<AnalysisSegmentRecord>) {
    if segments.is_empty() {
        return;
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for next in segments.iter().skip(1) {
        if next.start_sec - current.end_sec <= MERGE_GAP_SEC {
            current.end_sec = round3(current.end_sec.max(next.end_sec));
            current.duration_sec = round3(current.end_sec - current.start_sec);
            current.confidence = round4(current.confidence.max(next.confidence));
            current.score = current.confidence;
            current.reasons.push("adjacent_merge".to_string());
            continue;
        }
        merged.push(current);
        current = next.clone();
    }

    merged.push(current);
    *segments = merged;
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn round4(value: f64) -> f64 {
    (value * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_segments_from_synthetic_hits() {
        let sample_rate = 16_000u32;
        let duration_sec = 12.0f64;
        let sample_count = (sample_rate as f64 * duration_sec) as usize;
        let mut samples = vec![0.0f32; sample_count];

        let hit_times = [1.0, 1.7, 2.5, 6.0, 6.8, 7.6, 8.3];
        for hit_time in hit_times {
            inject_pulse(&mut samples, sample_rate, hit_time, 0.9);
        }

        let features = extract_features(&samples, sample_rate, 0.55);
        assert!(features.hit_times_sec.len() >= 5, "expected at least 5 hits, got {}", features.hit_times_sec.len());

        let segments = build_segments(&features.hit_times_sec, &features.hit_strengths, duration_sec, 0.55);
        assert!(!segments.is_empty(), "expected at least one segment");
        assert!(segments.len() <= 2, "expected clustered segments, got {}", segments.len());
        assert!(segments.iter().all(|segment| segment.duration_sec >= MIN_SEGMENT_SEC));
    }

    #[test]
    fn separates_two_rallies_with_dead_time_and_noise_floor() {
        let sample_rate = 16_000u32;
        let duration_sec = 18.0f64;
        let sample_count = (sample_rate as f64 * duration_sec) as usize;
        let mut samples = vec![0.0f32; sample_count];

        inject_room_noise(&mut samples, sample_rate, 0.025);
        inject_crowd_bed(&mut samples, sample_rate, duration_sec, 0.015);

        for hit_time in [1.2, 1.95, 2.7, 3.45, 4.2] {
            inject_pulse(&mut samples, sample_rate, hit_time, 0.95);
        }
        for hit_time in [10.1, 10.8, 11.55, 12.25, 13.0, 13.7] {
            inject_pulse(&mut samples, sample_rate, hit_time, 0.88);
        }

        let features = extract_features(&samples, sample_rate, 0.55);
        let segments = build_segments(&features.hit_times_sec, &features.hit_strengths, duration_sec, 0.55);

        assert_eq!(segments.len(), 2, "expected 2 rally segments, got {segments:#?}");
        assert!(segments[0].end_sec < 7.0, "first segment too long: {:?}", segments[0]);
        assert!(segments[1].start_sec > 9.0, "second segment starts too early: {:?}", segments[1]);
    }

    #[test]
    fn ignores_low_level_background_without_hits() {
        let sample_rate = 16_000u32;
        let duration_sec = 10.0f64;
        let sample_count = (sample_rate as f64 * duration_sec) as usize;
        let mut samples = vec![0.0f32; sample_count];

        inject_room_noise(&mut samples, sample_rate, 0.03);
        inject_crowd_bed(&mut samples, sample_rate, duration_sec, 0.02);

        let features = extract_features(&samples, sample_rate, 0.55);
        let segments = build_segments(&features.hit_times_sec, &features.hit_strengths, duration_sec, 0.55);

        assert!(features.hit_times_sec.len() <= 1, "expected near-zero false hits, got {}", features.hit_times_sec.len());
        assert!(segments.is_empty(), "expected no segments, got {segments:#?}");
    }

    fn inject_pulse(samples: &mut [f32], sample_rate: u32, time_sec: f64, amplitude: f32) {
        let center = (time_sec * sample_rate as f64) as isize;
        let half_width = (sample_rate as f64 * 0.012) as isize;

        for offset in -half_width..=half_width {
            let index = center + offset;
            if index < 0 || index as usize >= samples.len() {
                continue;
            }

            let distance = (offset as f32 / half_width.max(1) as f32).abs();
            let envelope = (1.0 - distance).max(0.0);
            let ring = ((offset as f32) * 0.35).sin().abs().max(0.2);
            samples[index as usize] += amplitude * envelope * ring;
        }
    }

    fn inject_room_noise(samples: &mut [f32], sample_rate: u32, amplitude: f32) {
        let mut seed = 0x1234_5678u32;
        for (index, sample) in samples.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let white = ((seed >> 8) as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let envelope = 0.75 + 0.25 * ((index as f32 / sample_rate as f32) * 0.9).sin();
            *sample += white * amplitude * envelope;
        }
    }

    fn inject_crowd_bed(samples: &mut [f32], sample_rate: u32, duration_sec: f64, amplitude: f32) {
        let _ = duration_sec;
        for (index, sample) in samples.iter_mut().enumerate() {
            let time = index as f32 / sample_rate as f32;
            let low = (time * 2.0 * std::f32::consts::PI * 180.0).sin();
            let high = (time * 2.0 * std::f32::consts::PI * 420.0).sin();
            *sample += (low * 0.7 + high * 0.3) * amplitude;
        }
    }
}
