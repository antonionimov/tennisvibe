from __future__ import annotations

import math

import numpy as np


def clamp01(value: float) -> float:
    return float(max(0.0, min(1.0, value)))


def normalize_series(values: np.ndarray) -> np.ndarray:
    if values.size == 0:
        return values.astype(np.float32)

    values = values.astype(np.float32)
    minimum = float(np.min(values))
    maximum = float(np.max(values))

    if math.isclose(maximum, minimum):
        return np.zeros_like(values, dtype=np.float32)

    return (values - minimum) / (maximum - minimum)


def combine_scores(audio: np.ndarray, motion: np.ndarray, peaks: np.ndarray, sensitivity: float, visual: np.ndarray | None = None) -> np.ndarray:
    audio_weight = 0.55 + (sensitivity - 0.5) * 0.2
    motion_weight = 0.45 - (sensitivity - 0.5) * 0.2
    peak_weight = 0.18
    visual_weight = 0.22

    combined = audio * audio_weight + motion * motion_weight + peaks * peak_weight
    if visual is not None and visual.size > 0:
        combined = combined + visual * visual_weight

    return np.clip(combined, 0.0, 1.0)


def detection_threshold(combined: np.ndarray, sensitivity: float) -> float:
    if combined.size == 0:
        return 1.0

    baseline = float(np.quantile(combined, 0.68))
    dynamic = 0.42 - (sensitivity - 0.5) * 0.22
    return clamp01(max(dynamic, baseline * 0.92))
