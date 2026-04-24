from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(slots=True)
class WindowScore:
    timestamp_sec: float
    duration_sec: float
    audio_level: float
    motion_level: float
    peak_level: float
    combined_score: float


@dataclass(slots=True)
class AnalysisSegment:
    segment_id: str
    start_sec: float
    end_sec: float
    duration_sec: float
    confidence: float
    score: float
    reasons: list[str] = field(default_factory=list)
    metrics: dict[str, float] = field(default_factory=dict)


@dataclass(slots=True)
class AnalysisSummary:
    duration_sec: float
    window_sec: float
    segment_count: int
    total_candidate_duration_sec: float
    average_confidence: float
    sensitivity: float


@dataclass(slots=True)
class AnalysisResult:
    project_id: str
    created_at: str
    sensitivity: float
    source_video_path: str
    proxy_path: str
    audio_path: str
    summary: AnalysisSummary
    segments: list[AnalysisSegment]
    debug: dict[str, Any] = field(default_factory=dict)
