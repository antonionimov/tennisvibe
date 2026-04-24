use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub project_id: String,
    pub source_video_path: String,
    pub created_at: String,
    pub status: String,
    pub title: String,
    pub probe_path: String,
    pub proxy_path: String,
    pub audio_path: String,
    pub analysis_result_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRecord {
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub project: ProjectRecord,
    pub probe: ProbeRecord,
    pub project_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProgressEvent {
    pub project_id: String,
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyGenerationResult {
    pub project_id: String,
    pub proxy_path: String,
    pub audio_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisProgressEvent {
    pub project_id: String,
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummaryRecord {
    pub duration_sec: f64,
    pub window_sec: f64,
    pub segment_count: usize,
    pub total_candidate_duration_sec: f64,
    pub average_confidence: f64,
    pub sensitivity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSegmentRecord {
    pub segment_id: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub confidence: f64,
    pub score: f64,
    pub reasons: Vec<String>,
    pub metrics: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResultRecord {
    pub project_id: String,
    pub created_at: String,
    pub sensitivity: f64,
    pub source_video_path: String,
    pub proxy_path: String,
    pub audio_path: String,
    pub summary: AnalysisSummaryRecord,
    pub segments: Vec<AnalysisSegmentRecord>,
    pub debug: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRunResult {
    pub project_id: String,
    pub analysis_result_path: String,
    pub segment_count: usize,
    pub summary: AnalysisSummaryRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDecisionRecord {
    pub segment_id: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub decision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummaryRecord {
    pub total_segments: usize,
    pub keep_count: usize,
    pub remove_count: usize,
    pub pending_count: usize,
    pub kept_duration_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResultRecord {
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_analysis_result_path: String,
    #[serde(default)]
    pub source_analysis_created_at: String,
    pub review_result_path: String,
    pub decisions: Vec<ReviewDecisionRecord>,
    pub summary: ReviewSummaryRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSaveResult {
    pub project_id: String,
    pub review_result_path: String,
    pub summary: ReviewSummaryRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProgressEvent {
    pub project_id: String,
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoflowProgressEvent {
    pub project_id: String,
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareAutomaticHighlightsResult {
    pub project: ProjectDetail,
    pub analysis: AnalysisRunResult,
    pub review: ReviewSaveResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyFileResult {
    pub source_path: String,
    pub destination_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportClipMapEntry {
    pub segment_id: String,
    pub source_start_sec: f64,
    pub source_end_sec: f64,
    pub exported_start_sec: f64,
    pub exported_end_sec: f64,
    pub duration_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportVideoResult {
    pub project_id: String,
    pub output_path: String,
    pub mapping_path: String,
    pub kept_segment_count: usize,
    pub kept_duration_sec: f64,
    pub export_resolution: String,
    pub export_format: String,
    pub clips: Vec<ExportClipMapEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareEncoderSupportRecord {
    pub key: String,
    pub label: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareExportSupportRecord {
    pub available: bool,
    pub recommended_key: Option<String>,
    pub summary: String,
    pub encoders: Vec<HardwareEncoderSupportRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointAnnotationRecord {
    pub point_id: String,
    pub start_sec: f64,
    pub end_sec: f64,
    pub shot_count: u32,
    pub serve_attempts: u8,
    pub first_serve_fault: bool,
    pub is_ace: bool,
    pub is_double_fault: bool,
    pub keep_decision: String,
    pub tail_sec: f64,
    pub reason: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointAnnotationDocumentRecord {
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub annotation_path: String,
    pub video_id: String,
    pub camera_mode: String,
    pub match_type: String,
    pub points: Vec<PointAnnotationRecord>,
}
