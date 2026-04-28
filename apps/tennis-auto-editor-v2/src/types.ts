export type ExportResolution = '360p' | '480p' | '720p' | '1080p' | '4k'
export type ExportProfile = 'fast' | 'hd' | '4k'
export type ExportFormat = 'mp4' | 'mov' | 'webm'

export interface ProjectRecord {
  project_id: string
  source_video_path: string
  created_at: string
  status: string
  title: string
  probe_path: string
  proxy_path: string
  audio_path: string
  analysis_result_path: string
}

export interface ProbeInfo {
  duration_sec: number
  width: number
  height: number
  fps: number
  has_audio: boolean
  video_codec: string | null
  audio_codec: string | null
}

export interface ProjectDetail {
  project: ProjectRecord
  probe: ProbeInfo
  project_dir: string
}

export interface AnalysisSummary {
  duration_sec: number
  window_sec: number
  segment_count: number
  total_candidate_duration_sec: number
  average_confidence: number
  sensitivity: number
}

export interface AnalysisRunResult {
  project_id: string
  analysis_result_path: string
  segment_count: number
  summary: AnalysisSummary
}

export interface ReviewSummary {
  total_segments: number
  keep_count: number
  remove_count: number
  pending_count: number
  kept_duration_sec: number
}

export interface ReviewSaveResult {
  project_id: string
  review_result_path: string
  summary: ReviewSummary
}

export interface PrepareAutomaticHighlightsResult {
  project: ProjectDetail
  analysis: AnalysisRunResult
  review: ReviewSaveResult
}

export interface AutoflowProgressEvent {
  project_id: string
  stage: string
  percent: number
  message: string
}

export interface ProxyProgressEvent {
  project_id: string
  stage: string
  percent: number
  message: string
}

export interface AnalysisProgressEvent {
  project_id: string
  stage: string
  percent: number
  message: string
}

export interface ExportProgressEvent {
  project_id: string
  stage: string
  percent: number
  message: string
}

export interface ExportClipMapEntry {
  segment_id: string
  source_start_sec: number
  source_end_sec: number
  exported_start_sec: number
  exported_end_sec: number
  duration_sec: number
}

export interface ExportVideoResult {
  project_id: string
  output_path: string
  mapping_path: string
  kept_segment_count: number
  kept_duration_sec: number
  export_resolution: ExportResolution
  export_format: ExportFormat
  clips: ExportClipMapEntry[]
}

export interface CopyFileResult {
  source_path: string
  destination_path: string
}

export interface HardwareEncoderSupport {
  key: string
  label: string
  available: boolean
}

export interface HardwareExportSupport {
  available: boolean
  recommended_key: string | null
  summary: string
  encoders: HardwareEncoderSupport[]
}

export interface RuntimeCapabilities {
  platform: string
  is_mobile: boolean
  supports_save_dialog: boolean
  prefers_generated_export_path: boolean
  export_directory: string
  import_mode: string
  analyzer_backend: string
  runtime_root: string | null
  runtime_source: string
  ffmpeg_bin: string
  ffprobe_bin: string
  ffmpeg_available: boolean
  ffprobe_available: boolean
  media_pipeline_ready: boolean
}
