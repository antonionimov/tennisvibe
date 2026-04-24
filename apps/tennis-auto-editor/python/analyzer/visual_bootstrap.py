from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import cv2
import numpy as np

ROOT_DIR = Path(__file__).resolve().parents[1]


@dataclass(slots=True)
class VisualBootstrapResult:
    enabled: bool
    source: str
    visual_levels: list[float]
    shot_times_sec: list[float]
    metadata: dict[str, Any]


# Adapted from Tennis-Vision's ball trajectory smoothing and shot-frame detection ideas.
def _moving_average(values: np.ndarray, window_size: int) -> np.ndarray:
    if values.size == 0 or window_size <= 1:
        return values.astype(np.float32)

    padded = np.pad(values.astype(np.float32), (window_size // 2, window_size - 1 - window_size // 2), mode='edge')
    kernel = np.ones(window_size, dtype=np.float32) / float(window_size)
    return np.convolve(padded, kernel, mode='valid')


def _interpolate_positions(positions: list[list[float] | None]) -> np.ndarray:
    if not positions:
        return np.empty((0, 4), dtype=np.float32)

    array = np.full((len(positions), 4), np.nan, dtype=np.float32)
    for index, bbox in enumerate(positions):
        if bbox is None or len(bbox) != 4:
            continue
        array[index] = np.asarray(bbox, dtype=np.float32)

    if np.isnan(array).all():
        return np.zeros_like(array)

    for column in range(array.shape[1]):
        column_values = array[:, column]
        valid = np.flatnonzero(~np.isnan(column_values))
        if valid.size == 0:
            array[:, column] = 0.0
            continue
        if valid.size == 1:
            array[:, column] = float(column_values[valid[0]])
            continue
        array[:, column] = np.interp(np.arange(len(column_values)), valid, column_values[valid])

    return array


def _detect_shot_frames(ball_positions: np.ndarray, fps: float) -> list[int]:
    if ball_positions.size == 0:
        return []

    mid_y = (ball_positions[:, 1] + ball_positions[:, 3]) / 2.0
    smoothed_y = _moving_average(mid_y, 5)
    delta_y = np.diff(smoothed_y, prepend=smoothed_y[0])

    minimum_change_frames = max(8, int(round(fps * 0.9)))
    lookahead = max(minimum_change_frames + 1, int(round(minimum_change_frames * 1.2)))
    shot_frames: list[int] = []

    for index in range(1, len(delta_y) - lookahead):
        negative_change = delta_y[index] > 0 and delta_y[index + 1] < 0
        positive_change = delta_y[index] < 0 and delta_y[index + 1] > 0
        if not (negative_change or positive_change):
            continue

        change_count = 0
        for next_index in range(index + 1, min(len(delta_y), index + lookahead + 1)):
            if negative_change and delta_y[next_index] < 0:
                change_count += 1
            elif positive_change and delta_y[next_index] > 0:
                change_count += 1

        if change_count >= minimum_change_frames - 1:
            if not shot_frames or index - shot_frames[-1] >= max(4, minimum_change_frames // 3):
                shot_frames.append(index)

    return shot_frames


def _levels_from_shots(shot_times_sec: list[float], duration_sec: float, window_sec: float) -> list[float]:
    if duration_sec <= 0 or window_sec <= 0:
        return []

    window_count = max(1, int(np.ceil(duration_sec / window_sec)))
    levels = np.zeros(window_count, dtype=np.float32)
    spread = 2

    for shot_time in shot_times_sec:
        center = int(shot_time / window_sec)
        for offset in range(-spread, spread + 1):
            index = center + offset
            if index < 0 or index >= window_count:
                continue
            score = max(0.0, 1.0 - abs(offset) * 0.35)
            levels[index] = max(levels[index], score)

    return levels.tolist()


def _resolve_ball_model(project_dir: Path) -> Path | None:
    env_path = os.environ.get('TENNIS_VISION_BALL_MODEL')
    candidates = [
        Path(env_path).expanduser() if env_path else None,
        project_dir / 'tennis_ball_light_yolo11s.pt',
        project_dir / 'models' / 'candidates' / 'tennis_ball_light_yolo11s.pt',
        ROOT_DIR / 'models' / 'candidates' / 'tennis_ball_light_yolo11s.pt',
        project_dir / 'tennis_vision_ball.pt',
        ROOT_DIR / 'models' / 'tennis_vision_ball.pt',
        project_dir / 'last.pt',
        ROOT_DIR / 'models' / 'last.pt',
        project_dir / 'tennis_ball_tiny_yolov12n_last.pt',
        project_dir / 'models' / 'candidates' / 'tennis_ball_tiny_yolov12n_last.pt',
        ROOT_DIR / 'models' / 'candidates' / 'tennis_ball_tiny_yolov12n_last.pt',
    ]

    for candidate in candidates:
        if candidate and candidate.exists():
            return candidate.resolve()
    return None


def extract_visual_bootstrap(video_path: str | Path, project_dir: str | Path, window_sec: float) -> VisualBootstrapResult:
    project_dir = Path(project_dir).resolve()
    video_path = Path(video_path).resolve()
    ball_model = _resolve_ball_model(project_dir)
    if ball_model is None:
        return VisualBootstrapResult(
            enabled=False,
            source='tennis_vision',
            visual_levels=[],
            shot_times_sec=[],
            metadata={'reason': 'missing_ball_model'},
        )

    try:
        from ultralytics import YOLO
    except Exception as exc:  # pragma: no cover - depends on optional local install
        return VisualBootstrapResult(
            enabled=False,
            source='tennis_vision',
            visual_levels=[],
            shot_times_sec=[],
            metadata={'reason': 'ultralytics_unavailable', 'error': str(exc), 'ball_model_path': str(ball_model)},
        )

    capture = cv2.VideoCapture(str(video_path))
    if not capture.isOpened():
        return VisualBootstrapResult(
            enabled=False,
            source='tennis_vision',
            visual_levels=[],
            shot_times_sec=[],
            metadata={'reason': 'video_open_failed', 'video_path': str(video_path)},
        )

    fps = capture.get(cv2.CAP_PROP_FPS)
    if fps <= 0:
        fps = 25.0
    frame_count = int(capture.get(cv2.CAP_PROP_FRAME_COUNT) or 0)
    duration_sec = frame_count / fps if frame_count > 0 else 0.0

    visual_enabled = os.environ.get('TENNIS_VISION_ENABLE', '0').lower() in {'1', 'true', 'yes', 'on'}
    force_visual = os.environ.get('TENNIS_VISION_FORCE', '0').lower() in {'1', 'true', 'yes', 'on'}
    if not visual_enabled and not force_visual:
        capture.release()
        return VisualBootstrapResult(
            enabled=False,
            source='tennis_vision',
            visual_levels=[],
            shot_times_sec=[],
            metadata={
                'reason': 'visual_bootstrap_disabled_by_default',
                'ball_model_path': str(ball_model),
            },
        )

    max_safe_duration_sec = float(os.environ.get('TENNIS_VISION_MAX_SAFE_DURATION_SEC', '120'))
    if duration_sec > max_safe_duration_sec and not force_visual:
        capture.release()
        return VisualBootstrapResult(
            enabled=False,
            source='tennis_vision',
            visual_levels=[],
            shot_times_sec=[],
            metadata={
                'reason': 'video_too_long_for_safe_cpu_bootstrap',
                'duration_sec': round(duration_sec, 3),
                'max_safe_duration_sec': round(max_safe_duration_sec, 3),
                'ball_model_path': str(ball_model),
            },
        )

    frame_stride = max(1, int(os.environ.get('TENNIS_VISION_FRAME_STRIDE', '10')))
    max_frames = int(os.environ.get('TENNIS_VISION_MAX_FRAMES', '0'))
    imgsz = max(160, int(os.environ.get('TENNIS_VISION_IMGSZ', '320')))
    confidence = float(os.environ.get('TENNIS_VISION_CONFIDENCE', '0.20'))

    model = YOLO(str(ball_model))
    positions: list[list[float] | None] = []
    processed_frames = 0
    frame_index = 0

    while True:
        ok, frame = capture.read()
        if not ok:
            break

        if max_frames > 0 and frame_index >= max_frames:
            break

        if frame_index % frame_stride != 0:
            positions.append(None)
            frame_index += 1
            continue

        results = model.predict(frame, conf=confidence, imgsz=imgsz, max_det=1, device='cpu', verbose=False)[0]
        best_bbox: list[float] | None = None
        best_confidence = -1.0

        boxes = getattr(results, 'boxes', None)
        if boxes is not None and len(boxes) > 0:
            xyxy = boxes.xyxy.tolist()
            confidences = boxes.conf.tolist() if getattr(boxes, 'conf', None) is not None else [1.0] * len(xyxy)
            for bbox, confidence in zip(xyxy, confidences):
                if len(bbox) != 4:
                    continue
                x1, y1, x2, y2 = bbox
                width = x2 - x1
                height = y2 - y1
                if width < 3 or height < 3 or width > 60 or height > 60:
                    continue
                aspect_ratio = width / height if height else 0.0
                if not 0.55 <= aspect_ratio <= 1.45:
                    continue
                if confidence > best_confidence:
                    best_confidence = float(confidence)
                    best_bbox = [float(x1), float(y1), float(x2), float(y2)]

        positions.append(best_bbox)
        processed_frames += 1
        frame_index += 1

    capture.release()

    interpolated = _interpolate_positions(positions)
    shot_frames = _detect_shot_frames(interpolated, fps=float(fps))
    shot_times_sec = [round(frame / float(fps), 3) for frame in shot_frames]
    visual_levels = _levels_from_shots(shot_times_sec, duration_sec=duration_sec, window_sec=window_sec)

    return VisualBootstrapResult(
        enabled=True,
        source='tennis_vision',
        visual_levels=visual_levels,
        shot_times_sec=shot_times_sec,
        metadata={
            'ball_model_path': str(ball_model),
            'fps': round(float(fps), 4),
            'frame_count': frame_count,
            'processed_frames': processed_frames,
            'frame_stride': frame_stride,
            'max_frames': max_frames,
            'duration_sec': round(duration_sec, 3),
            'imgsz': imgsz,
            'confidence': round(confidence, 3),
            'shot_frame_count': len(shot_frames),
        },
    )
