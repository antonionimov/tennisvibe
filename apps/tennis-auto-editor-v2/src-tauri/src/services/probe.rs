use crate::models::project::ProbeRecord;
use serde::Deserialize;
use std::{env, path::Path, process::Command};

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

pub fn resolved_ffprobe_bin() -> String {
    env::var("TENNIS_AUTO_EDITOR_FFPROBE_BIN").unwrap_or_else(|_| "ffprobe".to_string())
}

pub fn ffprobe_is_available() -> bool {
    Command::new(resolved_ffprobe_bin())
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn probe_video(video_path: &Path) -> Result<ProbeRecord, String> {
    let ffprobe_binary = resolved_ffprobe_bin();

    let output = Command::new(&ffprobe_binary)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(video_path)
        .output()
        .map_err(|error| format!("调用 ffprobe 失败：安装包内置 ffprobe 缺失或无法执行，且系统 PATH 中也未找到 ffprobe: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "ffprobe 未返回可读错误信息".to_string()
        } else {
            stderr
        };

        return Err(format!("ffprobe 读取视频失败: {message}"));
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("解析 ffprobe 输出失败: {error}"))?;

    let video_stream = parsed
        .streams
        .iter()
        .find(|stream| matches!(stream.codec_type.as_deref(), Some("video")))
        .ok_or_else(|| "ffprobe 未找到视频流".to_string())?;

    let audio_stream = parsed
        .streams
        .iter()
        .find(|stream| matches!(stream.codec_type.as_deref(), Some("audio")));

    let duration_sec = parsed
        .format
        .and_then(|format| format.duration)
        .and_then(|duration| duration.parse::<f64>().ok())
        .unwrap_or(0.0);

    Ok(ProbeRecord {
        duration_sec,
        width: video_stream.width.unwrap_or_default(),
        height: video_stream.height.unwrap_or_default(),
        fps: parse_frame_rate(
            video_stream.avg_frame_rate.as_deref().unwrap_or("0/0"),
            video_stream.r_frame_rate.as_deref().unwrap_or("0/0"),
        ),
        has_audio: audio_stream.is_some(),
        video_codec: video_stream.codec_name.clone(),
        audio_codec: audio_stream.and_then(|stream| stream.codec_name.clone()),
    })
}

fn parse_frame_rate(avg_frame_rate: &str, fallback_frame_rate: &str) -> f64 {
    parse_ratio(avg_frame_rate)
        .or_else(|| parse_ratio(fallback_frame_rate))
        .unwrap_or(0.0)
}

fn parse_ratio(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;

    if denominator == 0.0 {
        return None;
    }

    Some(numerator / denominator)
}
