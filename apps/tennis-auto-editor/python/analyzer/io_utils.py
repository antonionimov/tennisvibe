from __future__ import annotations

import json
from dataclasses import asdict
from pathlib import Path
from typing import Any

from analyzer.types import AnalysisResult


def emit_progress(stage: str, percent: int, message: str, **extra: Any) -> None:
    payload = {
        'type': 'progress',
        'stage': stage,
        'percent': percent,
        'message': message,
        **extra,
    }
    print(json.dumps(payload, ensure_ascii=False), flush=True)


def read_project_record(project_dir: Path) -> dict[str, Any]:
    return json.loads(project_dir.joinpath('project.json').read_text(encoding='utf-8'))


def write_analysis_result(output_path: Path, result: AnalysisResult) -> None:
    output_path.write_text(json.dumps(asdict(result), ensure_ascii=False, indent=2), encoding='utf-8')
