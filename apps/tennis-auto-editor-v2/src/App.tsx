import { convertFileSrc } from '@tauri-apps/api/core'
import { useEffect, useMemo, useRef, useState, type SVGProps } from 'react'
import {
  copyFileToPath,
  extractVideoThumbnail,
  exportReviewedVideo,
  getHardwareExportSupport,
  listenToAnalysisProgress,
  listenToAutoflowProgress,
  listenToExportProgress,
  listenToProxyProgress,
  prepareAutomaticHighlights,
  selectExportPath,
  selectVideoFile,
} from './lib/tauriApi'
import type {
  ExportFormat,
  ExportProfile,
  ExportVideoResult,
  HardwareExportSupport,
  PrepareAutomaticHighlightsResult,
} from './types'

type AppPhase = 'idle' | 'ready' | 'preparing' | 'prepared' | 'exporting' | 'done' | 'failed'
type ExportStage = 'preview' | 'final' | null
type ExportSampleStage = 'preview' | 'final'
type ExportProgressSample = {
  timeMs: number
  percent: number
  stage: ExportSampleStage
}

const exportProfiles: Array<{ value: ExportProfile; label: string; hint: string }> = [
  { value: 'fast', label: '极速导出', hint: '720P · superfast · CRF 24' },
  { value: 'hd', label: '高清导出', hint: '1080P · veryfast · CRF 20' },
  { value: '4k', label: '4K 导出', hint: '4K · veryfast · 更清晰' },
]
const formats: ExportFormat[] = ['mp4', 'mov']
const finalStageTimeRatioByProfile: Record<ExportProfile, number> = {
  fast: 1,
  hd: 3,
  '4k': 6.5,
}
const wechatQrPreviewAvailable = true

function TennisBallIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <circle cx="12" cy="12" r="10" />
      <path d="M6 6c3 3 3 9 0 12" />
      <path d="M18 6c-3 3-3 9 0 12" />
    </svg>
  )
}

function UploadIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M12 3v12" />
      <path d="m7 8 5-5 5 5" />
      <path d="M4 17v1a3 3 0 0 0 3 3h10a3 3 0 0 0 3-3v-1" />
    </svg>
  )
}

function FileVideoIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M14.5 3H8a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V8.5L14.5 3z" />
      <path d="M14 3v6h6" />
      <path d="m10 13 5 3-5 3v-6z" />
    </svg>
  )
}

function SparklesIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="m12 3 1.9 4.6L18.5 9.5l-4.6 1.9L12 16l-1.9-4.6L5.5 9.5l4.6-1.9L12 3Z" />
      <path d="M19 3v4" />
      <path d="M21 5h-4" />
      <path d="M5 17v4" />
      <path d="M7 19H3" />
    </svg>
  )
}

function DownloadIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M12 3v12" />
      <path d="m7 10 5 5 5-5" />
      <path d="M4 21h16" />
    </svg>
  )
}

function CheckCircleIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <circle cx="12" cy="12" r="10" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  )
}

function ChevronRightIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="m9 18 6-6-6-6" />
    </svg>
  )
}

function getFileName(filePath: string) {
  return filePath.split(/[\\/]/).pop() || filePath
}

function stripExtension(fileName: string) {
  const parts = fileName.split('.')
  if (parts.length <= 1) {
    return fileName
  }
  parts.pop()
  return parts.join('.')
}

function formatDuration(seconds: number) {
  const total = Math.max(0, Math.round(seconds))
  const minutes = Math.floor(total / 60)
  const remainingSeconds = total % 60
  return `${String(minutes).padStart(2, '0')}:${String(remainingSeconds).padStart(2, '0')}`
}

function buildDefaultExportFileName(fileName: string, format: ExportFormat) {
  return `${stripExtension(fileName)}-highlights.${format}`
}

function formatEtaLabel(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) {
    return '--:--'
  }

  return formatDuration(seconds)
}

function roundEta(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) {
    return null
  }

  if (seconds >= 180) {
    return Math.max(0, Math.round(seconds / 10) * 10)
  }

  if (seconds >= 60) {
    return Math.max(0, Math.round(seconds / 5) * 5)
  }

  return Math.max(0, Math.round(seconds))
}

function appendProgressSample(samples: ExportProgressSample[], nextSample: ExportProgressSample) {
  const recentSamples = samples.filter((sample) => nextSample.timeMs - sample.timeMs <= 45_000)
  const lastSample = recentSamples[recentSamples.length - 1]

  if (lastSample && lastSample.stage === nextSample.stage) {
    if (nextSample.percent <= lastSample.percent) {
      return recentSamples
    }

    if (nextSample.timeMs - lastSample.timeMs < 350) {
      return [...recentSamples.slice(0, -1), nextSample]
    }
  }

  return [...recentSamples, nextSample].slice(-24)
}

function estimateProgressRate(samples: ExportProgressSample[], elapsedSec: number, progressPercent: number) {
  const recentSamples = samples.slice(-8)
  let weightedRateSum = 0
  let weightedRateCount = 0

  for (let index = 1; index < recentSamples.length; index += 1) {
    const previous = recentSamples[index - 1]
    const current = recentSamples[index]
    const deltaPercent = current.percent - previous.percent
    const deltaSec = (current.timeMs - previous.timeMs) / 1000

    if (deltaPercent <= 0 || deltaSec <= 0.2) {
      continue
    }

    const weight = index === recentSamples.length - 1 ? 3 : 1 + index / recentSamples.length
    weightedRateSum += (deltaPercent / deltaSec) * weight
    weightedRateCount += weight
  }

  const recentRate = weightedRateCount > 0 ? weightedRateSum / weightedRateCount : null
  const averageRate = elapsedSec > 0.5 && progressPercent > 0 ? progressPercent / elapsedSec : null

  if (recentRate != null && averageRate != null) {
    return recentRate * 0.72 + averageRate * 0.28
  }

  return recentRate ?? averageRate
}

function estimateStageRemainingSeconds(samples: ExportProgressSample[], progressPercent: number, elapsedSec: number) {
  const rate = estimateProgressRate(samples, elapsedSec, progressPercent)
  if (rate == null || rate <= 0.01) {
    return null
  }

  return Math.max(0, (100 - progressPercent) / rate)
}

async function captureVideoFrame(path: string) {
  return await new Promise<string>((resolve, reject) => {
    const video = document.createElement('video')
    let settled = false

    const cleanup = () => {
      video.pause()
      video.removeAttribute('src')
      video.load()
    }

    const finish = (callback: (value: string | Error) => void, value: string | Error) => {
      if (settled) {
        return
      }

      settled = true
      window.clearTimeout(timeoutId)
      cleanup()
      callback(value)
    }

    const drawFrame = () => {
      if (!video.videoWidth || !video.videoHeight) {
        finish((value) => reject(value as Error), new Error('视频帧尺寸不可用'))
        return
      }

      const canvas = document.createElement('canvas')
      canvas.width = video.videoWidth
      canvas.height = video.videoHeight

      const context = canvas.getContext('2d')
      if (!context) {
        finish((value) => reject(value as Error), new Error('无法创建预览画布'))
        return
      }

      context.drawImage(video, 0, 0, canvas.width, canvas.height)
      finish((value) => resolve(value as string), canvas.toDataURL('image/jpeg', 0.84))
    }

    const timeoutId = window.setTimeout(() => {
      finish((value) => reject(value as Error), new Error('提取视频首帧超时'))
    }, 5000)

    video.preload = 'auto'
    video.muted = true
    video.playsInline = true
    video.src = convertFileSrc(path)

    video.addEventListener(
      'loadedmetadata',
      () => {
        const duration = Number.isFinite(video.duration) ? video.duration : 0
        const targetTime = duration > 0.4 ? Math.min(3, Math.max(0.2, duration * 0.08)) : 0.1
        const safeTime = duration > 0.2 ? Math.min(targetTime, Math.max(0, duration - 0.1)) : targetTime

        if (safeTime <= 0.12) {
          drawFrame()
          return
        }

        video.currentTime = safeTime
      },
      { once: true },
    )

    video.addEventListener('seeked', drawFrame, { once: true })
    video.addEventListener(
      'error',
      () => {
        finish((value) => reject(value as Error), new Error('浏览器路径预览失败'))
      },
      { once: true },
    )

    video.load()
  })
}

function mapExportProgress(percent: number, exportProfile: ExportProfile, exportStage: ExportStage) {
  if (exportProfile === 'fast') {
    return percent
  }

  if (exportStage === 'preview') {
    return Math.min(35, Math.round(percent * 0.35))
  }

  if (exportStage === 'final') {
    return Math.min(100, 35 + Math.round(percent * 0.65))
  }

  return percent
}

function describeError(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) {
    return error.message
  }

  if (typeof error === 'string' && error.trim()) {
    return error
  }

  try {
    const text = JSON.stringify(error)
    if (text && text !== '{}') {
      return text
    }
  } catch {
    // ignore stringify failure
  }

  return fallback
}

function App() {
  const [phase, setPhase] = useState<AppPhase>('idle')
  const [progress, setProgress] = useState(0)
  const [statusText, setStatusText] = useState('')
  const [exportProfile, setExportProfile] = useState<ExportProfile>('fast')
  const [format, setFormat] = useState<ExportFormat>('mp4')
  const [hardwareEncode, setHardwareEncode] = useState(false)
  const [selectedVideoPath, setSelectedVideoPath] = useState<string | null>(null)
  const [selectedVideoPreview, setSelectedVideoPreview] = useState<string | null>(null)
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null)
  const [prepareResult, setPrepareResult] = useState<PrepareAutomaticHighlightsResult | null>(null)
  const [exportResult, setExportResult] = useState<ExportVideoResult | null>(null)
  const [savedPath, setSavedPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [exportStartedAt, setExportStartedAt] = useState<number | null>(null)
  const [nowMs, setNowMs] = useState(() => Date.now())
  const [exportStage, setExportStage] = useState<ExportStage>(null)
  const [exportStageStartedAt, setExportStageStartedAt] = useState<number | null>(null)
  const [exportStageProgress, setExportStageProgress] = useState(0)
  const [previewStageDurationSec, setPreviewStageDurationSec] = useState<number | null>(null)
  const [exportProgressSamples, setExportProgressSamples] = useState<ExportProgressSample[]>([])
  const [hardwareSupport, setHardwareSupport] = useState<HardwareExportSupport | null>(null)
  const previewRequestIdRef = useRef(0)

  const selectedFileName = useMemo(
    () => (selectedVideoPath ? getFileName(selectedVideoPath) : 'MATCH_RECORDING_2024.mp4'),
    [selectedVideoPath],
  )

  const exportEtaSeconds = useMemo(() => {
    if (phase !== 'exporting' || !exportStartedAt || !exportStage || !exportStageStartedAt || progress >= 100) {
      return null
    }

    const stageElapsedSec = Math.max(1, (nowMs - exportStageStartedAt) / 1000)
    const stageSamples = exportProgressSamples.filter((sample) => sample.stage === exportStage)
    const directStageEta = estimateStageRemainingSeconds(stageSamples, exportStageProgress, stageElapsedSec)

    if (exportProfile === 'fast') {
      return roundEta(directStageEta)
    }

    if (exportStage === 'preview') {
      const previewTotalSec = directStageEta != null ? stageElapsedSec + directStageEta : stageElapsedSec / Math.max(exportStageProgress / 100, 0.08)
      const estimatedFinalSec = previewTotalSec * finalStageTimeRatioByProfile[exportProfile]
      return roundEta((directStageEta ?? Math.max(0, previewTotalSec - stageElapsedSec)) + estimatedFinalSec)
    }

    const priorBasedEta = previewStageDurationSec != null
      ? Math.max(0, previewStageDurationSec * finalStageTimeRatioByProfile[exportProfile] - stageElapsedSec)
      : null

    if (directStageEta != null && priorBasedEta != null) {
      const observedWeight = Math.max(0.45, exportStageProgress / 100)
      const priorWeight = 1 - observedWeight
      return roundEta(directStageEta * observedWeight + priorBasedEta * priorWeight)
    }

    return roundEta(directStageEta ?? priorBasedEta)
  }, [
    exportProfile,
    exportProgressSamples,
    exportStage,
    exportStageProgress,
    exportStageStartedAt,
    exportStartedAt,
    nowMs,
    phase,
    previewStageDurationSec,
    progress,
  ])

  useEffect(() => {
    if (phase !== 'exporting') {
      return
    }

    const timer = window.setInterval(() => {
      setNowMs(Date.now())
    }, 1000)

    return () => {
      window.clearInterval(timer)
    }
  }, [phase])

  useEffect(() => {
    void getHardwareExportSupport()
      .then((support) => {
        setHardwareSupport(support)
      })
      .catch(() => {
        setHardwareSupport(null)
      })
  }, [])

  useEffect(() => {
    let cleanupAutoflow: (() => void) | undefined
    let cleanupProxy: (() => void) | undefined
    let cleanupAnalysis: (() => void) | undefined
    let cleanupExport: (() => void) | undefined

    void listenToAutoflowProgress((payload) => {
      if (phase !== 'preparing') {
        return
      }

      if (activeProjectId && payload.project_id !== activeProjectId) {
        return
      }

      if (!activeProjectId) {
        setActiveProjectId(payload.project_id)
      }

      setProgress(payload.percent)
      setStatusText(payload.message)
    }).then((unlisten) => {
      cleanupAutoflow = unlisten
    })

    void listenToProxyProgress((payload) => {
      if (phase !== 'preparing') {
        return
      }
      if (activeProjectId && payload.project_id !== activeProjectId) {
        return
      }
      setProgress(Math.min(38, 8 + Math.round(payload.percent * 0.3)))
      setStatusText(payload.message)
    }).then((unlisten) => {
      cleanupProxy = unlisten
    })

    void listenToAnalysisProgress((payload) => {
      if (phase !== 'preparing') {
        return
      }
      if (activeProjectId && payload.project_id !== activeProjectId) {
        return
      }
      setProgress(Math.min(95, 38 + Math.round(payload.percent * 0.57)))
      setStatusText(payload.message)
    }).then((unlisten) => {
      cleanupAnalysis = unlisten
    })

    void listenToExportProgress((payload) => {
      if (phase !== 'exporting') {
        return
      }
      if (activeProjectId && payload.project_id !== activeProjectId) {
        return
      }

      const activeStage: ExportSampleStage = exportStage === 'preview' ? 'preview' : 'final'
      const mappedPercent = mapExportProgress(payload.percent, exportProfile, exportStage)
      const sampleTime = Date.now()

      setProgress((current) => Math.max(current, mappedPercent))
      setExportStageProgress((current) => Math.max(current, payload.percent))
      setExportProgressSamples((current) =>
        appendProgressSample(current, {
          timeMs: sampleTime,
          percent: payload.percent,
          stage: activeStage,
        }),
      )
      setStatusText(payload.message)
    }).then((unlisten) => {
      cleanupExport = unlisten
    })

    return () => {
      cleanupAutoflow?.()
      cleanupProxy?.()
      cleanupAnalysis?.()
      cleanupExport?.()
    }
  }, [activeProjectId, exportProfile, exportStage, phase])

  const handleFileSelect = async () => {
    try {
      const path = await selectVideoFile()
      if (!path) {
        return
      }

      setSelectedVideoPath(path)
      setPrepareResult(null)
      setExportResult(null)
      setSavedPath(null)
      setSelectedVideoPreview(null)
      setActiveProjectId(null)
      setError(null)
      setStatusText('')
      setProgress(0)
      setExportStartedAt(null)
      setExportStage(null)
      setExportStageStartedAt(null)
      setExportStageProgress(0)
      setPreviewStageDurationSec(null)
      setExportProgressSamples([])
      setPhase('ready')

      const requestId = previewRequestIdRef.current + 1
      previewRequestIdRef.current = requestId

      void captureVideoFrame(path)
        .catch(async () => {
          return await extractVideoThumbnail(path)
        })
        .then((preview) => {
          if (previewRequestIdRef.current === requestId) {
            setSelectedVideoPreview(preview)
          }
        })
        .catch(() => {
          if (previewRequestIdRef.current === requestId) {
            setSelectedVideoPreview(null)
          }
        })
    } catch (caughtError) {
      setError(describeError(caughtError, '选择视频失败'))
      setPhase('failed')
    }
  }

  const handleStartProcessing = async () => {
    if (!selectedVideoPath) {
      return
    }

    setPhase('preparing')
    setError(null)
    setPrepareResult(null)
    setExportResult(null)
    setSavedPath(null)
    setActiveProjectId(null)
    setProgress(3)
    setStatusText('正在创建项目并准备自动剪辑流程')
    setExportStartedAt(null)
    setExportStage(null)
    setExportStageStartedAt(null)
    setExportStageProgress(0)
    setPreviewStageDurationSec(null)
    setExportProgressSamples([])

    try {
      const result = await prepareAutomaticHighlights(selectedVideoPath)
      setPrepareResult(result)
      setActiveProjectId(result.project.project.project_id)
      setProgress(100)
      setStatusText('精彩片段已经准备完成，可以直接导出')
      setPhase('prepared')
    } catch (caughtError) {
      setError(describeError(caughtError, '自动剪辑失败'))
      setPhase('failed')
    }
  }

  const handleSaveHighlights = async () => {
    if (!prepareResult) {
      return
    }

    const destinationPath = await selectExportPath(buildDefaultExportFileName(selectedFileName, format), format)
    if (!destinationPath) {
      return
    }

    setPhase('exporting')
    setError(null)
    setSavedPath(null)
    setProgress(0)
    setStatusText(exportProfile === 'fast' ? '正在生成并保存高光视频' : '正在先生成可预览版本')
    const startedAt = Date.now()
    setExportStartedAt(startedAt)
    setNowMs(startedAt)
    setExportStage(exportProfile === 'fast' ? 'final' : 'preview')
    setExportStageStartedAt(startedAt)
    setExportStageProgress(0)
    setPreviewStageDurationSec(null)
    setExportProgressSamples([])

    try {
      if (exportProfile === 'fast') {
        const result = await exportReviewedVideo(prepareResult.project.project.project_id, exportProfile, format, hardwareEncode)
        await copyFileToPath(result.output_path, destinationPath)
        setExportResult(result)
        setSavedPath(destinationPath)
        setProgress(100)
        setStatusText('高光视频已经保存完成')
        setExportStartedAt(null)
        setExportStage(null)
        setPhase('done')
        return
      }

      const previewStageStartedAt = startedAt
      const previewResult = await exportReviewedVideo(prepareResult.project.project.project_id, 'fast', format, hardwareEncode)
      await copyFileToPath(previewResult.output_path, destinationPath)
      setExportResult(previewResult)
      setSavedPath(destinationPath)
      setProgress(35)
      setStatusText('预览版已保存，继续生成正式版')
      setPreviewStageDurationSec(Math.max(1, (Date.now() - previewStageStartedAt) / 1000))
      setExportStage('final')
      const finalStageStartedAt = Date.now()
      setExportStageStartedAt(finalStageStartedAt)
      setExportStageProgress(0)

      const finalResult = await exportReviewedVideo(prepareResult.project.project.project_id, exportProfile, format, hardwareEncode)
      await copyFileToPath(finalResult.output_path, destinationPath)
      setExportResult(finalResult)
      setSavedPath(destinationPath)
      setProgress(100)
      setStatusText('正式版高光视频已经保存完成')
      setExportStartedAt(null)
      setExportStage(null)
      setExportStageStartedAt(null)
      setExportStageProgress(0)
      setPhase('done')
    } catch (caughtError) {
      const baseMessage = describeError(caughtError, '导出高光视频失败')
      setError(exportProfile === 'fast' ? baseMessage : `预览版可能已保存，但正式版导出失败: ${baseMessage}`)
      setExportStartedAt(null)
      setExportStage(null)
      setExportStageStartedAt(null)
      setExportStageProgress(0)
      setPhase('prepared')
    }
  }

  const resetApp = () => {
    setPhase('idle')
    setProgress(0)
    setStatusText('')
    setSelectedVideoPath(null)
    setSelectedVideoPreview(null)
    setActiveProjectId(null)
    setPrepareResult(null)
    setExportResult(null)
    setSavedPath(null)
    setError(null)
    setExportStartedAt(null)
    setExportStage(null)
    setExportStageStartedAt(null)
    setExportStageProgress(0)
    setPreviewStageDurationSec(null)
    setExportProgressSamples([])
    setExportProfile('fast')
    setFormat('mp4')
    setHardwareEncode(false)
    previewRequestIdRef.current += 1
  }

  const isProcessing = phase === 'preparing' || phase === 'exporting'
  const canStart = phase === 'ready' || phase === 'failed'
  const showExportPanel = phase === 'prepared' || phase === 'done'
  const processingTitle = phase === 'exporting'
    ? exportProfile === 'fast'
      ? '正在生成高光合集...'
      : exportStage === 'preview'
        ? '正在生成预览版...'
        : '正在生成正式版...'
    : '正在扫描每一拍击球...'
  const successMessage = phase === 'done' && savedPath ? `已保存到 ${savedPath}` : '剪辑完成，请选择导出配置'

  return (
    <div className="min-h-screen bg-[#F5F5F7] px-4 py-8 text-[#1D1D1F] sm:py-10">
      <div className="mx-auto flex min-h-[calc(100vh-4rem)] w-full max-w-2xl flex-col items-center justify-center">
        <div className="mb-10 flex flex-col items-center">
          <div className="flex items-center gap-3 sm:gap-4">
            <div className="rounded-2xl border border-[#CCEE00] bg-[#DFFF00] p-3 shadow-[0_4px_20px_rgba(223,255,0,0.3)]">
              <TennisBallIcon className="h-8 w-8 text-[#1D1D1F]" />
            </div>
            <h1 className="text-center text-2xl font-semibold tracking-tight text-[#1D1D1F] sm:text-[1.75rem]">记录你的每一场高光</h1>
          </div>
          <div className="group relative mt-3">
            <p className="cursor-default text-center text-sm font-medium text-gray-500 underline decoration-dotted underline-offset-4 transition-colors duration-200 group-hover:text-[#1D1D1F]">
              欢迎给我意见改进这个工具
            </p>
            <div className="pointer-events-none absolute left-1/2 top-full z-20 mt-3 w-56 -translate-x-1/2 rounded-2xl border border-gray-100 bg-white p-4 text-center opacity-0 shadow-[0_20px_50px_rgba(0,0,0,0.08)] transition-all duration-200 group-hover:translate-y-1 group-hover:opacity-100">
              {wechatQrPreviewAvailable ? (
                <>
                  <img src="/wechat-qr.jpg" alt="微信二维码" className="mx-auto h-48 w-48 rounded-xl object-cover" />
                  <p className="mt-3 text-sm font-semibold text-[#1D1D1F]">微信扫码提意见</p>
                  <p className="mt-1 text-xs leading-5 text-gray-500">欢迎反馈 bug、体验问题，或者你想加的新功能</p>
                </>
              ) : (
                <div className="flex h-48 items-center justify-center rounded-xl border border-dashed border-gray-200 bg-gray-50 px-4 text-sm leading-6 text-gray-400">
                  等你把微信二维码附件发我后，我会直接接到这里
                </div>
              )}
            </div>
          </div>
        </div>

        <main className="w-full overflow-hidden rounded-[2.5rem] border border-gray-100 bg-white shadow-[0_20px_50px_rgba(0,0,0,0.03)]">
          <div className="border-b border-gray-50 bg-[#FAFAFB] p-6 sm:p-10">
            {phase === 'idle' ? (
              <button
                onClick={() => void handleFileSelect()}
                className="active-scale group flex aspect-[21/9] w-full flex-col items-center justify-center rounded-[2rem] border border-dashed border-gray-200 bg-white shadow-sm transition-all duration-500 hover:border-[#DFFF00] hover:bg-[#DFFF00]/5"
                type="button"
              >
                <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-full bg-gray-50 transition-transform duration-500 group-hover:scale-110">
                  <UploadIcon className="h-6 w-6 text-gray-400 group-hover:text-[#1D1D1F]" />
                </div>
                <h3 className="mb-1 text-base font-medium text-gray-900">选择网球比赛视频</h3>
                <p className="text-xs font-medium text-gray-400">支持 4K 高清录像拖拽上传</p>
              </button>
            ) : (
              <div className="group relative flex aspect-[21/9] w-full items-center justify-center overflow-hidden rounded-[2rem] bg-[#0F172A] shadow-inner">
                {selectedVideoPreview ? (
                  <img src={selectedVideoPreview} alt="视频预览帧" className="absolute inset-0 h-full w-full object-cover" />
                ) : null}
                <div
                  className={`absolute inset-0 ${
                    selectedVideoPreview
                      ? 'bg-[radial-gradient(circle_at_top_left,rgba(223,255,0,0.18),transparent_28%),linear-gradient(135deg,rgba(15,23,42,0.12),rgba(31,41,55,0.2)_55%,rgba(17,24,39,0.3))]'
                      : 'bg-[radial-gradient(circle_at_top_left,rgba(223,255,0,0.22),transparent_28%),linear-gradient(135deg,#101828,#1F2937_55%,#111827)]'
                  }`}
                />
                <div className={`absolute inset-0 ${selectedVideoPreview ? 'bg-black/10' : 'bg-black/20'}`} />
                <div className="absolute z-10 flex flex-col items-center">
                  <div className="mb-3 rounded-2xl border border-white/20 bg-white/10 p-3 backdrop-blur-xl">
                    <FileVideoIcon className="h-8 w-8 text-white" />
                  </div>
                  <span className="rounded-full border border-white/10 bg-black/60 px-5 py-2 text-center text-xs font-medium tracking-wide text-white backdrop-blur-md">
                    {selectedFileName}
                  </span>
                </div>
                {!isProcessing ? (
                  <button
                    onClick={resetApp}
                    className="active-scale absolute right-5 top-5 rounded-full border border-white/10 bg-white/10 px-4 py-2 text-[11px] font-bold uppercase tracking-widest text-white backdrop-blur-md transition-all hover:bg-white/20"
                    type="button"
                  >
                    更换文件
                  </button>
                ) : null}
              </div>
            )}
          </div>

          <div className="flex min-h-[220px] flex-col justify-center bg-white p-8 sm:p-12">
            {(phase === 'idle' || phase === 'ready' || phase === 'failed') && (
              <div className="flex flex-col items-center">
                <div className="mb-8 flex flex-wrap justify-center gap-2">
                  {['Ace识别', '制胜分提取', '多拍追踪'].map((tag) => (
                    <span key={tag} className="rounded-full border border-gray-100 bg-gray-50 px-3 py-1 text-[10px] font-bold uppercase tracking-widest text-gray-400 italic">
                      {tag}
                    </span>
                  ))}
                </div>
                <button
                  disabled={!canStart}
                  onClick={() => void handleStartProcessing()}
                  className={`active-scale group relative flex w-full max-w-md items-center justify-center overflow-hidden rounded-2xl py-4 text-lg font-bold transition-all duration-500 ${
                    canStart
                      ? 'bg-[#1D1D1F] text-white hover:-translate-y-1 hover:bg-black hover:shadow-lg'
                      : 'cursor-not-allowed bg-gray-100 text-gray-300'
                  }`}
                  type="button"
                >
                  <SparklesIcon className={`mr-3 h-5 w-5 ${canStart ? 'text-[#DFFF00]' : ''}`} />
                  AI 智能剪辑
                  <ChevronRightIcon className="ml-2 h-4 w-4 opacity-0 transition-all group-hover:translate-x-1 group-hover:opacity-100" />
                </button>
                {error ? <p className="mt-5 text-center text-sm font-medium text-[#C0392B]">{error}</p> : null}
              </div>
            )}

            {isProcessing && (
              <div className="mx-auto w-full max-w-md py-2">
                <div className="mb-4 flex items-end justify-between gap-4">
                  <div>
                    <h3 className="flex items-center text-lg font-bold italic text-[#1D1D1F]">
                      <TennisBallIcon className="mr-2 h-5 w-5 animate-spin text-[#DFFF00]" style={{ animationDuration: '2s' }} />
                      {processingTitle}
                    </h3>
                  </div>
                  <span className="text-3xl font-black tracking-tighter text-[#1D1D1F] italic">{progress}%</span>
                </div>
                <div className="h-3 w-full overflow-hidden rounded-full border border-gray-50 bg-gray-100 p-0.5 shadow-inner">
                  <div className="animate-progress-glow relative h-full rounded-full bg-[#DFFF00] transition-all duration-500 ease-out shadow-[0_0_18px_rgba(223,255,0,0.45)]" style={{ width: `${progress}%` }}>
                    <div className="animate-shimmer absolute inset-0 bg-gradient-to-r from-transparent via-white/35 to-transparent" />
                  </div>
                </div>
                {phase === 'exporting' ? (
                  <div className="mt-4 flex items-center justify-between text-xs font-medium text-gray-500">
                    <span>预计剩余 {formatEtaLabel(exportEtaSeconds)}</span>
                    <span>{statusText || '正在导出高光视频'}</span>
                  </div>
                ) : null}
              </div>
            )}

            {showExportPanel && (
              <div className="w-full animate-fade-in">
                <div className="mb-8 flex items-center justify-center text-center">
                  <div className="flex items-center rounded-full border border-[#DFFF00]/30 bg-[#DFFF00]/10 px-5 py-2 text-xs font-bold text-[#1D1D1F] shadow-sm">
                    <CheckCircleIcon className="mr-2 h-4 w-4 text-[#88AA00]" />
                    {successMessage}
                  </div>
                </div>

                <div className="mx-auto mb-8 grid max-w-md grid-cols-2 gap-3 text-center sm:grid-cols-4">
                  <div className="rounded-2xl border border-gray-100 bg-gray-50 px-3 py-3">
                    <p className="text-[11px] font-bold uppercase tracking-widest text-gray-400">候选片段</p>
                    <p className="mt-2 text-lg font-black text-[#1D1D1F]">{prepareResult?.analysis.segment_count ?? 0}</p>
                  </div>
                  <div className="rounded-2xl border border-gray-100 bg-gray-50 px-3 py-3">
                    <p className="text-[11px] font-bold uppercase tracking-widest text-gray-400">默认保留</p>
                    <p className="mt-2 text-lg font-black text-[#1D1D1F]">{prepareResult?.review.summary.keep_count ?? 0}</p>
                  </div>
                  <div className="rounded-2xl border border-gray-100 bg-gray-50 px-3 py-3">
                    <p className="text-[11px] font-bold uppercase tracking-widest text-gray-400">原视频时长</p>
                    <p className="mt-2 text-lg font-black text-[#1D1D1F]">{prepareResult ? formatDuration(prepareResult.project.probe.duration_sec) : '--:--'}</p>
                  </div>
                  <div className="rounded-2xl border border-gray-100 bg-gray-50 px-3 py-3">
                    <p className="text-[11px] font-bold uppercase tracking-widest text-gray-400">高光总时长</p>
                    <p className="mt-2 text-lg font-black text-[#1D1D1F]">{prepareResult ? formatDuration(prepareResult.review.summary.kept_duration_sec) : '--:--'}</p>
                  </div>
                </div>

                <div className="mx-auto max-w-md space-y-8">
                  <div>
                    <div className="mb-3 flex items-center justify-between px-1">
                      <span className="text-[11px] font-black uppercase tracking-widest text-gray-400">导出模式</span>
                      <span className="rounded bg-black px-2 py-0.5 text-[11px] font-bold uppercase text-[#DFFF00]">Fast path</span>
                    </div>
                    <div className="grid gap-3">
                      {exportProfiles.map((item) => (
                        <button
                          key={item.value}
                          onClick={() => setExportProfile(item.value)}
                          className={`rounded-2xl border px-4 py-3 text-left transition-all duration-300 ${
                            exportProfile === item.value
                              ? 'border-[#DFFF00] bg-[#F9FFD9] text-[#1D1D1F] shadow-md'
                              : 'border-gray-100 bg-gray-50 text-gray-500 hover:border-gray-200 hover:text-gray-700'
                          }`}
                          type="button"
                        >
                          <div className="text-sm font-black">{item.label}</div>
                          <div className="mt-1 text-xs font-medium opacity-80">{item.hint}</div>
                        </button>
                      ))}
                    </div>
                  </div>

                  <label className="flex items-start gap-3 rounded-2xl border border-gray-100 bg-gray-50 px-4 py-4 text-sm text-gray-600">
                    <input
                      type="checkbox"
                      checked={hardwareEncode}
                      onChange={(event) => setHardwareEncode(event.target.checked)}
                      className="mt-1 h-4 w-4 rounded border-gray-300 text-black focus:ring-black"
                    />
                    <span>
                      <span className="block font-semibold text-[#1D1D1F]">硬件加速导出</span>
                      <span className="mt-1 block text-xs leading-5 text-gray-500">
                        如果电脑支持，会优先用更快的硬件编码。即使不支持，也会自动回退到普通导出，不影响生成视频。
                      </span>
                      {hardwareSupport ? (
                        <span className="mt-2 block text-xs leading-5 text-gray-500">
                          {hardwareSupport.summary}
                          {hardwareSupport.encoders.some((item) => item.available) ? (
                            <span className="mt-1 block">
                              已检测到：{hardwareSupport.encoders.filter((item) => item.available).map((item) => item.label).join('、')}
                            </span>
                          ) : null}
                        </span>
                      ) : null}
                    </span>
                  </label>

                  <div className="flex justify-center gap-3">
                    {formats.map((item) => (
                      <button
                        key={item}
                        onClick={() => setFormat(item)}
                        className={`active-scale rounded-xl border-2 px-8 py-2.5 text-xs font-bold transition-all duration-300 ${
                          format === item
                            ? 'border-black bg-black text-white shadow-lg shadow-black/10'
                            : 'border-gray-100 bg-white text-gray-400 hover:border-gray-200'
                        }`}
                        type="button"
                      >
                        {item.toUpperCase()}
                      </button>
                    ))}
                  </div>

                  <button
                    onClick={() => void handleSaveHighlights()}
                    className="active-scale flex w-full items-center justify-center rounded-2xl bg-[#DFFF00] py-5 text-lg font-black text-[#1D1D1F] shadow-[0_15px_35px_rgba(223,255,0,0.25)] transition-all duration-500 hover:-translate-y-1 hover:bg-[#EFFF22]"
                    type="button"
                  >
                    <DownloadIcon className="mr-3 h-5 w-5" />
                    {phase === 'done' ? '再次保存高光合集' : '保存高光合集'}
                  </button>

                  {exportResult ? (
                    <div className="rounded-2xl border border-gray-100 bg-gray-50 px-4 py-4 text-sm text-gray-600">
                      <p className="font-semibold text-[#1D1D1F]">最近一次导出</p>
                      <p className="mt-2">格式：{exportResult.export_format.toUpperCase()} · 分辨率：{exportResult.export_resolution === '4k' ? '4K' : exportResult.export_resolution}</p>
                      <p className="mt-1">导出片段：{exportResult.kept_segment_count} 段</p>
                    </div>
                  ) : null}

                  {error ? <p className="text-center text-sm font-medium text-[#C0392B]">{error}</p> : null}
                </div>
              </div>
            )}
          </div>
        </main>

        <footer className="mt-12 flex flex-col items-center opacity-30">
          <p className="text-[10px] font-bold uppercase tracking-[0.3em]">内部测试版 v0423</p>
          <div className="mt-2 h-0.5 w-8 rounded-full bg-gray-400" />
        </footer>
      </div>
    </div>
  )
}

export default App
