from __future__ import annotations

import argparse
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parents[1]
VENDOR_DIR = ROOT_DIR / 'vendor'
ANALYZER_DIR = ROOT_DIR / 'analyzer'

for extra_path in (ROOT_DIR, VENDOR_DIR, ANALYZER_DIR):
    value = str(extra_path)
    if value not in sys.path:
        sys.path.insert(0, value)

from analyzer.audio_features import extract_audio_features
from analyzer.io_utils import emit_progress, read_project_record, write_analysis_result
from analyzer.segment_logic import build_segments
from analyzer.types import AnalysisResult, AnalysisSummary


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description='Analyze tennis audio and remove dead time between points.')
    parser.add_argument('--project-dir', required=True, help='Absolute path to the project directory')
    parser.add_argument('--proxy-path', required=True, help='Absolute path to proxy.mp4')
    parser.add_argument('--audio-path', required=True, help='Absolute path to audio.wav')
    parser.add_argument('--sensitivity', type=float, default=0.55, help='Detection sensitivity between 0.1 and 1.0')
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    project_dir = Path(args.project_dir).resolve()
    proxy_path = Path(args.proxy_path).resolve()
    audio_path = Path(args.audio_path).resolve()
    sensitivity = max(0.1, min(1.0, float(args.sensitivity)))

    if not project_dir.exists():
        raise FileNotFoundError(f'项目目录不存在: {project_dir}')
    if not proxy_path.exists():
        raise FileNotFoundError(f'proxy 文件不存在: {proxy_path}')
    if not audio_path.exists():
        raise FileNotFoundError(f'audio 文件不存在: {audio_path}')

    project = read_project_record(project_dir)
    output_path = project_dir / project.get('analysis_result_path', 'analysis_result.json')

    emit_progress('prepare', 5, '已读取项目配置，准备启动纯音频 dead-time removal analyzer')
    audio = extract_audio_features(audio_path, window_sec=0.25)
    emit_progress('audio_features', 55, f"已检测到 {audio['hit_count']} 个疑似击球声事件")

    segments, debug = build_segments(
        hit_times_sec=audio['hit_times_sec'],
        hit_strengths=audio['hit_strengths'],
        activity_levels=audio['levels'],
        window_sec=float(audio['window_sec']),
        duration_sec=float(audio['duration_sec']),
        sensitivity=sensitivity,
    )
    emit_progress('segment_logic', 85, '已完成 point/rally 分段与死球时间删除规则合并')

    total_duration = sum(segment.duration_sec for segment in segments)
    average_confidence = sum(segment.confidence for segment in segments) / len(segments) if segments else 0.0
    summary = AnalysisSummary(
        duration_sec=round(float(audio['duration_sec']), 3),
        window_sec=float(audio['window_sec']),
        segment_count=len(segments),
        total_candidate_duration_sec=round(total_duration, 3),
        average_confidence=round(average_confidence, 4),
        sensitivity=round(sensitivity, 3),
    )

    result = AnalysisResult(
        project_id=project.get('project_id', project_dir.name),
        created_at=datetime.now(timezone.utc).isoformat(),
        sensitivity=round(sensitivity, 3),
        source_video_path=project.get('source_video_path', ''),
        proxy_path=str(proxy_path),
        audio_path=str(audio_path),
        summary=summary,
        segments=segments,
        debug={
            **debug,
            'audio_sample_rate': int(audio['sample_rate']),
            'audio_hit_count': int(audio['hit_count']),
            'rule_engine': 'audio_hit_point_segmentation_v1',
            'notes': 'Pure audio experiment: keep whole points around hit clusters, remove dead time between points.',
        },
    )

    write_analysis_result(output_path, result)
    emit_progress('done', 100, f'分析完成，已输出 {len(segments)} 个候选片段', result_path=str(output_path), segment_count=len(segments))
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f'ERROR: {exc}', file=sys.stderr)
        raise
