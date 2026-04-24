use std::{env, fs, path::PathBuf, process::Command};

use serde_json::json;

mod models {
    pub mod project {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/models/project.rs"));
    }
}

mod services {
    pub mod ffmpeg {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/services/ffmpeg.rs"));
    }

    pub mod probe {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/services/probe.rs"));
    }
}

use models::project::AnalysisResultRecord;
use services::{ffmpeg, probe};

fn main() -> Result<(), String> {
    let source_video = env::var("SOURCE_VIDEO").map(PathBuf::from).map_err(|_| {
        "Please set SOURCE_VIDEO=/path/to/source.mp4 when running this smoke test".to_string()
    })?;
    let start_sec = env::var("SMOKE_START_SEC").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let duration_sec = env::var("SMOKE_DURATION_SEC").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(45.0);

    if !source_video.exists() {
        return Err(format!("Source video not found: {}", source_video.display()));
    }

    let work_dir = env::temp_dir().join("tennis-auto-editor-v2-smoke");
    if work_dir.exists() {
        fs::remove_dir_all(&work_dir).map_err(|e| format!("failed to clean temp dir {}: {e}", work_dir.display()))?;
    }
    fs::create_dir_all(&work_dir).map_err(|e| format!("failed to create temp dir {}: {e}", work_dir.display()))?;

    let sample_video = work_dir.join("sample.mp4");
    run_ffmpeg(&[
        "-y",
        "-ss",
        &format!("{start_sec:.3}"),
        "-t",
        &format!("{duration_sec:.3}"),
        "-i",
        &source_video.display().to_string(),
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-crf",
        "30",
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        &sample_video.display().to_string(),
    ])?;

    let probe_info = probe::probe_video(&sample_video)?;
    let project_json = work_dir.join("project.json");
    fs::write(
        &project_json,
        serde_json::to_string_pretty(&json!({
            "project_id": "smoke-auto",
            "source_video_path": source_video.display().to_string(),
            "created_at": chrono::Utc::now().to_rfc3339(),
            "status": "created",
            "title": "smoke-auto",
            "probe_path": "probe.json",
            "proxy_path": sample_video.display().to_string(),
            "audio_path": "audio.wav",
            "analysis_result_path": "analysis_result.json"
        }))
        .map_err(|e| format!("failed to serialize project.json: {e}"))?,
    )
    .map_err(|e| format!("failed to write {}: {e}", project_json.display()))?;

    let audio_path = work_dir.join("audio.wav");
    ffmpeg::extract_audio_wav_with_progress(&sample_video, &audio_path, probe_info.duration_sec, |_| {})?;

    let python_root = resolve_python_root()?;
    let analyzer_script = python_root.join("analyzer/main.py");
    let vendor_dir = python_root.join("vendor");
    let python_path = format!("{}:{}", vendor_dir.display(), python_root.display());

    let analyzer_status = Command::new("python3")
        .arg(&analyzer_script)
        .arg("--project-dir")
        .arg(&work_dir)
        .arg("--proxy-path")
        .arg(&sample_video)
        .arg("--audio-path")
        .arg(&audio_path)
        .arg("--sensitivity")
        .arg("0.55")
        .env("PYTHONPATH", python_path)
        .status()
        .map_err(|e| format!("failed to start analyzer: {e}"))?;

    if !analyzer_status.success() {
        return Err(format!("analyzer failed with status {analyzer_status}"));
    }

    let analysis_path = work_dir.join("analysis_result.json");
    let analysis: AnalysisResultRecord = serde_json::from_str(
        &fs::read_to_string(&analysis_path)
            .map_err(|e| format!("failed to read {}: {e}", analysis_path.display()))?,
    )
    .map_err(|e| format!("failed to parse analysis_result.json: {e}"))?;

    if analysis.segments.is_empty() {
        return Err("analysis completed but found 0 segments".to_string());
    }

    let keep_segments: Vec<(f64, f64)> = analysis
        .segments
        .iter()
        .map(|segment| (segment.start_sec, segment.end_sec))
        .collect();
    let clip_labels: Vec<String> = analysis
        .segments
        .iter()
        .map(|segment| segment.segment_id.clone())
        .collect();

    let output_path = work_dir.join("highlights.mp4");
    ffmpeg::export_highlight_video(
        &sample_video,
        &output_path,
        &keep_segments,
        &clip_labels,
        probe_info.fps,
        probe_info.has_audio,
        ffmpeg::ExportSettings::new(ffmpeg::ExportResolution::P720, ffmpeg::ExportFormat::Mp4),
    )?;

    let output_size = fs::metadata(&output_path)
        .map_err(|e| format!("failed to stat output {}: {e}", output_path.display()))?
        .len();

    println!("SMOKE_OK sample={} segments={} output={} bytes={} workdir={}", sample_video.display(), analysis.summary.segment_count, output_path.display(), output_size, work_dir.display());
    Ok(())
}

fn resolve_python_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../python"),
        manifest_dir.join("../../tennis-auto-editor/python"),
    ];

    for candidate in candidates {
        if candidate.join("analyzer/main.py").exists() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "Could not find python analyzer root near {}",
        manifest_dir.display()
    ))
}

fn run_ffmpeg(args: &[&str]) -> Result<(), String> {
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&output.stderr).to_string())
}
