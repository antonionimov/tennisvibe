from __future__ import annotations

from typing import Any

import numpy as np

from analyzer.scoring import clamp01
from analyzer.types import AnalysisSegment


def _activity_slice(activity_levels: np.ndarray, window_sec: float, start_sec: float, end_sec: float) -> np.ndarray:
    if activity_levels.size == 0 or window_sec <= 0:
        return np.asarray([], dtype=np.float32)

    start_index = max(0, int(start_sec / window_sec))
    end_index = min(activity_levels.size, max(start_index + 1, int(np.ceil(end_sec / window_sec))))
    if end_index <= start_index:
        return np.asarray([], dtype=np.float32)
    return activity_levels[start_index:end_index]


def _segment_activity_mean(activity_levels: np.ndarray, window_sec: float, start_sec: float, end_sec: float) -> float:
    window_values = _activity_slice(activity_levels, window_sec, start_sec, end_sec)
    return float(np.mean(window_values)) if window_values.size > 0 else 0.0


def _activity_profile_stats(activity_levels: np.ndarray, window_sec: float, start_sec: float, end_sec: float) -> tuple[float, float]:
    window_values = _activity_slice(activity_levels, window_sec, start_sec, end_sec)
    if window_values.size == 0:
        return 0.0, 1.0

    high_activity_ratio = float(np.mean(window_values >= 0.35))
    very_low_activity_ratio = float(np.mean(window_values <= 0.12))
    return high_activity_ratio, very_low_activity_ratio


def _reference_hit_strength_threshold(segments: list[AnalysisSegment]) -> tuple[float | None, dict[str, float]]:
    reference_strengths = [
        float(segment.metrics.get('hit_strength_mean', 0.0))
        for segment in segments
        if segment.metrics.get('hit_count', 0.0) >= 3.0 and segment.duration_sec >= 4.0
    ]

    if len(reference_strengths) < 10:
        return None, {
            'reference_segment_count': float(len(reference_strengths)),
        }

    strengths = np.asarray(reference_strengths, dtype=np.float32)
    median = float(np.median(strengths))
    mad = float(np.median(np.abs(strengths - median)))
    threshold = median - 2.0 * mad

    return threshold, {
        'reference_segment_count': float(len(reference_strengths)),
        'reference_hit_strength_median': round(median, 4),
        'reference_hit_strength_mad': round(mad, 4),
        'reference_hit_strength_threshold': round(threshold, 4),
    }


def _drop_short_weak_single_hit_segments(segments: list[AnalysisSegment]) -> tuple[list[AnalysisSegment], dict[str, Any]]:
    threshold, debug = _reference_hit_strength_threshold(segments)
    if threshold is None:
        return segments, {
            **debug,
            'dropped_short_weak_single_hits': [],
        }

    kept: list[AnalysisSegment] = []
    dropped_segment_ids: list[str] = []
    for segment in segments:
        hit_count = float(segment.metrics.get('hit_count', 0.0))
        hit_strength_mean = float(segment.metrics.get('hit_strength_mean', 0.0))

        if hit_count == 1.0 and segment.duration_sec <= 2.3 and hit_strength_mean < threshold:
            dropped_segment_ids.append(segment.segment_id)
            continue

        kept.append(segment)

    for index, segment in enumerate(kept, start=1):
        segment.segment_id = f'segment-{index:03d}'

    return kept, {
        **debug,
        'dropped_short_weak_single_hits': dropped_segment_ids,
    }


def _group_hits(hit_times: list[float], hit_strengths: list[float], max_intra_gap_sec: float) -> list[dict[str, Any]]:
    if not hit_times:
        return []

    clusters: list[dict[str, Any]] = []
    current = {
        'hits': [float(hit_times[0])],
        'strengths': [float(hit_strengths[0])],
        'merge_reasons': [],
    }

    for hit_time, hit_strength in zip(hit_times[1:], hit_strengths[1:]):
        if float(hit_time) - float(current['hits'][-1]) <= max_intra_gap_sec:
            current['hits'].append(float(hit_time))
            current['strengths'].append(float(hit_strength))
            continue

        clusters.append(current)
        current = {
            'hits': [float(hit_time)],
            'strengths': [float(hit_strength)],
            'merge_reasons': [],
        }

    clusters.append(current)
    return clusters


def _merge_serve_retry_clusters(
    clusters: list[dict[str, Any]],
    strong_single_hit_threshold: float,
    serve_retry_gap_min_sec: float,
    serve_retry_gap_max_sec: float,
) -> tuple[list[dict[str, Any]], int]:
    if not clusters:
        return [], 0

    merged: list[dict[str, Any]] = []
    merge_count = 0
    index = 0

    while index < len(clusters):
        current = {
            'hits': list(clusters[index]['hits']),
            'strengths': list(clusters[index]['strengths']),
            'merge_reasons': list(clusters[index].get('merge_reasons', [])),
        }

        while index + 1 < len(clusters):
            next_cluster = clusters[index + 1]
            gap_sec = float(next_cluster['hits'][0] - current['hits'][-1])
            is_singleton = len(current['hits']) == 1
            is_strong_singleton = is_singleton and float(current['strengths'][0]) >= strong_single_hit_threshold + 0.04
            next_hit_count = len(next_cluster['hits'])
            next_has_reliable_continuation = next_hit_count >= 2 and (
                next_hit_count >= 3 or max(float(value) for value in next_cluster['strengths']) >= strong_single_hit_threshold + 0.05
            )

            if is_strong_singleton and next_has_reliable_continuation and serve_retry_gap_min_sec <= gap_sec <= serve_retry_gap_max_sec:
                current['hits'].extend(float(value) for value in next_cluster['hits'])
                current['strengths'].extend(float(value) for value in next_cluster['strengths'])
                current['merge_reasons'].append('serve_retry_merged')
                merge_count += 1
                index += 1
                continue

            break

        merged.append(current)
        index += 1

    return merged, merge_count


def _split_oversized_clusters(clusters: list[dict[str, Any]], max_cluster_span_sec: float) -> list[dict[str, Any]]:
    if not clusters:
        return []

    split_clusters: list[dict[str, Any]] = []
    for cluster in clusters:
        current_hits: list[float] = []
        current_strengths: list[float] = []
        merge_reasons = list(cluster.get('merge_reasons', []))

        for hit_time, hit_strength in zip(cluster['hits'], cluster['strengths']):
            hit_time = float(hit_time)
            hit_strength = float(hit_strength)
            if current_hits and hit_time - current_hits[0] > max_cluster_span_sec:
                split_clusters.append(
                    {
                        'hits': list(current_hits),
                        'strengths': list(current_strengths),
                        'merge_reasons': list(merge_reasons),
                    }
                )
                current_hits = []
                current_strengths = []

            current_hits.append(hit_time)
            current_strengths.append(hit_strength)

        if current_hits:
            split_clusters.append(
                {
                    'hits': list(current_hits),
                    'strengths': list(current_strengths),
                    'merge_reasons': list(merge_reasons),
                }
            )

    return split_clusters


def _estimate_post_roll_sec(
    activity_levels: np.ndarray,
    window_sec: float,
    last_hit_sec: float,
    hit_count: int,
    mean_strength: float,
    sensitivity: float,
) -> float:
    if activity_levels.size == 0 or window_sec <= 0:
        return round(0.7 + sensitivity * 0.25, 3)

    tail_start_index = min(activity_levels.size - 1, max(0, int(last_hit_sec / window_sec)))
    tail_end_index = min(activity_levels.size, tail_start_index + max(1, int(round(0.8 / window_sec))))
    tail_activity_mean = float(np.mean(activity_levels[tail_start_index:tail_end_index])) if tail_end_index > tail_start_index else 0.0

    post_roll_sec = 0.55 + sensitivity * 0.20
    if hit_count >= 6:
        post_roll_sec += 0.15
    elif hit_count >= 3:
        post_roll_sec += 0.08

    if mean_strength >= 0.75:
        post_roll_sec += 0.10

    if tail_activity_mean >= 0.42:
        post_roll_sec += 0.12
    elif tail_activity_mean <= 0.18:
        post_roll_sec -= 0.10

    return round(float(np.clip(post_roll_sec, 0.45, 1.10)), 3)


def _estimate_end_hold_sec(
    activity_levels: np.ndarray,
    window_sec: float,
    hit_times: list[float],
    hit_strengths: list[float],
    last_hit_sec: float,
    hit_count: int,
    mean_strength: float,
    sensitivity: float,
) -> tuple[float, dict[str, float]]:
    base_post_roll_sec = _estimate_post_roll_sec(
        activity_levels=activity_levels,
        window_sec=window_sec,
        last_hit_sec=last_hit_sec,
        hit_count=hit_count,
        mean_strength=mean_strength,
        sensitivity=sensitivity,
    )

    if hit_count < 4 or window_sec <= 0 or activity_levels.size == 0:
        return base_post_roll_sec, {
            'post_roll_sec': round(base_post_roll_sec, 4),
            'recent_gap_median_sec': 0.0,
            'expected_return_window_sec': 0.0,
            'tail_live_activity_mean': 0.0,
            'tail_live_activity_peak': 0.0,
            'tail_live_hold_applied': 0.0,
            'tail_last_live_offset_sec': 0.0,
            'tail_dead_run_found': 0.0,
        }

    recent_gaps = np.diff(np.asarray(hit_times[-4:], dtype=np.float32))
    recent_gap_median_sec = float(np.median(recent_gaps)) if recent_gaps.size > 0 else 0.0
    expected_return_window_sec = float(np.clip(recent_gap_median_sec * 1.35, 0.75, 1.65))

    live_tail_values = _activity_slice(
        activity_levels,
        window_sec,
        last_hit_sec,
        last_hit_sec + expected_return_window_sec,
    )
    tail_live_activity_mean = float(np.mean(live_tail_values)) if live_tail_values.size > 0 else 0.0
    tail_live_activity_peak = float(np.max(live_tail_values)) if live_tail_values.size > 0 else 0.0

    trailing_hit_strength_mean = float(np.mean(hit_strengths[-3:])) if hit_strengths else 0.0
    has_live_tail_signal = (
        tail_live_activity_mean >= 0.16
        or tail_live_activity_peak >= 0.24
        or (recent_gap_median_sec <= 0.95 and trailing_hit_strength_mean >= 0.74)
    )

    protect_window_sec = max(base_post_roll_sec, expected_return_window_sec if has_live_tail_signal else min(expected_return_window_sec, 0.95))

    max_search_sec = float(np.clip(protect_window_sec + 1.0, 1.25, 2.2))
    future_values = _activity_slice(
        activity_levels,
        window_sec,
        last_hit_sec,
        last_hit_sec + max_search_sec,
    )

    protect_windows = max(1, int(np.ceil(protect_window_sec / window_sec)))
    dead_ball_threshold = 0.14
    live_activity_threshold = 0.18
    dead_run_start_sec = max_search_sec
    dead_run_found = False

    if future_values.size > protect_windows:
        for index in range(protect_windows, future_values.size - 1):
            if future_values[index] <= dead_ball_threshold and future_values[index + 1] <= dead_ball_threshold:
                dead_run_start_sec = index * window_sec
                dead_run_found = True
                break

    tail_last_live_offset_sec = 0.0
    if future_values.size > 0:
        for index, value in enumerate(future_values.tolist()):
            if float(value) >= live_activity_threshold:
                tail_last_live_offset_sec = (index + 1) * window_sec

    end_hold_sec = protect_window_sec
    if dead_run_found:
        end_hold_sec = max(end_hold_sec, dead_run_start_sec)
    elif tail_last_live_offset_sec > 0.0:
        end_hold_sec = max(end_hold_sec, tail_last_live_offset_sec)

    end_hold_sec = round(float(np.clip(end_hold_sec, 0.45, 1.65)), 3)
    return end_hold_sec, {
        'post_roll_sec': round(base_post_roll_sec, 4),
        'recent_gap_median_sec': round(recent_gap_median_sec, 4),
        'expected_return_window_sec': round(expected_return_window_sec, 4),
        'tail_live_activity_mean': round(tail_live_activity_mean, 4),
        'tail_live_activity_peak': round(tail_live_activity_peak, 4),
        'tail_live_hold_applied': 1.0 if has_live_tail_signal else 0.0,
        'tail_last_live_offset_sec': round(tail_last_live_offset_sec, 4),
        'tail_dead_run_found': 1.0 if dead_run_found else 0.0,
    }


def _trim_terminal_bounce_cluster(
    hit_times: list[float],
    hit_strengths: list[float],
) -> tuple[list[float], list[float], dict[str, float]]:
    if len(hit_times) < 4:
        return hit_times, hit_strengths, {
            'terminal_bounce_trimmed': 0.0,
            'terminal_bounce_trimmed_hits': 0.0,
            'terminal_bounce_gap_before_sec': 0.0,
        }

    candidate_index: int | None = None
    candidate_gap = 0.0
    total_hit_span = max(hit_times[-1] - hit_times[0], 1e-6)

    for gap_index, gap_sec in enumerate(np.diff(np.asarray(hit_times, dtype=np.float32)).tolist()):
        split_index = gap_index + 1
        trailing_count = len(hit_times) - split_index
        if trailing_count < 2 or trailing_count > 4:
            continue

        trailing_span_sec = hit_times[-1] - hit_times[split_index]
        trailing_start_ratio = (hit_times[split_index] - hit_times[0]) / total_hit_span
        if gap_sec >= 1.95 and trailing_span_sec <= 2.8 and trailing_start_ratio >= 0.65:
            candidate_index = split_index
            candidate_gap = float(gap_sec)

    if candidate_index is None:
        return hit_times, hit_strengths, {
            'terminal_bounce_trimmed': 0.0,
            'terminal_bounce_trimmed_hits': 0.0,
            'terminal_bounce_gap_before_sec': 0.0,
        }

    trimmed_times = hit_times[:candidate_index]
    trimmed_strengths = hit_strengths[:candidate_index]
    trimmed_count = len(hit_times) - len(trimmed_times)

    if len(trimmed_times) >= 3:
        return trimmed_times, trimmed_strengths, {
            'terminal_bounce_trimmed': 1.0,
            'terminal_bounce_trimmed_hits': float(trimmed_count),
            'terminal_bounce_gap_before_sec': round(candidate_gap, 4),
        }

    return [], [], {
        'terminal_bounce_trimmed': 1.0,
        'terminal_bounce_trimmed_hits': float(len(hit_times)),
        'terminal_bounce_gap_before_sec': round(candidate_gap, 4),
    }


def _build_segment_from_cluster(
    cluster: dict[str, Any],
    segment_number: int,
    duration_sec: float,
    activity_levels: np.ndarray,
    window_sec: float,
    pre_roll_sec: float,
    sensitivity: float,
    strong_single_hit_threshold: float,
) -> AnalysisSegment | None:
    hit_times = [float(value) for value in cluster['hits']]
    hit_strengths = [float(value) for value in cluster['strengths']]
    hit_times, hit_strengths, terminal_bounce_debug = _trim_terminal_bounce_cluster(hit_times, hit_strengths)
    hit_count = len(hit_times)

    if hit_count == 0:
        return None

    max_strength = float(max(hit_strengths))
    mean_strength = float(np.mean(hit_strengths))
    if hit_count == 1 and max_strength < strong_single_hit_threshold:
        return None

    end_hold_sec, end_hold_debug = _estimate_end_hold_sec(
        activity_levels=activity_levels,
        window_sec=window_sec,
        hit_times=hit_times,
        hit_strengths=hit_strengths,
        last_hit_sec=hit_times[-1],
        hit_count=hit_count,
        mean_strength=mean_strength,
        sensitivity=sensitivity,
    )

    start_sec = max(0.0, hit_times[0] - pre_roll_sec)
    end_sec = min(duration_sec, hit_times[-1] + end_hold_sec)
    if end_sec <= start_sec:
        return None

    duration = end_sec - start_sec
    mean_gap = float(np.mean(np.diff(hit_times))) if hit_count >= 2 else 0.0
    activity_mean = _segment_activity_mean(activity_levels, window_sec, start_sec, end_sec)
    high_activity_ratio, very_low_activity_ratio = _activity_profile_stats(
        activity_levels,
        window_sec,
        start_sec,
        end_sec,
    )

    if hit_count == 1:
        if max_strength < strong_single_hit_threshold + 0.07 or activity_mean < 0.24:
            return None
    elif hit_count == 2:
        if mean_gap > 1.85:
            return None
        if activity_mean < 0.18 and max_strength < 0.78:
            return None
        if duration <= 2.4 and max_strength >= 0.84 and activity_mean < 0.34 and high_activity_ratio < 0.32 and very_low_activity_ratio >= 0.40:
            return None
        if duration <= 3.1 and max_strength < 0.75 and high_activity_ratio < 0.18:
            return None
    elif hit_count == 3:
        if activity_mean < 0.15 and max_strength < 0.72:
            return None
        if high_activity_ratio < 0.20 and very_low_activity_ratio > 0.60:
            return None
    elif hit_count <= 4:
        if max_strength < 0.76 and activity_mean < 0.31 and high_activity_ratio < 0.24 and very_low_activity_ratio >= 0.40:
            return None

    if hit_count <= 4 and high_activity_ratio < 0.18 and very_low_activity_ratio > 0.62:
        return None

    if 'serve_retry_merged' in cluster.get('merge_reasons', []) and high_activity_ratio < 0.18 and very_low_activity_ratio > 0.58:
        return None

    hit_density = hit_count / max(duration, 0.001)

    if hit_count >= 8 and high_activity_ratio < 0.30 and very_low_activity_ratio > 0.45 and activity_mean < 0.34:
        return None

    if hit_count >= 6 and duration >= 6.0 and hit_density < 0.85 and high_activity_ratio < 0.24 and very_low_activity_ratio > 0.58:
        return None

    confidence = clamp01(
        0.30
        + min(hit_count, 6) * 0.07
        + mean_strength * 0.28
        + max_strength * 0.15
        + activity_mean * 0.10
        + (0.08 if 'serve_retry_merged' in cluster.get('merge_reasons', []) else 0.0)
    )

    reasons: list[str] = ['audio_hit_cluster', 'dead_time_removed']
    if terminal_bounce_debug['terminal_bounce_trimmed'] > 0.0:
        reasons.append('terminal_bounce_trimmed')
    if hit_count >= 3:
        reasons.append('multi_hit_rally')
    elif hit_count == 2:
        reasons.append('two_hit_exchange')
    else:
        reasons.append('single_strong_hit_candidate')
    if 'serve_retry_merged' in cluster.get('merge_reasons', []):
        reasons.append('serve_retry_merged')

    return AnalysisSegment(
        segment_id=f'segment-{segment_number:03d}',
        start_sec=round(start_sec, 3),
        end_sec=round(end_sec, 3),
        duration_sec=round(duration, 3),
        confidence=round(float(confidence), 4),
        score=round(max_strength, 4),
        reasons=reasons,
        metrics={
            'hit_count': float(hit_count),
            'hit_strength_mean': round(mean_strength, 4),
            'hit_strength_max': round(max_strength, 4),
            'hit_gap_mean_sec': round(mean_gap, 4),
            'hit_density': round(hit_density, 4),
            'activity_mean': round(activity_mean, 4),
            'high_activity_ratio': round(high_activity_ratio, 4),
            'very_low_activity_ratio': round(very_low_activity_ratio, 4),
            'serve_retry_merged': 1.0 if 'serve_retry_merged' in cluster.get('merge_reasons', []) else 0.0,
            'post_roll_sec': end_hold_debug['post_roll_sec'],
            'recent_gap_median_sec': end_hold_debug['recent_gap_median_sec'],
            'expected_return_window_sec': end_hold_debug['expected_return_window_sec'],
            'tail_live_activity_mean': end_hold_debug['tail_live_activity_mean'],
            'tail_live_activity_peak': end_hold_debug['tail_live_activity_peak'],
            'tail_live_hold_applied': end_hold_debug['tail_live_hold_applied'],
            'tail_last_live_offset_sec': end_hold_debug['tail_last_live_offset_sec'],
            'tail_dead_run_found': end_hold_debug['tail_dead_run_found'],
            'terminal_bounce_trimmed': terminal_bounce_debug['terminal_bounce_trimmed'],
            'terminal_bounce_trimmed_hits': terminal_bounce_debug['terminal_bounce_trimmed_hits'],
            'terminal_bounce_gap_before_sec': terminal_bounce_debug['terminal_bounce_gap_before_sec'],
            'end_hold_sec': round(end_hold_sec, 4),
        },
    )


def _merge_neighboring_segments(
    segments: list[AnalysisSegment],
    activity_levels: np.ndarray,
    window_sec: float,
    join_gap_sec: float,
    soft_join_gap_sec: float,
    max_merged_duration_sec: float,
) -> list[AnalysisSegment]:
    if not segments:
        return []

    merged: list[AnalysisSegment] = [segments[0]]

    for segment in segments[1:]:
        previous = merged[-1]
        gap_sec = segment.start_sec - previous.end_sec
        merged_duration_sec = max(previous.end_sec, segment.end_sec) - min(previous.start_sec, segment.start_sec)

        gap_start_index = min(len(activity_levels), max(0, int(previous.end_sec / window_sec))) if window_sec > 0 else 0
        gap_end_index = min(len(activity_levels), max(gap_start_index + 1, int(np.ceil(segment.start_sec / window_sec)))) if window_sec > 0 else 0
        gap_activity_mean = (
            float(np.mean(activity_levels[gap_start_index:gap_end_index]))
            if gap_end_index > gap_start_index and activity_levels.size > 0
            else 0.0
        )

        previous_hit_count = previous.metrics.get('hit_count', 0.0)
        current_hit_count = segment.metrics.get('hit_count', 0.0)
        previous_activity = previous.metrics.get('activity_mean', 0.0)
        current_activity = segment.metrics.get('activity_mean', 0.0)
        has_short_side = previous.duration_sec <= 6.0 or segment.duration_sec <= 6.0
        has_small_hit_side = previous_hit_count <= 4.0 or current_hit_count <= 4.0
        both_rallyish = previous_hit_count >= 3.0 and current_hit_count >= 3.0
        both_weak_fragments = (
            previous_hit_count <= 2.0
            and current_hit_count <= 2.0
            and previous_activity < 0.18
            and current_activity < 0.18
        )

        merged_high_activity_ratio, merged_very_low_activity_ratio = _activity_profile_stats(
            activity_levels,
            window_sec,
            min(previous.start_sec, segment.start_sec),
            max(previous.end_sec, segment.end_sec),
        )

        should_merge = False
        if gap_sec <= join_gap_sec:
            should_merge = gap_activity_mean >= 0.05 and not both_weak_fragments
        elif (
            gap_sec <= soft_join_gap_sec
            and (has_short_side or has_small_hit_side or both_rallyish)
            and gap_activity_mean >= 0.18
            and not both_weak_fragments
        ):
            should_merge = True

        if merged_high_activity_ratio < 0.24 and merged_very_low_activity_ratio > 0.50 and merged_duration_sec > 6.5:
            should_merge = False

        if gap_activity_mean < 0.14 and merged_high_activity_ratio < 0.30 and merged_very_low_activity_ratio > 0.48 and merged_duration_sec > 7.5:
            should_merge = False

        if not should_merge or merged_duration_sec > max_merged_duration_sec:
            merged.append(segment)
            continue

        previous.start_sec = min(previous.start_sec, segment.start_sec)
        previous.end_sec = max(previous.end_sec, segment.end_sec)
        previous.duration_sec = round(previous.end_sec - previous.start_sec, 3)
        previous.confidence = round(float(max(previous.confidence, segment.confidence)), 4)
        previous.score = round(float(max(previous.score, segment.score)), 4)
        previous.reasons = sorted(set(previous.reasons + segment.reasons + ['adjacent_fragment_merged']))
        previous.metrics['hit_count'] = round(previous.metrics.get('hit_count', 0.0) + segment.metrics.get('hit_count', 0.0), 4)
        previous.metrics['hit_strength_max'] = round(max(previous.metrics.get('hit_strength_max', 0.0), segment.metrics.get('hit_strength_max', 0.0)), 4)
        previous.metrics['hit_strength_mean'] = round(
            (previous.metrics.get('hit_strength_mean', 0.0) + segment.metrics.get('hit_strength_mean', 0.0)) / 2.0,
            4,
        )
        previous.metrics['activity_mean'] = round(
            (previous.metrics.get('activity_mean', 0.0) + segment.metrics.get('activity_mean', 0.0) + gap_activity_mean) / 3.0,
            4,
        )
        previous.metrics['gap_activity_mean'] = round(gap_activity_mean, 4)

    for index, segment in enumerate(merged, start=1):
        segment.segment_id = f'segment-{index:03d}'

    return merged


def build_segments(
    hit_times_sec: list[float],
    hit_strengths: list[float],
    activity_levels: list[float],
    window_sec: float,
    duration_sec: float,
    sensitivity: float,
) -> tuple[list[AnalysisSegment], dict[str, Any]]:
    if not hit_times_sec:
        return [], {
            'rule_engine': 'audio_hit_point_segmentation_v1',
            'window_count': len(activity_levels),
            'hit_count': 0,
            'cluster_count': 0,
            'hit_times_sec': [],
            'hit_strengths': [],
            'activity_series': [round(float(value), 4) for value in activity_levels],
            'dropped_singletons': 0,
        }

    activity = np.asarray(activity_levels, dtype=np.float32)
    max_intra_gap_sec = 2.15 + (0.5 - sensitivity) * 0.45
    strong_single_hit_threshold = 0.56 - (sensitivity - 0.5) * 0.08
    serve_retry_gap_min_sec = 2.0
    serve_retry_gap_max_sec = 5.2
    pre_roll_sec = 0.95 + sensitivity * 0.35
    join_gap_sec = 0.65
    soft_join_gap_sec = 1.05
    max_cluster_span_sec = 28.0 + sensitivity * 4.0

    clusters = _group_hits(hit_times_sec, hit_strengths, max_intra_gap_sec=max_intra_gap_sec)
    merged_clusters, serve_retry_merge_count = _merge_serve_retry_clusters(
        clusters,
        strong_single_hit_threshold=strong_single_hit_threshold,
        serve_retry_gap_min_sec=serve_retry_gap_min_sec,
        serve_retry_gap_max_sec=serve_retry_gap_max_sec,
    )

    merged_clusters = _split_oversized_clusters(merged_clusters, max_cluster_span_sec=max_cluster_span_sec)

    dropped_singletons = 0
    raw_segments: list[AnalysisSegment] = []
    for cluster in merged_clusters:
        segment = _build_segment_from_cluster(
            cluster,
            segment_number=len(raw_segments) + 1,
            duration_sec=duration_sec,
            activity_levels=activity,
            window_sec=window_sec,
            pre_roll_sec=pre_roll_sec,
            sensitivity=sensitivity,
            strong_single_hit_threshold=strong_single_hit_threshold,
        )
        if segment is None:
            dropped_singletons += 1
            continue
        raw_segments.append(segment)

    segments = _merge_neighboring_segments(
        raw_segments,
        activity_levels=activity,
        window_sec=window_sec,
        join_gap_sec=join_gap_sec,
        soft_join_gap_sec=soft_join_gap_sec,
        max_merged_duration_sec=max_cluster_span_sec,
    )
    segments, single_hit_filter_debug = _drop_short_weak_single_hit_segments(segments)
    total_duration = sum(segment.duration_sec for segment in segments)
    average_confidence = float(np.mean([segment.confidence for segment in segments])) if segments else 0.0

    debug = {
        'rule_engine': 'audio_hit_point_segmentation_v1',
        'window_count': len(activity_levels),
        'hit_count': len(hit_times_sec),
        'cluster_count': len(merged_clusters),
        'serve_retry_merge_count': serve_retry_merge_count,
        'dropped_singletons': dropped_singletons,
        'activity_series': [round(float(value), 4) for value in activity.tolist()],
        'hit_times_sec': [round(float(value), 3) for value in hit_times_sec],
        'hit_strengths': [round(float(value), 4) for value in hit_strengths],
        'max_intra_gap_sec': round(max_intra_gap_sec, 3),
        'strong_single_hit_threshold': round(strong_single_hit_threshold, 4),
        'pre_roll_sec': round(pre_roll_sec, 3),
        'max_cluster_span_sec': round(max_cluster_span_sec, 3),
        'serve_retry_gap_max_sec': round(serve_retry_gap_max_sec, 3),
        'join_gap_sec': round(join_gap_sec, 3),
        'soft_join_gap_sec': round(soft_join_gap_sec, 3),
        'total_candidate_duration_sec': round(total_duration, 3),
        'average_confidence': round(average_confidence, 4),
        'short_single_hit_filter': single_hit_filter_debug,
    }

    return segments, debug
