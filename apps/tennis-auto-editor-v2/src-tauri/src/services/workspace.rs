use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tauri::{AppHandle, Manager};

pub fn bundled_runtime_root(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let runtime_dir = resource_dir.join("runtime");
    runtime_dir.exists().then_some(runtime_dir)
}

pub fn bundled_python_root(app: &AppHandle) -> Option<PathBuf> {
    let runtime_dir = bundled_runtime_root(app)?;
    let python_dir = runtime_dir.join("python");
    python_dir.join("analyzer/main.py").exists().then_some(python_dir)
}

pub fn bundled_python_home(app: &AppHandle) -> Option<PathBuf> {
    let runtime_dir = bundled_runtime_root(app)?;
    let python_home = runtime_dir.join("python-home");
    python_home.exists().then_some(python_home)
}

pub fn bundled_python_bin(app: &AppHandle) -> Option<PathBuf> {
    let runtime_dir = bundled_runtime_root(app)?;
    let candidates = if cfg!(target_os = "windows") {
        vec![
            runtime_dir.join("bin/python.exe"),
            runtime_dir.join("python/python.exe"),
            runtime_dir.join("python-home/python.exe"),
        ]
    } else {
        vec![
            runtime_dir.join("bin/python3"),
            runtime_dir.join("bin/python"),
            runtime_dir.join("python/bin/python3"),
            runtime_dir.join("python-home/bin/python3"),
            runtime_dir.join("python-home/bin/python"),
        ]
    };

    candidates.into_iter().find(|path| path.exists())
}

pub fn bundled_ffmpeg_bin(app: &AppHandle) -> Option<PathBuf> {
    let runtime_dir = bundled_runtime_root(app)?;
    let candidates = if cfg!(target_os = "windows") {
        vec![runtime_dir.join("bin/ffmpeg.exe"), runtime_dir.join("ffmpeg/ffmpeg.exe")]
    } else {
        vec![runtime_dir.join("bin/ffmpeg"), runtime_dir.join("ffmpeg/ffmpeg")]
    };

    candidates.into_iter().find(|path| path.exists())
}

pub fn bundled_ffprobe_bin(app: &AppHandle) -> Option<PathBuf> {
    let runtime_dir = bundled_runtime_root(app)?;
    let candidates = if cfg!(target_os = "windows") {
        vec![runtime_dir.join("bin/ffprobe.exe"), runtime_dir.join("ffmpeg/ffprobe.exe")]
    } else {
        vec![runtime_dir.join("bin/ffprobe"), runtime_dir.join("ffmpeg/ffprobe")]
    };

    candidates.into_iter().find(|path| path.exists())
}

pub fn projects_root(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法获取 app data 目录: {error}"))?;

    let projects_dir = app_data_dir.join("projects");
    fs::create_dir_all(&projects_dir)
        .map_err(|error| format!("无法创建项目根目录 {}: {error}", projects_dir.display()))?;

    Ok(projects_dir)
}

pub fn ensure_project_dir(app: &AppHandle, project_id: &str) -> Result<PathBuf, String> {
    let project_dir = projects_root(app)?.join(project_id);
    fs::create_dir_all(&project_dir)
        .map_err(|error| format!("无法创建项目目录 {}: {error}", project_dir.display()))?;

    Ok(project_dir)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|error| format!("序列化 JSON 失败 {}: {error}", path.display()))?;

    fs::write(path, serialized).map_err(|error| format!("写入文件失败 {}: {error}", path.display()))
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("读取文件失败 {}: {error}", path.display()))?;

    serde_json::from_str(&raw)
        .map_err(|error| format!("解析 JSON 失败 {}: {error}", path.display()))
}

pub fn latest_project_dir(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let projects_dir = projects_root(app)?;
    let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();

    for entry in fs::read_dir(&projects_dir)
        .map_err(|error| format!("读取项目根目录失败 {}: {error}", projects_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("读取项目目录项失败: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let project_json_path = path.join("project.json");
        if !project_json_path.exists() {
            continue;
        }

        let modified_at = fs::metadata(&project_json_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        candidates.push((modified_at, path));
    }

    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(candidates.into_iter().map(|(_, path)| path).next())
}

pub fn build_title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "untitled_match".to_string())
}
