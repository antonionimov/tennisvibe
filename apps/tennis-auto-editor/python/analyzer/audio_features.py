from __future__ import annotations

import math
from pathlib import Path

import numpy as np
from scipy import signal
from scipy.io import wavfile

from analyzer.scoring import normalize_series

FRAME_SEC = 0.02
HOP_SEC = 0.01
MIN_HIT_SEPARATION_SEC = 0.12
MIN_HIT_DURATION_SEC = 0.02
MAX_HIT_DURATION_SEC = 0.18
AUDIO_BAND_LOW_HZ = 700.0
AUDIO_BAND_HIGH_HZ = 4500.0


def _safe_kernel_size(target: int, values_size: int) -> int:
    if values_size <= 1:
        return 1

    kernel = min(target, values_size if values_size % 2 == 1 else values_size - 1)
    if kernel < 3:
        return 1
    if kernel % 2 == 0:
        kernel -= 1
    return max(1, kernel)


def _rolling_median(values: np.ndarray, frames: int) -> np.ndarray:
    if values.size == 0:
        return values.astype(np.float32)

    kernel = _safe_kernel_size(frames, values.size)
    if kernel <= 1:
        return values.astype(np.float32)

    return signal.medfilt(values.astype(np.float32), kernel_size=kernel)


def _bandpass_filter(data: np.ndarray, sample_rate: int) -> np.ndarray:
    nyquist = sample_rate / 2.0
    low_hz = min(AUDIO_BAND_LOW_HZ, nyquist * 0.45)
    high_hz = min(AUDIO_BAND_HIGH_HZ, nyquist * 0.92)

    if high_hz <= low_hz + 50.0:
        return data.astype(np.float32)

    sos = signal.butter(4, [low_hz / nyquist, high_hz / nyquist], btype='bandpass', output='sos')
    filtered = signal.sosfiltfilt(sos, data.astype(np.float32))
    return filtered.astype(np.float32)


def _frame_signal(data: np.ndarray, frame_size: int, hop_size: int) -> tuple[np.ndarray, np.ndarray]:
    if data.size == 0:
        return np.empty((0, frame_size), dtype=np.float32), np.empty(0, dtype=np.int32)

    frames: list[np.ndarray] = []
    starts: list[int] = []
    cursor = 0

    while cursor < data.size:
        chunk = data[cursor : cursor + frame_size]
        if chunk.size < frame_size:
            chunk = np.pad(chunk, (0, frame_size - chunk.size), mode='constant')
        frames.append(chunk.astype(np.float32))
        starts.append(cursor)
        if cursor + frame_size >= data.size:
            break
        cursor += hop_size

    return np.asarray(frames, dtype=np.float32), np.asarray(starts, dtype=np.int32)


def _prepare_audio(data: np.ndarray) -> np.ndarray:
    if data.size == 0:
        return data.astype(np.float32)

    data = data.astype(np.float32)
    scale = float(np.percentile(np.abs(data), 99.5))
    if scale > 0:
        data = data / scale
    return np.clip(data, -1.0, 1.0).astype(np.float32)


def extract_audio_features(audio_path: str | Path, window_sec: float = 0.25) -> dict:
    sample_rate, data = wavfile.read(str(audio_path))

    if data.ndim > 1:
        data = np.mean(data, axis=1)

    data = _prepare_audio(data)
    if data.size == 0:
        return {
            'window_sec': window_sec,
            'duration_sec': 0.0,
            'sample_rate': int(sample_rate),
            'levels': [],
            'peaks': [],
            'raw_rms': [],
            'hit_times_sec': [],
            'hit_strengths': [],
            'hit_count': 0,
            'frame_hop_sec': HOP_SEC,
        }

    filtered = _bandpass_filter(data, int(sample_rate))
    frame_size = max(1, int(sample_rate * FRAME_SEC))
    hop_size = max(1, int(sample_rate * HOP_SEC))
    frames, frame_starts = _frame_signal(filtered, frame_size, hop_size)

    if frames.size == 0:
        return {
            'window_sec': window_sec,
            'duration_sec': float(data.size / sample_rate),
            'sample_rate': int(sample_rate),
            'levels': [],
            'peaks': [],
            'raw_rms': [],
            'hit_times_sec': [],
            'hit_strengths': [],
            'hit_count': 0,
            'frame_hop_sec': HOP_SEC,
        }

    analysis_window = np.hanning(frame_size).astype(np.float32)
    weighted_frames = frames * analysis_window
    rms = np.sqrt(np.mean(np.square(weighted_frames), axis=1))
    peak = np.max(np.abs(frames), axis=1)
    spectra = np.abs(np.fft.rfft(weighted_frames, axis=1))
    spectral_flux = np.concatenate(
        [np.zeros(1, dtype=np.float32), np.sum(np.maximum(0.0, spectra[1:] - spectra[:-1]), axis=1).astype(np.float32)]
    )

    rms_db = 20.0 * np.log10(np.maximum(rms, 1e-5))
    peak_db = 20.0 * np.log10(np.maximum(peak, 1e-5))
    baseline_db = _rolling_median(rms_db, int(round(1.2 / HOP_SEC)))
    db_over_baseline = np.maximum(0.0, peak_db - baseline_db)

    rms_norm = normalize_series(np.log1p(rms * 40.0))
    peak_norm = normalize_series(np.log1p(peak * 50.0))
    flux_norm = normalize_series(np.log1p(spectral_flux))
    delta_norm = normalize_series(db_over_baseline)
    transient_score = np.clip(peak_norm * 0.45 + flux_norm * 0.35 + delta_norm * 0.20, 0.0, 1.0)

    flux_baseline = _rolling_median(flux_norm, int(round(0.9 / HOP_SEC)))
    candidate_mask = (db_over_baseline >= 8.0) & (flux_norm >= np.maximum(flux_baseline + 0.08, 0.14))

    peak_indexes, _ = signal.find_peaks(
        transient_score,
        distance=max(1, int(round(MIN_HIT_SEPARATION_SEC / HOP_SEC))),
        prominence=max(0.08, float(np.std(transient_score)) * 0.45),
    )

    hit_times_sec: list[float] = []
    hit_strengths: list[float] = []
    event_indexes: list[int] = []

    for peak_index in peak_indexes.tolist():
        left = peak_index
        right = peak_index
        threshold = max(transient_score[peak_index] * 0.45, 0.12)

        while left > 0 and transient_score[left - 1] >= threshold:
            left -= 1
        while right + 1 < transient_score.size and transient_score[right + 1] >= threshold:
            right += 1

        duration_sec = (right - left + 1) * HOP_SEC
        if duration_sec < MIN_HIT_DURATION_SEC or duration_sec > MAX_HIT_DURATION_SEC:
            continue

        vicinity_start = max(0, peak_index - 1)
        vicinity_end = min(candidate_mask.size, peak_index + 2)
        if not bool(np.any(candidate_mask[vicinity_start:vicinity_end])):
            continue

        strength = float(np.clip(transient_score[peak_index] * 0.55 + delta_norm[peak_index] * 0.25 + flux_norm[peak_index] * 0.20, 0.0, 1.0))
        if strength < 0.22:
            continue

        event_time_sec = float(frame_starts[peak_index] / sample_rate + FRAME_SEC / 2.0)
        if hit_times_sec and event_time_sec - hit_times_sec[-1] < MIN_HIT_SEPARATION_SEC:
            if strength > hit_strengths[-1]:
                hit_times_sec[-1] = round(event_time_sec, 3)
                hit_strengths[-1] = round(strength, 4)
                event_indexes[-1] = peak_index
            continue

        hit_times_sec.append(round(event_time_sec, 3))
        hit_strengths.append(round(strength, 4))
        event_indexes.append(peak_index)

    duration_sec = float(data.size / sample_rate)
    samples_per_window = max(1, int(sample_rate * window_sec))
    window_count = max(1, math.ceil(data.size / samples_per_window))
    activity_levels = np.zeros(window_count, dtype=np.float32)
    hit_marks = np.zeros(window_count, dtype=np.float32)

    for frame_index, start in enumerate(frame_starts.tolist()):
        timestamp_sec = start / sample_rate
        window_index = min(window_count - 1, int(timestamp_sec / window_sec))
        activity_levels[window_index] = max(
            float(activity_levels[window_index]),
            float(transient_score[frame_index] * 0.7 + rms_norm[frame_index] * 0.3),
        )

    for hit_time_sec, hit_strength in zip(hit_times_sec, hit_strengths):
        window_index = min(window_count - 1, int(hit_time_sec / window_sec))
        hit_marks[window_index] = max(float(hit_marks[window_index]), float(hit_strength))
        activity_levels[window_index] = max(float(activity_levels[window_index]), float(0.75 + hit_strength * 0.25))

    return {
        'window_sec': window_sec,
        'duration_sec': duration_sec,
        'sample_rate': int(sample_rate),
        'levels': normalize_series(activity_levels).tolist(),
        'peaks': normalize_series(hit_marks).tolist(),
        'raw_rms': rms.tolist(),
        'hit_times_sec': hit_times_sec,
        'hit_strengths': hit_strengths,
        'hit_count': len(hit_times_sec),
        'frame_hop_sec': HOP_SEC,
    }
