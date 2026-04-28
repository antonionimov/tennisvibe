mod commands;
mod models;
mod services;

use std::env;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();

            if let Err(error) = services::workspace::prepare_mobile_runtime(&app_handle) {
                eprintln!("[tennis-auto-editor-v2] prepare mobile runtime failed: {error}");
            }

            if let Some(path) = services::workspace::bundled_python_root(&app_handle) {
                env::set_var("TENNIS_AUTO_EDITOR_PYTHON_ROOT", path);
            }

            if let Some(path) = services::workspace::bundled_python_bin(&app_handle) {
                env::set_var("TENNIS_AUTO_EDITOR_PYTHON_BIN", path);
            }

            if let Some(path) = services::workspace::bundled_python_home(&app_handle) {
                env::set_var("TENNIS_AUTO_EDITOR_PYTHONHOME", path);
            }

            if let Some(path) = services::workspace::bundled_ffmpeg_bin(&app_handle) {
                env::set_var("TENNIS_AUTO_EDITOR_FFMPEG_BIN", path);
            }

            if let Some(path) = services::workspace::bundled_ffprobe_bin(&app_handle) {
                env::set_var("TENNIS_AUTO_EDITOR_FFPROBE_BIN", path);
            }

            Ok(())
        })
        .manage(Mutex::new(services::mpv::MpvController::default()))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::project::create_project,
            commands::project::get_project,
            commands::project::get_latest_project,
            commands::project::extract_video_thumbnail,
            commands::project::get_hardware_export_support,
            commands::project::get_runtime_capabilities,
            commands::project::generate_proxy,
            commands::project::run_analysis,
            commands::project::get_analysis_result,
            commands::project::get_review_result,
            commands::project::save_review,
            commands::project::export_reviewed_video,
            commands::project::prepare_automatic_highlights,
            commands::project::suggest_export_path,
            commands::project::resolve_imported_app_path,
            commands::project::copy_file_to_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tennis auto editor v2");
}
