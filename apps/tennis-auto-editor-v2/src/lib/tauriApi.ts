import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { basename } from '@tauri-apps/api/path'
import { open, save } from '@tauri-apps/plugin-dialog'
import { copyFile, exists, mkdir, BaseDirectory } from '@tauri-apps/plugin-fs'
import type {
  AnalysisProgressEvent,
  AutoflowProgressEvent,
  CopyFileResult,
  ExportFormat,
  ExportProfile,
  ExportProgressEvent,
  ExportVideoResult,
  HardwareExportSupport,
  PrepareAutomaticHighlightsResult,
  ProxyProgressEvent,
  RuntimeCapabilities,
} from '../types'

const SUPPORTED_VIDEO_EXTENSIONS = ['mp4', 'mov', 'm4v', 'avi', 'mkv', 'webm']
const IMPORTS_DIRECTORY = 'imports'

function ensureTauriRuntime() {
  if (typeof window === 'undefined') {
    throw new Error('当前没有可用的窗口环境')
  }
}

async function tauriInvoke<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  ensureTauriRuntime()
  return invoke<T>(command, payload)
}

async function tauriListen<T>(eventName: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  ensureTauriRuntime()
  return listen<T>(eventName, (event) => {
    handler(event.payload)
  })
}

export async function selectVideoFile(): Promise<string | null> {
  const selection = await open({
    directory: false,
    multiple: false,
    title: '选择网球比赛视频',
    filters: [
      {
        name: 'Video',
        extensions: SUPPORTED_VIDEO_EXTENSIONS,
      },
    ],
  })

  return typeof selection === 'string' ? selection : null
}

export async function selectExportPath(defaultFileName: string, format: ExportFormat): Promise<string | null> {
  return save({
    title: '保存高光视频',
    defaultPath: defaultFileName,
    filters: [
      {
        name: format.toUpperCase(),
        extensions: [format],
      },
    ],
  })
}

export async function prepareAutomaticHighlights(videoPath: string): Promise<PrepareAutomaticHighlightsResult> {
  return tauriInvoke<PrepareAutomaticHighlightsResult>('prepare_automatic_highlights', { videoPath })
}

export async function extractVideoThumbnail(videoPath: string): Promise<string> {
  return tauriInvoke<string>('extract_video_thumbnail', { videoPath })
}

export async function getHardwareExportSupport(): Promise<HardwareExportSupport> {
  return tauriInvoke<HardwareExportSupport>('get_hardware_export_support')
}

export async function getRuntimeCapabilities(): Promise<RuntimeCapabilities> {
  return tauriInvoke<RuntimeCapabilities>('get_runtime_capabilities')
}

export async function suggestExportPath(defaultFileName: string): Promise<string> {
  return tauriInvoke<string>('suggest_export_path', { defaultFileName })
}

export async function importVideoIntoAppStorage(sourcePath: string): Promise<string> {
  const importId = String(Date.now())
  const sourceName = (await basename(sourcePath).catch(() => null)) || `match-${importId}.mp4`
  const sanitizedName = sourceName.replace(/[\\/:*?"<>|]/g, '_')

  const importsDirExists = await exists(IMPORTS_DIRECTORY, { baseDir: BaseDirectory.AppData })
  if (!importsDirExists) {
    await mkdir(IMPORTS_DIRECTORY, { baseDir: BaseDirectory.AppData, recursive: true })
  }

  const relativeImportDir = `${IMPORTS_DIRECTORY}/${importId}`
  await mkdir(relativeImportDir, { baseDir: BaseDirectory.AppData, recursive: true })

  const relativeTargetPath = `${relativeImportDir}/${sanitizedName}`
  await copyFile(sourcePath, relativeTargetPath, { toPathBaseDir: BaseDirectory.AppData })
  return tauriInvoke<string>('resolve_imported_app_path', { relativePath: relativeTargetPath })
}

export async function exportReviewedVideo(
  projectId: string,
  exportProfile: ExportProfile,
  exportFormat: ExportFormat,
  hardwareEncode: boolean,
): Promise<ExportVideoResult> {
  return tauriInvoke<ExportVideoResult>('export_reviewed_video', { projectId, exportProfile, exportFormat, hardwareEncode })
}

export async function copyFileToPath(sourcePath: string, destinationPath: string): Promise<CopyFileResult> {
  return tauriInvoke<CopyFileResult>('copy_file_to_path', { sourcePath, destinationPath })
}

export async function listenToAutoflowProgress(
  handler: (payload: AutoflowProgressEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<AutoflowProgressEvent>('autoflow-progress', handler)
}

export async function listenToProxyProgress(
  handler: (payload: ProxyProgressEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<ProxyProgressEvent>('proxy-progress', handler)
}

export async function listenToAnalysisProgress(
  handler: (payload: AnalysisProgressEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<AnalysisProgressEvent>('analysis-progress', handler)
}

export async function listenToExportProgress(
  handler: (payload: ExportProgressEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<ExportProgressEvent>('export-progress', handler)
}
