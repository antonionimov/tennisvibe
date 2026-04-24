from __future__ import annotations

from pathlib import Path

import cv2
import numpy as np
from scipy import signal

from analyzer.scoring import normalize_series


MAX_WIDTH = 160


def _prepare_frame(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    scale = min(1.0, MAX_WIDTH / max(width, 1))
    if scale < 1.0:
        frame = cv2.resize(frame, (int(width * scale), int(height * scale)), interpolation=cv2.INTER_AREA)

    gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
    return cv2.GaussianBlur(gray, (5, 5), 0)


def _smooth_series(values: np.ndarray) -> np.ndarray:
    if values.size < 5:
        return values

    window_length = min(values.size if values.size % 2 == 1 else values.size - 1, 9)
    if window_length < 5:
        return values

    return signal.savgol_filter(values, window_length=window_length, polyorder=2, mode='interp')


def extract_video_features(video_path: str | Path, window_sec: float = 0.5) -> dict:
    capture = cv2.VideoCapture(str(video_path))
    if not capture.isOpened():
        raise RuntimeError(f'无法打开视频文件: {video_path}')

    fps = capture.get(cv2.CAP_PROP_FPS)
    if fps <= 0:
        fps = 25.0

    sample_every = max(1, int(round(fps * window_sec)))
    frame_index = 0
    sampled = 0
    previous_frame: np.ndarray | None = None
    motion_values: list[float] = []

    while True:
        ok, frame = capture.read()
        if not ok:
            break

        if frame_index % sample_every != 0:
            frame_index += 1
            continue

        prepared = _prepare_frame(frame)
        if previous_frame is None:
            motion_values.append(0.0)
        else:
            frame_diff = cv2.absdiff(prepared, previous_frame)
            edges_prev = cv2.Canny(previous_frame, 40, 120)
            edges_next = cv2.Canny(prepared, 40, 120)
            edge_diff = cv2.absdiff(edges_next, edges_prev)
            motion_score = float(frame_diff.mean() / 255.0)
            edge_score = float(edge_diff.mean() / 255.0)
            motion_values.append(motion_score * 0.72 + edge_score * 0.28)

        previous_frame = prepared
        sampled += 1
        frame_index += 1

    capture.release()

    motion_array = np.asarray(motion_values, dtype=np.float32)
    smooth_motion = _smooth_series(motion_array)
    normalized_motion = normalize_series(np.maximum(smooth_motion, 0.0))

    return {
        'window_sec': window_sec,
        'fps': float(fps),
        'sample_count': sampled,
        'levels': normalized_motion.tolist(),
        'raw_motion': motion_array.tolist(),
    }
