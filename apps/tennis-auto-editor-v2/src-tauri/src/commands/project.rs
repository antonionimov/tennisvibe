use crate::{
    models::project::{
        AnalysisProgressEvent, AnalysisResultRecord, AnalysisRunResult, AutoflowProgressEvent,
        CopyFileResult, ExportClipMapEntry, ExportProgressEvent, ExportVideoResult,
        HardwareExportSupportRecord, PointAnnotationDocumentRecord, PointAnnotationRecord,
        PrepareAutomaticHighlightsResult, ProbeRecord, ProjectDetail, ProjectRecord,
        ProxyGenerationResult, ProxyProgressEvent, ReviewDecisionRecord, ReviewResultRecord,
        ReviewSaveResult, ReviewSummaryRecord, RuntimeCapabilitiesRecord,
    },
    services::{analyzer, ffmpeg, mpv::MpvController, probe, workspace},
};
use chrono::{Local, Utc};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;
use uuid::Uuid;

#[tauri::command]
pub fn create_project(app: AppHandle, video_path: String) -> Result<ProjectDetail, String> {
    let source_video_path = normalize_video_path(&video_path)?;
    let project_id = Uuid::new_v4().to_string();
    let project_dir = workspace::ensure_project_dir(&app, &project_id)?;
    let probe = probe::probe_video(&source_video_path)?;

    let project = ProjectRecord {
        project_id: project_id.clone(),
        source_video_path: source_video_path.display().to_string(),
        created_at: Utc::now().to_rfc3339(),
        status: "created".to_string(),
        title: workspace::build_title_from_path(&source_video_path),
        probe_path: "probe.json".to_string(),
        proxy_path: "proxy.mp4".to_string(),
        audio_path: "audio.wav".to_string(),
        analysis_result_path: "analysis_result.json".to_string(),
    };

    workspace::write_json(&project_dir.join("project.json"), &project)?;
    workspace::write_json(&project_dir.join("probe.json"), &probe)?;

    Ok(ProjectDetail {
        project,
        probe,
        project_dir: project_dir.display().to_string(),
    })
}

#[tauri::command]
pub fn get_project(app: AppHandle, project_id: String) -> Result<ProjectDetail, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let probe: ProbeRecord = workspace::read_json(&project_dir.join("probe.json"))?;

    Ok(ProjectDetail {
        project,
        probe,
        project_dir: project_dir.display().to_string(),
    })
}

#[tauri::command]
pub fn get_latest_project(app: AppHandle) -> Result<Option<ProjectDetail>, String> {
    let Some(project_dir) = workspace::latest_project_dir(&app)? else {
        return Ok(None);
    };

    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let probe: ProbeRecord = workspace::read_json(&project_dir.join("probe.json"))?;

    Ok(Some(ProjectDetail {
        project,
        probe,
        project_dir: project_dir.display().to_string(),
    }))
}

#[tauri::command]
pub fn extract_video_thumbnail(video_path: String) -> Result<String, String> {
    let source_video_path = normalize_video_path(&video_path)?;
    let probe = probe::probe_video(&source_video_path)?;
    let seek_sec = thumbnail_seek_seconds(probe.duration_sec);
    ffmpeg::extract_video_thumbnail_data_url(&source_video_path, seek_sec)
}

#[tauri::command]
pub fn get_hardware_export_support() -> Result<HardwareExportSupportRecord, String> {
    Ok(ffmpeg::hardware_export_support())
}

#[tauri::command]
pub fn get_runtime_capabilities(app: AppHandle) -> Result<RuntimeCapabilitiesRecord, String> {
    let export_directory = workspace::exports_root(&app)?.display().to_string();
    let runtime_root = workspace::bundled_runtime_root(&app);
    let ffmpeg_bin = ffmpeg::resolved_ffmpeg_bin();
    let ffprobe_bin = probe::resolved_ffprobe_bin();
    let ffmpeg_available = ffmpeg::ffmpeg_is_available();
    let ffprobe_available = probe::ffprobe_is_available();

    Ok(RuntimeCapabilitiesRecord {
        platform: std::env::consts::OS.to_string(),
        is_mobile: cfg!(mobile),
        supports_save_dialog: !cfg!(target_os = "android"),
        prefers_generated_export_path: cfg!(target_os = "android"),
        export_directory,
        import_mode: if cfg!(target_os = "android") {
            "uri-or-path".to_string()
        } else {
            "path".to_string()
        },
        analyzer_backend: "rust-audio-mvp".to_string(),
        runtime_root: runtime_root.as_ref().map(|path| path.display().to_string()),
        runtime_source: if runtime_root.is_some() {
            if cfg!(target_os = "android") {
                "bundled-mobile-runtime".to_string()
            } else {
                "bundled-runtime".to_string()
            }
        } else {
            "system-path".to_string()
        },
        ffmpeg_bin,
        ffprobe_bin,
        ffmpeg_available,
        ffprobe_available,
        media_pipeline_ready: ffmpeg_available && ffprobe_available,
    })
}

#[tauri::command]
pub fn suggest_export_path(
    app: AppHandle,
    default_file_name: String,
) -> Result<String, String> {
    let exports_dir = workspace::exports_root(&app)?;
    let file_name = sanitize_export_file_name(&default_file_name);
    Ok(exports_dir.join(file_name).display().to_string())
}

#[tauri::command]
pub fn resolve_imported_app_path(app: AppHandle, relative_path: String) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法获取 app data 目录: {error}"))?;

    let safe_relative = PathBuf::from(&relative_path);
    if safe_relative.is_absolute() || relative_path.contains("..") {
        return Err(format!("导入目标路径不安全: {relative_path}"));
    }

    Ok(app_data_dir.join(safe_relative).display().to_string())
}

#[tauri::command]
pub async fn prepare_automatic_highlights(
    app: AppHandle,
    video_path: String,
) -> Result<PrepareAutomaticHighlightsResult, String> {
    tauri::async_runtime::spawn_blocking(move || prepare_automatic_highlights_sync(app, video_path))
        .await
        .map_err(|error| format!("自动剪辑任务异常终止: {error}"))?
}

fn prepare_automatic_highlights_sync(
    app: AppHandle,
    video_path: String,
) -> Result<PrepareAutomaticHighlightsResult, String> {
    let project = create_project(app.clone(), video_path)?;
    let project_id = project.project.project_id.clone();

    emit_autoflow_progress(
        &app,
        &project_id,
        "project_created",
        5,
        "已创建项目，准备提取音频",
    )?;

    generate_proxy_sync(app.clone(), project_id.clone())?;

    emit_autoflow_progress(
        &app,
        &project_id,
        "audio_ready",
        35,
        "音频提取完成，开始分析精彩片段",
    )?;

    let analysis = run_analysis_sync(
        app.clone(),
        project_id.clone(),
        0.55,
        Some("off".to_string()),
        Some("auto".to_string()),
    )?;

    if analysis.segment_count == 0 {
        return Err("没有识别到可导出的候选回合，请换一个视频再试。".to_string());
    }

    emit_autoflow_progress(
        &app,
        &project_id,
        "analysis_ready",
        92,
        &format!("分析完成，已识别 {} 个候选片段", analysis.segment_count),
    )?;

    let review = save_review_with_default_keep(app.clone(), project_id.clone())?;

    emit_autoflow_progress(
        &app,
        &project_id,
        "review_ready",
        100,
        "已自动完成片段筛选，可以直接导出视频",
    )?;

    Ok(PrepareAutomaticHighlightsResult {
        project,
        analysis,
        review,
    })
}

#[tauri::command]
pub fn copy_file_to_path(
    source_path: String,
    destination_path: String,
) -> Result<CopyFileResult, String> {
    let source = PathBuf::from(&source_path);
    let destination = PathBuf::from(&destination_path);

    if !source.exists() {
        return Err(format!("导出文件不存在: {}", source.display()));
    }

    let Some(parent_dir) = destination.parent() else {
        return Err(format!("目标路径无效: {}", destination.display()));
    };

    fs::create_dir_all(parent_dir)
        .map_err(|error| format!("无法创建目标目录 {}: {error}", parent_dir.display()))?;
    fs::copy(&source, &destination).map_err(|error| {
        format!(
            "复制导出文件失败 {} -> {}: {error}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(CopyFileResult {
        source_path,
        destination_path,
    })
}

#[tauri::command]
pub async fn generate_proxy(
    app: AppHandle,
    project_id: String,
) -> Result<ProxyGenerationResult, String> {
    tauri::async_runtime::spawn_blocking(move || generate_proxy_sync(app, project_id))
        .await
        .map_err(|error| format!("proxy 生成任务异常终止: {error}"))?
}

fn generate_proxy_sync(
    app: AppHandle,
    project_id: String,
) -> Result<ProxyGenerationResult, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let probe: ProbeRecord = workspace::read_json(&project_dir.join("probe.json"))?;
    let source_video_path = PathBuf::from(&project.source_video_path);
    let audio_output_path = project_dir.join(&project.audio_path);

    emit_proxy_progress(
        &app,
        &project_id,
        "prepare",
        5,
        "已读取项目配置，准备提取 audio.wav",
    )?;

    if probe.has_audio {
        emit_proxy_progress(
            &app,
            &project_id,
            "extract_audio",
            15,
            "开始提取 mono 16k audio.wav",
        )?;
        ffmpeg::extract_audio_wav_with_progress(
            &source_video_path,
            &audio_output_path,
            probe.duration_sec,
            |progress| {
                let current = 15 + ((progress * 80.0).round() as u8).min(80);
                let _ = emit_proxy_progress(
                    &app,
                    &project_id,
                    "extract_audio",
                    current,
                    &format!("正在提取 audio.wav ({:.0}%)", progress * 100.0),
                );
            },
        )?;
    } else {
        emit_proxy_progress(
            &app,
            &project_id,
            "generate_silent_audio",
            15,
            "源视频无音频，生成静默 wav 以保持后续流程一致",
        )?;
        ffmpeg::generate_silent_audio(probe.duration_sec, &audio_output_path)?;
    }

    emit_proxy_progress(
        &app,
        &project_id,
        "done",
        100,
        "audio.wav 已生成完成",
    )?;

    Ok(ProxyGenerationResult {
        project_id,
        proxy_path: String::new(),
        audio_path: audio_output_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn run_analysis(
    app: AppHandle,
    project_id: String,
    sensitivity: f64,
    visual_bootstrap_mode: Option<String>,
    ball_model_choice: Option<String>,
) -> Result<AnalysisRunResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_analysis_sync(
            app,
            project_id,
            sensitivity,
            visual_bootstrap_mode,
            ball_model_choice,
        )
    })
    .await
    .map_err(|error| format!("分析任务异常终止: {error}"))?
}

fn run_analysis_sync(
    app: AppHandle,
    project_id: String,
    sensitivity: f64,
    visual_bootstrap_mode: Option<String>,
    ball_model_choice: Option<String>,
) -> Result<AnalysisRunResult, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let proxy_path = project_dir.join(&project.proxy_path);
    let source_video_path = PathBuf::from(&project.source_video_path);
    let audio_path = project_dir.join(&project.audio_path);
    let analysis_result_path = project_dir.join(&project.analysis_result_path);

    if !audio_path.exists() {
        return Err(format!(
            "audio 文件不存在，请先执行 M2 生成音频: {}",
            audio_path.display()
        ));
    }

    let sensitivity = sensitivity.clamp(0.1, 1.0);
    let analysis_media_path = if proxy_path.exists() {
        proxy_path.clone()
    } else {
        source_video_path.clone()
    };

    if visual_bootstrap_mode.as_deref().unwrap_or("off") != "off" || ball_model_choice.is_some() {
        emit_analysis_progress(
            &app,
            &project_id,
            "visual_bootstrap_skipped",
            2,
            "当前使用 Rust MVP analyzer，已跳过 Python 视觉 bootstrap 参数",
        )?;
    }

    emit_analysis_progress(&app, &project_id, "queued", 0, "准备启动 Rust analyzer")?;
    emit_analysis_progress(&app, &project_id, "audio_features", 20, "正在提取音频能量与瞬态特征")?;

    let analyzer_output = analyzer::analyze_audio_project(
        &project,
        &project_dir,
        &analysis_media_path,
        &audio_path,
        sensitivity,
    )?;

    emit_analysis_progress(
        &app,
        &project_id,
        "segment_logic",
        82,
        &format!("已检测到 {} 个疑似击球声事件，正在生成回合片段", analyzer_output.hit_count),
    )?;

    workspace::write_json(&analysis_result_path, &analyzer_output.result)?;

    emit_analysis_progress(
        &app,
        &project_id,
        "done",
        100,
        &format!("分析完成，已输出 {} 个候选片段", analyzer_output.result.summary.segment_count),
    )?;

    let analysis_result: AnalysisResultRecord = workspace::read_json(&analysis_result_path)?;

    Ok(AnalysisRunResult {
        project_id,
        analysis_result_path: analysis_result_path.display().to_string(),
        segment_count: analysis_result.summary.segment_count,
        summary: analysis_result.summary,
    })
}

fn resolve_ball_model_choice(
    manifest_dir: &Path,
    ball_model_choice: Option<String>,
) -> Result<Option<String>, String> {
    let Some(choice) = ball_model_choice.map(|value| value.to_lowercase()) else {
        return Ok(None);
    };

    let python_root = resolve_python_root(manifest_dir)?;

    let model_path = match choice.as_str() {
        "auto" => return Ok(None),
        "light" => python_root.join("models/candidates/tennis_ball_light_yolo11s.pt"),
        "legacy" => python_root.join("models/tennis_vision_ball.pt"),
        "tiny" => python_root.join("models/candidates/tennis_ball_tiny_yolov12n_last.pt"),
        other => return Err(format!("不支持的 ball model 选项: {other}")),
    };

    if !model_path.exists() {
        return Err(format!("选中的 ball model 不存在: {}", model_path.display()));
    }

    Ok(Some(model_path.display().to_string()))
}

fn resolve_python_root(manifest_dir: &Path) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    if let Ok(override_root) = std::env::var("TENNIS_AUTO_EDITOR_PYTHON_ROOT") {
        candidates.push(PathBuf::from(override_root));
    }

    candidates.push(manifest_dir.join("../python"));
    candidates.push(manifest_dir.join("../../tennis-auto-editor/python"));

    for candidate in candidates {
        if candidate.join("analyzer/main.py").exists() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "未找到 Python 分析目录。可设置 TENNIS_AUTO_EDITOR_PYTHON_ROOT，或确认安装包 runtime/python / 旧项目目录存在: {}",
        manifest_dir.join("../../tennis-auto-editor/python").display()
    ))
}

fn resolve_python_bin() -> PathBuf {
    if let Ok(path) = std::env::var("TENNIS_AUTO_EDITOR_PYTHON_BIN") {
        return PathBuf::from(path);
    }

    if cfg!(target_os = "windows") {
        PathBuf::from("python")
    } else {
        PathBuf::from("python3")
    }
}

#[tauri::command]
pub fn get_analysis_result(
    app: AppHandle,
    project_id: String,
) -> Result<Option<AnalysisResultRecord>, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let analysis_result_path = project_dir.join(&project.analysis_result_path);

    if !analysis_result_path.exists() {
        return Ok(None);
    }

    let analysis_result: AnalysisResultRecord = workspace::read_json(&analysis_result_path)?;
    Ok(Some(analysis_result))
}

#[tauri::command]
pub fn get_review_result(
    app: AppHandle,
    project_id: String,
) -> Result<Option<ReviewResultRecord>, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let review_path = review_result_path(&project_dir);

    if !review_path.exists() {
        return Ok(None);
    }

    let review: ReviewResultRecord = workspace::read_json(&review_path)?;
    Ok(Some(review))
}

#[tauri::command]
pub fn get_point_annotations(
    app: AppHandle,
    project_id: String,
) -> Result<Option<PointAnnotationDocumentRecord>, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let annotation_path = point_annotations_path(&project_dir);

    if !annotation_path.exists() {
        return Ok(None);
    }

    let document: PointAnnotationDocumentRecord = workspace::read_json(&annotation_path)?;
    Ok(Some(document))
}

#[tauri::command]
pub fn ensure_annotation_player(
    app: AppHandle,
    project_id: String,
    player: State<'_, Mutex<MpvController>>,
) -> Result<String, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let proxy_path = project_dir.join(&project.proxy_path);
    let media_path = if proxy_path.exists() {
        proxy_path
    } else {
        PathBuf::from(&project.source_video_path)
    };

    let mut controller = player
        .lock()
        .map_err(|_| "无法获取 mpv 控制器锁".to_string())?;
    controller.ensure_player(&media_path)
}

#[tauri::command]
pub fn toggle_annotation_player(player: State<'_, Mutex<MpvController>>) -> Result<(), String> {
    let mut controller = player
        .lock()
        .map_err(|_| "无法获取 mpv 控制器锁".to_string())?;
    controller.toggle_pause()
}

#[tauri::command]
pub fn seek_annotation_player_by(
    delta_sec: f64,
    player: State<'_, Mutex<MpvController>>,
) -> Result<(), String> {
    let mut controller = player
        .lock()
        .map_err(|_| "无法获取 mpv 控制器锁".to_string())?;
    controller.seek_relative(delta_sec)
}

#[tauri::command]
pub fn seek_annotation_player_to(
    time_sec: f64,
    player: State<'_, Mutex<MpvController>>,
) -> Result<(), String> {
    let mut controller = player
        .lock()
        .map_err(|_| "无法获取 mpv 控制器锁".to_string())?;
    controller.seek_absolute(time_sec)
}

#[tauri::command]
pub fn get_annotation_player_time(
    player: State<'_, Mutex<MpvController>>,
) -> Result<f64, String> {
    let mut controller = player
        .lock()
        .map_err(|_| "无法获取 mpv 控制器锁".to_string())?;
    controller.current_time()
}

#[tauri::command]
pub fn save_point_annotations(
    app: AppHandle,
    project_id: String,
    match_type: String,
    points: Vec<PointAnnotationInput>,
) -> Result<PointAnnotationDocumentRecord, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let annotation_path = point_annotations_path(&project_dir);
    let now = Utc::now().to_rfc3339();
    let created_at = if annotation_path.exists() {
        workspace::read_json::<PointAnnotationDocumentRecord>(&annotation_path)
            .map(|existing| existing.created_at)
            .unwrap_or_else(|_| now.clone())
    } else {
        now.clone()
    };

    let normalized_match_type = normalize_match_type(&match_type);
    let video_id = PathBuf::from(&project.source_video_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project.project_id.clone());

    let mut normalized_points: Vec<PointAnnotationRecord> = points
        .into_iter()
        .enumerate()
        .map(|(index, item)| normalize_point_annotation(item, index + 1, &normalized_match_type))
        .collect();
    normalized_points.sort_by(|left, right| {
        left.start_sec
            .partial_cmp(&right.start_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let document = PointAnnotationDocumentRecord {
        project_id: project_id.clone(),
        created_at,
        updated_at: now,
        annotation_path: annotation_path.display().to_string(),
        video_id,
        camera_mode: "fixed".to_string(),
        match_type: normalized_match_type,
        points: normalized_points,
    };

    workspace::write_json(&annotation_path, &document)?;
    Ok(document)
}

#[tauri::command]
pub fn save_review(
    app: AppHandle,
    project_id: String,
    decisions: Vec<ReviewDecisionInput>,
) -> Result<ReviewSaveResult, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let analysis_result_path = project_dir.join(&project.analysis_result_path);

    if !analysis_result_path.exists() {
        return Err("analysis_result.json 不存在，请先完成 M3 分析".to_string());
    }

    let now = Utc::now().to_rfc3339();
    let review_path = review_result_path(&project_dir);
    let created_at = if review_path.exists() {
        workspace::read_json::<ReviewResultRecord>(&review_path)
            .map(|existing| existing.created_at)
            .unwrap_or_else(|_| now.clone())
    } else {
        now.clone()
    };

    let analysis_result: AnalysisResultRecord = workspace::read_json(&analysis_result_path)?;

    let decision_records: Vec<ReviewDecisionRecord> = decisions
        .into_iter()
        .map(|item| ReviewDecisionRecord {
            segment_id: item.segment_id,
            start_sec: item.start_sec,
            end_sec: item.end_sec,
            duration_sec: item.duration_sec,
            decision: normalize_decision(&item.decision),
        })
        .collect();

    let summary = summarize_review(&decision_records);
    let review = ReviewResultRecord {
        project_id: project_id.clone(),
        created_at,
        updated_at: now,
        source_analysis_result_path: analysis_result_path.display().to_string(),
        source_analysis_created_at: analysis_result.created_at.clone(),
        review_result_path: review_path.display().to_string(),
        decisions: decision_records,
        summary: summary.clone(),
    };

    workspace::write_json(&review_path, &review)?;

    Ok(ReviewSaveResult {
        project_id,
        review_result_path: review_path.display().to_string(),
        summary,
    })
}

#[tauri::command]
pub async fn export_reviewed_video(
    app: AppHandle,
    project_id: String,
    export_profile: Option<String>,
    export_format: Option<String>,
    hardware_encode: Option<bool>,
) -> Result<ExportVideoResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        export_reviewed_video_sync(app, project_id, export_profile, export_format, hardware_encode)
    })
        .await
        .map_err(|error| format!("导出任务异常终止: {error}"))?
}

fn export_reviewed_video_sync(
    app: AppHandle,
    project_id: String,
    export_profile: Option<String>,
    export_format: Option<String>,
    hardware_encode: Option<bool>,
) -> Result<ExportVideoResult, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let probe: ProbeRecord = workspace::read_json(&project_dir.join("probe.json"))?;
    let review_path = review_result_path(&project_dir);

    if !review_path.exists() {
        return Err("review_result.json 不存在，请先完成 M4 审核并保存结果".to_string());
    }

    let review: ReviewResultRecord = workspace::read_json(&review_path)?;
    let analysis_result_path = project_dir.join(&project.analysis_result_path);
    let analysis_result: AnalysisResultRecord = workspace::read_json(&analysis_result_path)?;

    if review.source_analysis_created_at.trim().is_empty()
        || review.source_analysis_created_at != analysis_result.created_at
    {
        return Err("M3 分析结果已经变化，请先在 M4 重新保存审核结果，再进行导出".to_string());
    }

    let keep_decisions: Vec<ReviewDecisionRecord> = review
        .decisions
        .iter()
        .filter(|item| item.decision == "keep")
        .cloned()
        .collect();
    let keep_segments: Vec<(f64, f64)> = keep_decisions
        .iter()
        .map(|item| (item.start_sec, item.end_sec))
        .collect();

    if keep_segments.is_empty() {
        return Err("当前没有 keep 片段，请先在 M4 中至少保留一个片段".to_string());
    }

    let export_profile = normalize_export_profile(export_profile)?;
    let export_format = normalize_export_format(export_format)?;
    let export_settings = ffmpeg::ExportSettings::new(
        export_profile,
        export_format,
        hardware_encode.unwrap_or(false),
    );
    let output_path = export_output_path(&project_dir, &project.title, export_format);
    let mapping_path = export_mapping_path(&output_path);
    let source_video_path = PathBuf::from(&project.source_video_path);

    if !source_video_path.exists() {
        return Err(format!(
            "源视频不存在，可能已被移动或重命名，请重新选择原视频: {}",
            source_video_path.display()
        ));
    }

    emit_export_progress(
        &app,
        &project_id,
        "prepare",
        5,
        &format!(
            "已读取项目与审核结果，准备{} {}",
            export_profile_display(export_profile),
            export_format_display(export_format)
        ),
    )?;
    emit_export_progress(
        &app,
        &project_id,
        "collect_segments",
        20,
        &format!("已收集 {} 个 keep 片段", keep_segments.len()),
    )?;
    emit_export_progress(
        &app,
        &project_id,
        "run_ffmpeg",
        30,
        &format!(
            "开始生成{} {} 高光视频{}",
            export_profile_display(export_profile),
            export_format_display(export_format)
            ,if hardware_encode.unwrap_or(false) { "（已开启硬件加速）" } else { "" }
        ),
    )?;

    let clip_labels: Vec<String> = keep_decisions
        .iter()
        .map(|item| {
            format!(
                "{} | src {}-{}",
                item.segment_id,
                format_seconds_label(item.start_sec),
                format_seconds_label(item.end_sec)
            )
        })
        .collect();

    ffmpeg::export_highlight_video_with_progress(
        &source_video_path,
        &output_path,
        &keep_segments,
        &clip_labels,
        probe.fps,
        probe.has_audio,
        export_settings,
        |progress| {
            let current = 30 + ((progress * 65.0).round() as u8).min(65);
            let _ = emit_export_progress(
                &app,
                &project_id,
                "run_ffmpeg",
                current,
                &format!("正在生成高光视频 ({:.0}%)", progress * 100.0),
            );
        },
    )?;

    let mut exported_cursor_sec = 0.0_f64;
    let clips: Vec<ExportClipMapEntry> = keep_decisions
        .iter()
        .map(|item| {
            let duration_sec = item.duration_sec.max(0.0);
            let clip = ExportClipMapEntry {
                segment_id: item.segment_id.clone(),
                source_start_sec: item.start_sec,
                source_end_sec: item.end_sec,
                exported_start_sec: exported_cursor_sec,
                exported_end_sec: exported_cursor_sec + duration_sec,
                duration_sec,
            };
            exported_cursor_sec += duration_sec;
            clip
        })
        .collect();

    let kept_duration_sec: f64 = clips.iter().map(|item| item.duration_sec).sum();
    workspace::write_json(&mapping_path, &clips)?;

    emit_export_progress(
        &app,
        &project_id,
        "done",
        100,
        &format!(
            "导出完成，已生成{} {} 与片段映射表",
            export_profile_display(export_profile),
            export_format_display(export_format)
        ),
    )?;

    Ok(ExportVideoResult {
        project_id,
        output_path: output_path.display().to_string(),
        mapping_path: mapping_path.display().to_string(),
        kept_segment_count: keep_segments.len(),
        kept_duration_sec,
        export_resolution: export_profile.resolution().as_str().to_string(),
        export_format: export_format.as_str().to_string(),
        clips,
    })
}

fn save_review_with_default_keep(
    app: AppHandle,
    project_id: String,
) -> Result<ReviewSaveResult, String> {
    let project_dir = workspace::projects_root(&app)?.join(&project_id);
    let project: ProjectRecord = workspace::read_json(&project_dir.join("project.json"))?;
    let analysis_result_path = project_dir.join(&project.analysis_result_path);

    if !analysis_result_path.exists() {
        return Err("analysis_result.json 不存在，无法自动生成默认审核结果".to_string());
    }

    let analysis_result: AnalysisResultRecord = workspace::read_json(&analysis_result_path)?;
    let decisions: Vec<ReviewDecisionInput> = analysis_result
        .segments
        .iter()
        .map(|segment| ReviewDecisionInput {
            segment_id: segment.segment_id.clone(),
            start_sec: segment.start_sec,
            end_sec: segment.end_sec,
            duration_sec: segment.duration_sec,
            decision: "keep".to_string(),
        })
        .collect();

    save_review(app, project_id, decisions)
}

fn emit_autoflow_progress(
    app: &AppHandle,
    project_id: &str,
    stage: &str,
    percent: u8,
    message: &str,
) -> Result<(), String> {
    app.emit(
        "autoflow-progress",
        AutoflowProgressEvent {
            project_id: project_id.to_string(),
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    )
    .map_err(|error| format!("发送 autoflow-progress 事件失败: {error}"))
}

fn emit_proxy_progress(
    app: &AppHandle,
    project_id: &str,
    stage: &str,
    percent: u8,
    message: &str,
) -> Result<(), String> {
    app.emit(
        "proxy-progress",
        ProxyProgressEvent {
            project_id: project_id.to_string(),
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    )
    .map_err(|error| format!("发送 proxy-progress 事件失败: {error}"))
}

fn emit_analysis_progress(
    app: &AppHandle,
    project_id: &str,
    stage: &str,
    percent: u8,
    message: &str,
) -> Result<(), String> {
    app.emit(
        "analysis-progress",
        AnalysisProgressEvent {
            project_id: project_id.to_string(),
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    )
    .map_err(|error| format!("发送 analysis-progress 事件失败: {error}"))
}

fn emit_export_progress(
    app: &AppHandle,
    project_id: &str,
    stage: &str,
    percent: u8,
    message: &str,
) -> Result<(), String> {
    app.emit(
        "export-progress",
        ExportProgressEvent {
            project_id: project_id.to_string(),
            stage: stage.to_string(),
            percent,
            message: message.to_string(),
        },
    )
    .map_err(|error| format!("发送 export-progress 事件失败: {error}"))
}

fn normalize_video_path(raw_path: &str) -> Result<PathBuf, String> {
    let path = if raw_path.starts_with("file://") {
        let url = Url::parse(raw_path)
            .map_err(|error| format!("无法解析文件 URI {raw_path}: {error}"))?;
        url.to_file_path()
            .map_err(|_| format!("无法转换文件 URI 为本地路径: {raw_path}"))?
    } else if raw_path.contains("://") {
        return Err(format!(
            "当前版本暂不支持直接处理此 URI：{raw_path}。Android 版本下一步需要补上 URI 导入到应用沙盒的流程。"
        ));
    } else {
        PathBuf::from(raw_path)
    };

    if !path.exists() {
        return Err(format!("视频文件不存在: {raw_path}"));
    }

    if !path.is_file() {
        return Err(format!("所选路径不是文件: {raw_path}"));
    }

    path.canonicalize()
        .map_err(|error| format!("无法解析视频文件路径 {raw_path}: {error}"))
}

fn sanitize_export_file_name(raw_name: &str) -> String {
    let trimmed = raw_name.trim();
    let candidate = if trimmed.is_empty() {
        "tennis-highlights.mp4"
    } else {
        trimmed
    };

    candidate
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

fn review_result_path(project_dir: &Path) -> PathBuf {
    project_dir.join("review_result.json")
}

fn point_annotations_path(project_dir: &Path) -> PathBuf {
    project_dir.join("point_annotations.json")
}

fn export_output_path(
    project_dir: &Path,
    project_title: &str,
    export_format: ffmpeg::ExportFormat,
) -> PathBuf {
    let slug = slugify_filename(project_title);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S");
    project_dir.join(format!(
        "{}-highlights-{}.{}",
        slug,
        timestamp,
        export_format.extension()
    ))
}

fn export_mapping_path(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("tennis-highlight-export");
    output_path.with_file_name(format!("{}-mapping.json", stem))
}

fn format_seconds_label(value: f64) -> String {
    let total_ms = (value.max(0.0) * 1000.0).round() as u64;
    let minutes = total_ms / 60_000;
    let seconds = (total_ms % 60_000) / 1000;
    let centiseconds = (total_ms % 1000) / 10;
    format!("{:02}:{:02}.{:02}", minutes, seconds, centiseconds)
}

fn thumbnail_seek_seconds(duration_sec: f64) -> f64 {
    if !duration_sec.is_finite() || duration_sec <= 0.0 {
        return 0.0;
    }

    let ten_percent = duration_sec * 0.1;
    ten_percent.max(0.8).min(8.0).min(duration_sec * 0.5)
}

fn normalize_export_profile(
    value: Option<String>,
) -> Result<ffmpeg::ExportProfile, String> {
    match value
        .unwrap_or_else(|| "fast".to_string())
        .trim()
        .to_lowercase()
        .as_str()
    {
        "fast" => Ok(ffmpeg::ExportProfile::Fast),
        "hd" => Ok(ffmpeg::ExportProfile::Hd),
        "4k" => Ok(ffmpeg::ExportProfile::K4),
        other => Err(format!("不支持的导出模式: {other}")),
    }
}

fn normalize_export_format(value: Option<String>) -> Result<ffmpeg::ExportFormat, String> {
    match value
        .unwrap_or_else(|| "mp4".to_string())
        .trim()
        .to_lowercase()
        .as_str()
    {
        "mp4" => Ok(ffmpeg::ExportFormat::Mp4),
        "mov" => Ok(ffmpeg::ExportFormat::Mov),
        "webm" => Ok(ffmpeg::ExportFormat::Webm),
        other => Err(format!("不支持的导出格式: {other}")),
    }
}

fn export_profile_display(value: ffmpeg::ExportProfile) -> &'static str {
    match value {
        ffmpeg::ExportProfile::Fast => "极速导出（720P）",
        ffmpeg::ExportProfile::Hd => "高清导出（1080P）",
        ffmpeg::ExportProfile::K4 => "4K 导出",
    }
}

fn export_format_display(value: ffmpeg::ExportFormat) -> &'static str {
    match value {
        ffmpeg::ExportFormat::Mp4 => "MP4",
        ffmpeg::ExportFormat::Mov => "MOV",
        ffmpeg::ExportFormat::Webm => "WEBM",
    }
}

fn slugify_filename(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if ch.is_ascii_whitespace() || ch == '-' || ch == '_' {
            if !last_was_separator && !slug.is_empty() {
                slug.push('-');
                last_was_separator = true;
            }
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "tennis-highlight".to_string()
    } else {
        slug
    }
}

fn normalize_decision(value: &str) -> String {
    match value {
        "keep" => "keep".to_string(),
        "remove" => "remove".to_string(),
        _ => "pending".to_string(),
    }
}

fn summarize_review(decisions: &[ReviewDecisionRecord]) -> ReviewSummaryRecord {
    let keep_count = decisions
        .iter()
        .filter(|item| item.decision == "keep")
        .count();
    let remove_count = decisions
        .iter()
        .filter(|item| item.decision == "remove")
        .count();
    let pending_count = decisions
        .iter()
        .filter(|item| item.decision == "pending")
        .count();
    let kept_duration_sec = decisions
        .iter()
        .filter(|item| item.decision == "keep")
        .map(|item| item.duration_sec)
        .sum();

    ReviewSummaryRecord {
        total_segments: decisions.len(),
        keep_count,
        remove_count,
        pending_count,
        kept_duration_sec,
    }
}

fn normalize_match_type(value: &str) -> String {
    match value {
        "doubles" => "doubles".to_string(),
        _ => "singles".to_string(),
    }
}

fn normalize_keep_decision(value: &str) -> String {
    match value {
        "keep" => "keep".to_string(),
        _ => "drop".to_string(),
    }
}

fn normalize_annotation_flags(flags: Vec<String>, first_serve_fault: bool, match_type: &str) -> Vec<String> {
    let mut normalized = Vec::new();

    for flag in flags {
        let allowed = matches!(flag.as_str(), "low_visibility_ball");
        if allowed && !normalized.iter().any(|item| item == &flag) {
            normalized.push(flag);
        }
    }

    if first_serve_fault {
        normalized.push("first_serve_fault".to_string());
    }
    if match_type == "doubles" {
        normalized.push("doubles_point".to_string());
    }

    normalized
}

fn normalize_point_annotation(
    input: PointAnnotationInput,
    fallback_index: usize,
    match_type: &str,
) -> PointAnnotationRecord {
    let point_id = if input.point_id.trim().is_empty() {
        format!("point-{:03}", fallback_index)
    } else {
        input.point_id.trim().to_string()
    };
    let start_sec = input.start_sec.max(0.0);
    let end_sec = input.end_sec.max(start_sec);
    let serve_attempts = input.serve_attempts.clamp(1, 2);
    let flags = normalize_annotation_flags(input.flags, input.first_serve_fault, match_type);

    PointAnnotationRecord {
        point_id,
        start_sec,
        end_sec,
        shot_count: input.shot_count,
        serve_attempts,
        first_serve_fault: input.first_serve_fault,
        is_ace: input.is_ace,
        is_double_fault: input.is_double_fault,
        keep_decision: normalize_keep_decision(&input.keep_decision),
        tail_sec: input.tail_sec.max(0.0),
        reason: input.reason.trim().to_string(),
        flags,
        notes: input.notes.trim().to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct AnalyzerProgressLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    stage: String,
    percent: u8,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct ReviewDecisionInput {
    segment_id: String,
    start_sec: f64,
    end_sec: f64,
    duration_sec: f64,
    decision: String,
}

#[derive(Debug, Deserialize)]
pub struct PointAnnotationInput {
    point_id: String,
    start_sec: f64,
    end_sec: f64,
    shot_count: u32,
    serve_attempts: u8,
    first_serve_fault: bool,
    is_ace: bool,
    is_double_fault: bool,
    keep_decision: String,
    tail_sec: f64,
    reason: String,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    notes: String,
}

#[cfg(test)]
mod tests {
    use super::slugify_filename;

    #[test]
    fn slugify_filename_keeps_ascii_words() {
        assert_eq!(slugify_filename("Match 01 Final"), "match-01-final");
    }

    #[test]
    fn slugify_filename_falls_back_when_empty() {
        assert_eq!(slugify_filename("   ###   "), "tennis-highlight");
    }
}
