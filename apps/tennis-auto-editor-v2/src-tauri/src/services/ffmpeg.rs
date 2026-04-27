use crate::models::project::{HardwareEncoderSupportRecord, HardwareExportSupportRecord};
use std::{
    env,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportResolution {
    P360,
    P480,
    P720,
    P1080,
    K4,
}

impl ExportResolution {
    fn target_height(self) -> u32 {
        match self {
            Self::P360 => 360,
            Self::P480 => 480,
            Self::P720 => 720,
            Self::P1080 => 1080,
            Self::K4 => 2160,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::P360 => "360p",
            Self::P480 => "480p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
            Self::K4 => "4k",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Mp4,
    Mov,
    Webm,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Webm => "webm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportProfile {
    Fast,
    Hd,
    K4,
}

impl ExportProfile {
    pub fn resolution(self) -> ExportResolution {
        match self {
            Self::Fast => ExportResolution::P720,
            Self::Hd => ExportResolution::P1080,
            Self::K4 => ExportResolution::K4,
        }
    }

    fn cpu_preset(self) -> &'static str {
        match self {
            Self::Fast => "superfast",
            Self::Hd | Self::K4 => "veryfast",
        }
    }

    fn cpu_crf(self) -> u8 {
        match self {
            Self::Fast => 24,
            Self::Hd => 20,
            Self::K4 => 20,
        }
    }

    fn target_fps(self, source_fps: f64) -> f64 {
        let normalized = normalize_export_fps(source_fps);
        match self {
            Self::Fast => normalized.min(30.0),
            Self::Hd => normalized.min(60.0),
            Self::K4 => normalized.min(60.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportSettings {
    pub profile: ExportProfile,
    pub format: ExportFormat,
    pub hardware_encode: bool,
}

impl ExportSettings {
    pub fn new(profile: ExportProfile, format: ExportFormat, hardware_encode: bool) -> Self {
        Self {
            profile,
            format,
            hardware_encode,
        }
    }
}

pub fn generate_proxy_video_with_progress<F>(
    input_path: &Path,
    output_path: &Path,
    duration_sec: f64,
    on_progress: F,
) -> Result<(), String>
where
    F: FnMut(f64),
{
    run_ffmpeg_with_progress(
        &[
            "-y".to_string(),
            "-i".to_string(),
            input_path.display().to_string(),
            "-vf".to_string(),
            "scale=-2:540".to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-crf".to_string(),
            "28".to_string(),
            "-an".to_string(),
            output_path.display().to_string(),
        ],
        duration_sec,
        on_progress,
    )
}

pub fn extract_audio_wav_with_progress<F>(
    input_path: &Path,
    output_path: &Path,
    duration_sec: f64,
    on_progress: F,
) -> Result<(), String>
where
    F: FnMut(f64),
{
    run_ffmpeg_with_progress(
        &[
            "-y".to_string(),
            "-i".to_string(),
            input_path.display().to_string(),
            "-vn".to_string(),
            "-ac".to_string(),
            "1".to_string(),
            "-ar".to_string(),
            "16000".to_string(),
            "-c:a".to_string(),
            "pcm_s16le".to_string(),
            output_path.display().to_string(),
        ],
        duration_sec,
        on_progress,
    )
}

pub fn generate_silent_audio(duration_sec: f64, output_path: &Path) -> Result<(), String> {
    run_ffmpeg(&[
        "-y".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        "anullsrc=r=16000:cl=mono".to_string(),
        "-t".to_string(),
        format!("{:.3}", duration_sec.max(0.1)),
        "-c:a".to_string(),
        "pcm_s16le".to_string(),
        output_path.display().to_string(),
    ])
}

pub fn export_highlight_video(
    input_path: &Path,
    output_path: &Path,
    segments: &[(f64, f64)],
    clip_labels: &[String],
    source_fps: f64,
    has_audio: bool,
    settings: ExportSettings,
) -> Result<(), String> {
    export_highlight_video_with_progress(
        input_path,
        output_path,
        segments,
        clip_labels,
        source_fps,
        has_audio,
        settings,
        |_| {},
    )
}

pub fn export_highlight_video_with_progress<F>(
    input_path: &Path,
    output_path: &Path,
    segments: &[(f64, f64)],
    clip_labels: &[String],
    source_fps: f64,
    has_audio: bool,
    settings: ExportSettings,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(f64),
{
    let normalized_segments = normalize_segments(segments);
    if normalized_segments.is_empty() {
        return Err("没有可导出的 keep 片段，请先在 M4 中至少保留一个片段".to_string());
    }

    let normalized_fps = settings.profile.target_fps(source_fps);
    let filter_complex = build_export_filter_complex(
        &normalized_segments,
        clip_labels,
        normalized_fps,
        settings.profile.resolution(),
        has_audio,
    );
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input_path.display().to_string(),
        "-filter_complex".to_string(),
        filter_complex,
        "-map".to_string(),
        "[outv]".to_string(),
        "-vsync".to_string(),
        "cfr".to_string(),
        "-r".to_string(),
        format_fps_value(normalized_fps),
    ];

    match settings.format {
        ExportFormat::Mp4 | ExportFormat::Mov => {
            args.extend([
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                settings.profile.cpu_preset().to_string(),
                "-crf".to_string(),
                settings.profile.cpu_crf().to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
        ExportFormat::Webm => {
            args.extend([
                "-c:v".to_string(),
                "libvpx-vp9".to_string(),
                "-row-mt".to_string(),
                "1".to_string(),
                "-b:v".to_string(),
                "0".to_string(),
                "-crf".to_string(),
                "32".to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
            ]);
        }
    }

    if has_audio {
        args.extend(["-map".to_string(), "[outa]".to_string()]);

        match settings.format {
            ExportFormat::Mp4 | ExportFormat::Mov => {
                args.extend([
                    "-c:a".to_string(),
                    "aac".to_string(),
                    "-b:a".to_string(),
                    "160k".to_string(),
                ]);
            }
            ExportFormat::Webm => {
                args.extend([
                    "-c:a".to_string(),
                    "libopus".to_string(),
                    "-b:a".to_string(),
                    "128k".to_string(),
                ]);
            }
        }
    } else {
        args.push("-an".to_string());
    }

    args.push(output_path.display().to_string());
    let output_duration_sec: f64 = normalized_segments
        .iter()
        .map(|(start_sec, end_sec)| end_sec - start_sec)
        .sum();

    if settings.hardware_encode {
        for encoder in hardware_encoders_for_current_platform() {
            if !ffmpeg_encoder_is_available(encoder) {
                continue;
            }
            let hardware_args = build_hardware_export_args(&args, settings, encoder, output_path);
            if run_ffmpeg_with_progress(&hardware_args, output_duration_sec, |progress| on_progress(progress)).is_ok() {
                return Ok(());
            }
        }
    }

    run_ffmpeg_with_progress(&args, output_duration_sec, on_progress)
}

fn ffmpeg_encoder_is_available(encoder: &str) -> bool {
    let ffmpeg_binary =
        env::var("TENNIS_AUTO_EDITOR_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_string());

    let output = Command::new(&ffmpeg_binary)
        .args(["-hide_banner", "-encoders"])
        .output();

    match output {
        Ok(value) if value.status.success() => {
            let text = String::from_utf8_lossy(&value.stdout);
            text.lines().any(|line| line.contains(encoder))
        }
        _ => false,
    }
}

fn hardware_encoders_for_current_platform() -> &'static [&'static str] {
    match env::consts::OS {
        "macos" => &["h264_videotoolbox"],
        "windows" => &["h264_nvenc", "h264_qsv", "h264_amf"],
        _ => &["h264_vaapi", "h264_nvenc", "h264_qsv"],
    }
}

pub fn hardware_export_support() -> HardwareExportSupportRecord {
    let encoders = hardware_encoders_for_current_platform()
        .iter()
        .map(|key| HardwareEncoderSupportRecord {
            key: (*key).to_string(),
            label: hardware_encoder_label(key).to_string(),
            available: hardware_encoder_is_ready(key),
        })
        .collect::<Vec<_>>();

    let recommended = encoders.iter().find(|item| item.available);
    let summary = if let Some(encoder) = recommended {
        format!("已检测到 {}，勾选后会优先尝试硬件加速导出", encoder.label)
    } else {
        "未检测到可用硬件编码器，勾选后也会自动回退普通导出".to_string()
    };

    HardwareExportSupportRecord {
        available: recommended.is_some(),
        recommended_key: recommended.map(|item| item.key.clone()),
        summary,
        encoders,
    }
}

fn hardware_encoder_is_ready(encoder: &str) -> bool {
    if !ffmpeg_encoder_is_available(encoder) {
        return false;
    }

    if encoder == "h264_vaapi" {
        return find_vaapi_device().is_some();
    }

    true
}

fn hardware_encoder_label(encoder: &str) -> &'static str {
    match encoder {
        "h264_videotoolbox" => "VideoToolbox（macOS）",
        "h264_nvenc" => "NVENC（NVIDIA）",
        "h264_qsv" => "QSV（Intel）",
        "h264_amf" => "AMF（AMD）",
        "h264_vaapi" => "VAAPI（Linux AMD/Intel）",
        _ => "硬件编码",
    }
}

fn build_hardware_export_args(
    cpu_args: &[String],
    settings: ExportSettings,
    encoder: &str,
    output_path: &Path,
) -> Vec<String> {
    let mut args = Vec::new();
    let mut skip_next = false;
    let output_path_string = output_path.display().to_string();

    for arg in cpu_args {
        if skip_next {
            skip_next = false;
            continue;
        }

        if arg == "-c:v"
            || arg == "-preset"
            || arg == "-crf"
            || arg == "-pix_fmt"
            || arg == "-movflags"
        {
            skip_next = true;
            continue;
        }

        if arg == "-global_quality" || arg == "-look_ahead" || arg == "-cq" || arg == "-b:v" || arg == "-qp" || arg == "-quality" {
            skip_next = true;
            continue;
        }

        if arg == &output_path_string {
            continue;
        }

        args.push(arg.clone());
    }

    if encoder == "h264_vaapi" {
        if let Some(filter_index) = args.iter().position(|arg| arg == "-filter_complex") {
            if let Some(filter_value) = args.get_mut(filter_index + 1) {
                filter_value.push_str(";[outv]format=nv12,hwupload[outv_hw]");
            }
        }

        if let Some(map_index) = args.iter().position(|arg| arg == "-map") {
            if let Some(map_value) = args.get_mut(map_index + 1) {
                if map_value == "[outv]" {
                    *map_value = "[outv_hw]".to_string();
                }
            }
        }
    }

    match encoder {
        "h264_videotoolbox" => {
            args.extend([
                "-c:v".to_string(),
                "h264_videotoolbox".to_string(),
                "-b:v".to_string(),
                hardware_target_bitrate(settings.profile).to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
        "h264_qsv" => {
            args.extend([
                "-c:v".to_string(),
                "h264_qsv".to_string(),
                "-global_quality".to_string(),
                hardware_quality_value(settings.profile).to_string(),
                "-look_ahead".to_string(),
                "0".to_string(),
                "-pix_fmt".to_string(),
                "nv12".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
        "h264_amf" => {
            args.extend([
                "-c:v".to_string(),
                "h264_amf".to_string(),
                "-quality".to_string(),
                if settings.profile == ExportProfile::Fast {
                    "speed".to_string()
                } else {
                    "balanced".to_string()
                },
                "-b:v".to_string(),
                hardware_target_bitrate(settings.profile).to_string(),
                "-pix_fmt".to_string(),
                "nv12".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
        "h264_vaapi" => {
            if let Some(device_path) = find_vaapi_device() {
                args.extend([
                    "-vaapi_device".to_string(),
                    device_path.display().to_string(),
                    "-c:v".to_string(),
                    "h264_vaapi".to_string(),
                    "-qp".to_string(),
                    hardware_quality_value(settings.profile).to_string(),
                    "-movflags".to_string(),
                    "+faststart".to_string(),
                ]);
            }
        }
        _ => {
            args.extend([
                "-c:v".to_string(),
                encoder.to_string(),
                "-preset".to_string(),
                "p4".to_string(),
                "-cq".to_string(),
                hardware_quality_value(settings.profile).to_string(),
                "-b:v".to_string(),
                "0".to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ]);
        }
    }

    args.push(output_path.display().to_string());
    args
}

fn find_vaapi_device() -> Option<PathBuf> {
    ["/dev/dri/renderD128", "/dev/dri/renderD129", "/dev/dri/card0"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn hardware_target_bitrate(profile: ExportProfile) -> &'static str {
    match profile {
        ExportProfile::Fast => "4500k",
        ExportProfile::Hd => "8000k",
        ExportProfile::K4 => "18000k",
    }
}

fn hardware_quality_value(profile: ExportProfile) -> &'static str {
    match profile {
        ExportProfile::Fast => "24",
        ExportProfile::Hd => "20",
        ExportProfile::K4 => "18",
    }
}

fn normalize_segments(segments: &[(f64, f64)]) -> Vec<(f64, f64)> {
    segments
        .iter()
        .filter_map(|(start_sec, end_sec)| {
            let start = start_sec.max(0.0);
            let end = end_sec.max(0.0);
            if end - start < 0.05 {
                None
            } else {
                Some((start, end))
            }
        })
        .collect()
}

fn normalize_export_fps(source_fps: f64) -> f64 {
    if source_fps.is_finite() && source_fps >= 15.0 && source_fps <= 120.0 {
        source_fps
    } else {
        30.0
    }
}

fn format_fps_value(value: f64) -> String {
    format!("{:.3}", value)
}

fn build_export_filter_complex(
    segments: &[(f64, f64)],
    _clip_labels: &[String],
    target_fps: f64,
    export_resolution: ExportResolution,
    has_audio: bool,
) -> String {
    let mut parts = Vec::new();
    let fps_value = format_fps_value(target_fps);
    let target_height = export_resolution.target_height();

    for (index, (start_sec, end_sec)) in segments.iter().enumerate() {
        parts.push(format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS,settb=AVTB,fps={},scale=-2:{},format=yuv420p[v{}]",
            start_sec, end_sec, fps_value, target_height, index
        ));

        if has_audio {
            parts.push(format!(
                "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS,aresample=async=1:first_pts=0[a{}]",
                start_sec, end_sec, index
            ));
        }
    }

    if has_audio {
        let concat_inputs = (0..segments.len())
            .map(|index| format!("[v{index}][a{index}]"))
            .collect::<String>();
        parts.push(format!(
            "{}concat=n={}:v=1:a=1[vcat][acat]",
            concat_inputs,
            segments.len()
        ));
        parts.push(format!("[vcat]fps={},format=yuv420p[outv]", fps_value));
        parts.push("[acat]aresample=async=1:first_pts=0[outa]".to_string());
    } else {
        let concat_inputs = (0..segments.len())
            .map(|index| format!("[v{index}]"))
            .collect::<String>();
        parts.push(format!(
            "{}concat=n={}:v=1:a=0[vcat]",
            concat_inputs,
            segments.len()
        ));
        parts.push(format!("[vcat]fps={},format=yuv420p[outv]", fps_value));
    }

    parts.join(";")
}

pub fn extract_video_thumbnail_data_url(input_path: &Path, seek_sec: f64) -> Result<String, String> {
    let ffmpeg_binary =
        env::var("TENNIS_AUTO_EDITOR_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_string());

    let args = vec![
        "-y".to_string(),
        "-ss".to_string(),
        format!("{:.3}", seek_sec.max(0.0)),
        "-i".to_string(),
        input_path.display().to_string(),
        "-frames:v".to_string(),
        "1".to_string(),
        "-vf".to_string(),
        "scale=min(960\\,iw):-2".to_string(),
        "-q:v".to_string(),
        "3".to_string(),
        "-f".to_string(),
        "image2pipe".to_string(),
        "-vcodec".to_string(),
        "mjpeg".to_string(),
        "pipe:1".to_string(),
    ];

    let output = Command::new(&ffmpeg_binary)
        .args(&args)
        .output()
        .map_err(|error| format!("调用 ffmpeg 失败：安装包内置 ffmpeg 缺失或无法执行，且系统 PATH 中也未找到 ffmpeg: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("ffmpeg 未返回可读错误信息")
            .to_string();
        return Err(format!("提取视频预览图失败: {summary}"));
    }

    if output.stdout.is_empty() {
        return Err("提取视频预览图失败: ffmpeg 没有输出图片数据".to_string());
    }

    Ok(format!(
        "data:image/jpeg;base64,{}",
        BASE64_STANDARD.encode(output.stdout)
    ))
}

fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let ffmpeg_binary =
        env::var("TENNIS_AUTO_EDITOR_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_string());

    let output = Command::new(&ffmpeg_binary)
        .args(args)
        .output()
        .map_err(|error| format!("调用 ffmpeg 失败：安装包内置 ffmpeg 缺失或无法执行，且系统 PATH 中也未找到 ffmpeg: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("ffmpeg 未返回可读错误信息")
        .to_string();

    Err(format!("ffmpeg 执行失败: {summary}"))
}

fn run_ffmpeg_with_progress<F>(
    args: &[String],
    duration_sec: f64,
    mut on_progress: F,
) -> Result<(), String>
where
    F: FnMut(f64),
{
    let ffmpeg_binary =
        env::var("TENNIS_AUTO_EDITOR_FFMPEG_BIN").unwrap_or_else(|_| "ffmpeg".to_string());

    let mut full_args = vec![
        "-progress".to_string(),
        "pipe:1".to_string(),
        "-nostats".to_string(),
    ];
    full_args.extend(args.iter().cloned());

    let mut child = Command::new(&ffmpeg_binary)
        .args(&full_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("调用 ffmpeg 失败：安装包内置 ffmpeg 缺失或无法执行，且系统 PATH 中也未找到 ffmpeg: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 ffmpeg progress 输出".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 ffmpeg stderr 输出".to_string())?;

    let reader = BufReader::new(stdout);
    let mut last_percent = 0u8;

    for line_result in reader.lines() {
        let line = line_result.map_err(|error| format!("读取 ffmpeg progress 失败: {error}"))?;
        if let Some(value) = line.strip_prefix("out_time=") {
            if let Some(current_sec) = parse_ffmpeg_time(value.trim()) {
                let progress = if duration_sec > 0.0 {
                    (current_sec / duration_sec).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let percent = (progress * 100.0).round() as u8;
                if percent > last_percent {
                    last_percent = percent;
                    on_progress(progress);
                }
            }
        } else if line.trim() == "progress=end" {
            on_progress(1.0);
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("等待 ffmpeg 结束失败: {error}"))?;

    let mut stderr_output = String::new();
    stderr
        .read_to_string(&mut stderr_output)
        .map_err(|error| format!("读取 ffmpeg stderr 失败: {error}"))?;

    if status.success() {
        return Ok(());
    }

    let summary = stderr_output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("ffmpeg 未返回可读错误信息")
        .to_string();

    Err(format!("ffmpeg 执行失败: {summary}"))
}

fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<f64>().ok()?;
    let seconds = parts.next()?.parse::<f64>().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::{
        build_export_filter_complex, parse_ffmpeg_time, ExportResolution,
    };

    #[test]
    fn build_filter_complex_with_audio() {
        let filter = build_export_filter_complex(
            &[(1.0, 3.0), (5.0, 8.5)],
            &["segment-001".to_string(), "segment-002".to_string()],
            30.0,
            ExportResolution::P720,
            true,
        );
        assert!(filter.contains("[0:v]trim=start=1.000:end=3.000,setpts=PTS-STARTPTS,settb=AVTB,fps=30.000,scale=-2:720,format=yuv420p[v0]"));
        assert!(filter.contains("[0:a]atrim=start=5.000:end=8.500,asetpts=PTS-STARTPTS,aresample=async=1:first_pts=0[a1]"));
        assert!(filter.contains("[vcat]fps=30.000,format=yuv420p[outv]"));
        assert!(filter.ends_with("[acat]aresample=async=1:first_pts=0[outa]"));
    }

    #[test]
    fn build_filter_complex_without_audio() {
        let filter = build_export_filter_complex(
            &[(0.2, 2.4)],
            &["segment-001".to_string()],
            25.0,
            ExportResolution::P1080,
            false,
        );
        assert!(filter.contains("[0:v]trim=start=0.200:end=2.400,setpts=PTS-STARTPTS,settb=AVTB,fps=25.000,scale=-2:1080,format=yuv420p[v0]"));
        assert!(filter.ends_with("[vcat]fps=25.000,format=yuv420p[outv]"));
    }

    #[test]
    fn parse_ffmpeg_time_supports_fractional_seconds() {
        assert_eq!(parse_ffmpeg_time("00:01:02.500"), Some(62.5));
    }
}
