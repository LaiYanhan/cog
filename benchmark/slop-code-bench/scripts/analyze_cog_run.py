"""Analyze a slop-code-bench run with optional cog integration.

Parses result.json, checkpoint_results.jsonl, run_info.yaml, infer.log,
agent/stdout.jsonl, and cog_state/ (usage.jsonl + cog.db) to produce a
flat summary of task results, efficiency, cognitive-layer usage, and code
quality. See benchmark/slop-code-bench/COG_INTEGRATION_DESIGN.md §6.

cd benchmark/slop-code-bench

# 分析单个 run
uv run python scripts/analyze_cog_run.py outputs/anthropic/claude_code-2.0.51_just-solve_20260701T1034

# 输出 JSON 摘要
uv run python scripts/analyze_cog_run.py outputs/... -o summary.json

# baseline / cog 对照
uv run python scripts/analyze_cog_run.py outputs/.../baseline --compare outputs/.../cog -o compare.json
"""

from __future__ import annotations

import json
import sqlite3
from collections import Counter
from dataclasses import asdict
from dataclasses import dataclass
from dataclasses import field
from pathlib import Path
from typing import Any

import typer
import yaml
from rich.console import Console
from rich.table import Table

app = typer.Typer()
console = Console()
err = Console(stderr=True)

READ_COMMANDS = {
    "query",
    "stats",
    "index",
    "usage",
    "export",
    "impact",
    "trace",
    "verify",
    "next",
    "depend",
}
WRITE_COMMANDS = {
    "assert",
    "sync",
    "retract",
    "experiment",
    "backup",
    "migrate",
}

LARGE_OUTPUT_LINE_THRESHOLD = 30
LARGE_OUTPUT_BYTE_THRESHOLD = 8192


@dataclass
class CheckpointMetrics:
    checkpoint: str
    strict_pass_rate: float | None = None
    core_pass_rate: float | None = None
    isolated_pass_rate: float | None = None
    regression_pass_rate: float | None = None
    total_tests: int | None = None
    passed_tests: int | None = None
    duration: float | None = None
    cost: float | None = None
    steps: int | None = None
    input_tokens: int | None = None
    output_tokens: int | None = None
    loc: int | None = None
    sloc: int | None = None
    cog_calls: int = 0
    cog_ok: int = 0
    cog_err: int = 0
    cog_duration_ms: int = 0
    cog_read_cmds: int = 0
    cog_write_cmds: int = 0
    cog_assertions: int = 0
    cog_retracts: int = 0
    cog_entities_queried: int = 0
    cog_large_outputs: int = 0


@dataclass
class RunMetrics:
    run_dir: str
    mode: str
    model: str
    agent: str
    prompt: str
    num_checkpoints: int = 0
    completed_checkpoints: int = 0
    total_cost: float | None = None
    total_steps: int | None = None
    total_input_tokens: int | None = None
    total_output_tokens: int | None = None
    duration_seconds: float | None = None
    checkpoints_core_solved: int | None = None
    checkpoints_solved: int | None = None
    overall_pass_rate: float | None = None
    erosion: Any = None
    checkpoints: list[CheckpointMetrics] = field(default_factory=list)
    cog_total_calls: int = 0
    cog_total_ok: int = 0
    cog_total_err: int = 0
    cog_total_duration_ms: int = 0
    cog_command_dist: dict[str, int] = field(default_factory=dict)
    cog_entities: int = 0
    cog_assertions: int = 0
    cog_relations: int = 0
    cog_assertion_coverage: float = 0.0
    cog_retract_ratio: float = 0.0
    cog_total_tool_calls: int = 0
    cog_adoption_rate: float = 0.0
    cog_read_ratio: float = 0.0
    cog_large_outputs: int = 0


def _load_json(path: Path) -> Any:
    if not path.exists():
        return None
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    out = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                out.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return out


def _load_yaml(path: Path) -> Any:
    if not path.exists():
        return None
    with open(path, "r", encoding="utf-8") as f:
        return yaml.safe_load(f)


def _detect_mode(run_dir: Path, run_info: dict[str, Any] | None) -> str:
    name = run_dir.name
    if "_cog" in name:
        return "cog"
    if "_nocog" in name:
        return "baseline"
    if run_info and "cog" in str(run_info.get("template", "")).lower():
        return "cog"
    return "baseline"


def _extract_cog_commands_from_stdout(
    stdout_path: Path,
) -> list[dict[str, Any]]:
    """Parse agent/stdout.jsonl for Bash tool_use commands containing 'cog '."""
    cmds = []
    for evt in _load_jsonl(stdout_path):
        msg = evt.get("message") or {}
        for block in msg.get("content", []):
            if block.get("type") != "tool_use":
                continue
            if block.get("name") != "Bash":
                continue
            command = block.get("input", {}).get("command", "")
            if "cog " in command:
                cmds.append(
                    {
                        "ts": evt.get("timestamp"),
                        "cmd": command,
                        "source": "stdout",
                    }
                )
    return cmds


def _extract_cog_commands_from_infer(infer_path: Path) -> list[dict[str, Any]]:
    """Parse infer.log for assistant tool_use payloads containing 'cog '."""
    cmds = []
    for line in _load_jsonl(infer_path):
        if line.get("type") != "assistant":
            continue
        content = line.get("content", "")
        if not isinstance(content, str):
            continue
        try:
            payload = json.loads(content)
        except json.JSONDecodeError:
            continue
        if not isinstance(payload, list):
            payload = [payload]
        for block in payload:
            if block.get("type") != "tool_use":
                continue
            if block.get("name") != "Bash":
                continue
            command = block.get("input", {}).get("command", "")
            if "cog " in command:
                cmds.append(
                    {
                        "ts": line.get("timestamp"),
                        "cmd": command,
                        "source": "infer",
                    }
                )
    return cmds


def _extract_cog_large_outputs_from_stdout(stdout_path: Path) -> int:
    """Count cog Bash tool_results whose output exceeds line/byte thresholds."""
    if not stdout_path.exists():
        return 0
    cog_tool_ids: set[Any] = set()
    large = 0
    for evt in _load_jsonl(stdout_path):
        msg = evt.get("message") or {}
        for block in msg.get("content", []):
            btype = block.get("type")
            if btype == "tool_use" and block.get("name") == "Bash":
                command = block.get("input", {}).get("command", "")
                if "cog " in command:
                    cog_tool_ids.add(block.get("id"))
            elif btype == "tool_result":
                tid = block.get("tool_use_id") or block.get("id")
                if tid not in cog_tool_ids:
                    continue
                content = block.get("content", "")
                if isinstance(content, list):
                    text = "\n".join(str(c) for c in content)
                else:
                    text = str(content)
                lines = text.count("\n") + 1
                size = len(text.encode("utf-8"))
                if lines > LARGE_OUTPUT_LINE_THRESHOLD or size > LARGE_OUTPUT_BYTE_THRESHOLD:
                    large += 1
    return large


def _usage_read(path: Path) -> list[dict[str, Any]]:
    return _load_jsonl(path)


def _cog_db_metrics(db_path: Path) -> dict[str, Any]:
    if not db_path.exists():
        return {}
    metrics = {}
    try:
        conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
        cur = conn.cursor()
        metrics["entities"] = cur.execute(
            "SELECT COUNT(*) FROM entities"
        ).fetchone()[0]
        metrics["assertions"] = cur.execute(
            "SELECT COUNT(*) FROM assertions"
        ).fetchone()[0]
        metrics["relations"] = (
            cur.execute("SELECT COUNT(*) FROM entity_relations").fetchone()[0]
            + cur.execute(
                "SELECT COUNT(*) FROM assertion_relations"
            ).fetchone()[0]
        )
        with_assertions = cur.execute(
            "SELECT COUNT(DISTINCT entity_id) FROM assertions"
        ).fetchone()[0]
        metrics["assertion_coverage"] = (
            with_assertions / metrics["entities"]
            if metrics["entities"]
            else 0.0
        )
        metrics["retracts"] = cur.execute(
            "SELECT COUNT(*) FROM changelog WHERE action = 'retract'"
        ).fetchone()[0]
        metrics["fragilities"] = cur.execute(
            "SELECT COUNT(*) FROM assertions WHERE kind = 'fragility'"
        ).fetchone()[0]
        conn.close()
    except sqlite3.Error as e:
        err.print(f"sqlite error reading {db_path}: {e}")
    return metrics


def _phase_from_usage(events: list[dict[str, Any]]) -> list[str]:
    phases = []
    for e in events:
        pfrom = e.get("phase_from")
        pto = e.get("phase_to")
        if pfrom and pto and pfrom != pto:
            phases.append(f"{pfrom}→{pto}")
    return phases


def _command_verb(cmd: str) -> str:
    parts = cmd.strip().split()
    if len(parts) < 2:
        return "unknown"
    return parts[1]


def _is_read(command: str) -> bool:
    verb = _command_verb(command)
    return verb in READ_COMMANDS


def _analyze_run(run_dir: Path) -> RunMetrics:
    result = _load_json(run_dir / "result.json") or {}
    checkpoint_results = _load_jsonl(run_dir / "checkpoint_results.jsonl")

    # Find problem subdirectories.
    problems = [p for p in run_dir.iterdir() if p.is_dir() and p.name != ".git"]
    if not problems:
        err.print(f"[yellow]No problem directories found in {run_dir}[/yellow]")
    problem_dir = problems[0] if problems else None
    run_info = (
        _load_yaml(problem_dir / "run_info.yaml") if problem_dir else None
    )

    mode = _detect_mode(run_dir, run_info)
    metrics = RunMetrics(
        run_dir=str(run_dir),
        mode=mode,
        model=result.get("model", "unknown"),
        agent=f"{result.get('agent_type', 'unknown')}-{result.get('agent_version', 'unknown')}",
        prompt=result.get("prompt", "unknown"),
    )

    # Run-level aggregates.
    metrics.num_checkpoints = result.get(
        "num_checkpoints", len(checkpoint_results)
    )
    metrics.total_cost = result.get("costs", {}).get("total")
    metrics.total_steps = result.get("steps", {}).get("total")
    tok = result.get("tokens", {})
    metrics.total_input_tokens = tok.get("input")
    metrics.total_output_tokens = tok.get("output")
    metrics.checkpoints_core_solved = result.get("checkpoints_core_solved")
    metrics.checkpoints_solved = result.get("checkpoints_solved")
    metrics.overall_pass_rate = result.get("pass_rates", {}).get(
        "problem", {}
    ).get("total") or result.get("pass_rates", {}).get("checkpoint", {}).get(
        "total"
    )
    metrics.erosion = result.get("erosion")

    if run_info:
        metrics.duration_seconds = run_info.get("summary", {}).get(
            "duration_seconds"
        )
        if metrics.total_cost is None:
            metrics.total_cost = run_info.get("summary", {}).get("total_cost")
        if metrics.total_steps is None:
            metrics.total_steps = run_info.get("summary", {}).get("total_steps")

    # Per-checkpoint metrics.
    for cr in checkpoint_results:
        cp_name = cr.get("checkpoint", "unknown")
        cp = CheckpointMetrics(checkpoint=cp_name)
        cp.strict_pass_rate = cr.get("strict_pass_rate")
        cp.core_pass_rate = cr.get("core_pass_rate")
        cp.isolated_pass_rate = cr.get("isolated_pass_rate")
        cp.regression_pass_rate = (
            cr.get("regression_passed", 0) / cr.get("regression_total", 1)
            if cr.get("regression_total")
            else None
        )
        cp.total_tests = cr.get("total_tests")
        cp.passed_tests = cr.get("passed_tests")
        cp.duration = cr.get("duration")
        cp.cost = cr.get("cost")
        cp.steps = cr.get("steps")
        cp.input_tokens = cr.get("input")
        cp.output_tokens = cr.get("output")
        cp.loc = cr.get("loc")
        cp.sloc = cr.get("sloc")
        if cr.get("state") == "ran":
            metrics.completed_checkpoints += 1

        cp_dir = problem_dir / cp_name if problem_dir else Path()
        if cp_dir.exists():
            # Cross-validate cog commands from both trajectory sources.
            stdout_path = cp_dir / "agent" / "stdout.jsonl"
            stdout_cmds = _extract_cog_commands_from_stdout(stdout_path)
            cp.cog_large_outputs = _extract_cog_large_outputs_from_stdout(stdout_path)
            infer_cmds = _extract_cog_commands_from_infer(
                cp_dir / "infer.log"
                if (cp_dir / "infer.log").exists()
                else run_dir / problem_dir.name / "infer.log"
            )
            metrics.cog_total_tool_calls += len(stdout_cmds) or len(infer_cmds)

            # Prefer usage.jsonl for precise cog metrics if available.
            usage_path = cp_dir / "cog_state" / "usage.jsonl"
            if usage_path.exists():
                events = _usage_read(usage_path)
                cp.cog_calls = len(events)
                cp.cog_ok = sum(1 for e in events if e.get("ok"))
                cp.cog_err = sum(1 for e in events if not e.get("ok"))
                cp.cog_duration_ms = sum(
                    e.get("duration_ms", 0) for e in events
                )
                cp.cog_read_cmds = sum(
                    1 for e in events if _is_read(e.get("command", ""))
                )
                cp.cog_write_cmds = cp.cog_calls - cp.cog_read_cmds
                cp.cog_assertions = sum(
                    1 for e in events if e.get("command") == "assert"
                )
                cp.cog_retracts = sum(
                    1 for e in events if e.get("command") == "retract"
                )
                cp.cog_entities_queried = len(
                    {
                        e.get("args", {}).get("entity")
                        for e in events
                        if e.get("command") == "query"
                        and e.get("args", {}).get("entity")
                    }
                )

            # If no usage.jsonl, fall back to infer.log command count.
            elif infer_cmds:
                cp.cog_calls = len(infer_cmds)

            # Latest cog.db in this checkpoint for coverage.
            db_path = cp_dir / "cog_state" / "cog.db"
            if db_path.exists():
                dbm = _cog_db_metrics(db_path)
                metrics.cog_entities = max(
                    metrics.cog_entities, dbm.get("entities", 0)
                )
                metrics.cog_assertions = max(
                    metrics.cog_assertions, dbm.get("assertions", 0)
                )
                metrics.cog_relations = max(
                    metrics.cog_relations, dbm.get("relations", 0)
                )
                metrics.cog_assertion_coverage = max(
                    metrics.cog_assertion_coverage,
                    dbm.get("assertion_coverage", 0.0),
                )

        metrics.cog_total_calls += cp.cog_calls
        metrics.cog_total_ok += cp.cog_ok
        metrics.cog_total_err += cp.cog_err
        metrics.cog_total_duration_ms += cp.cog_duration_ms
        metrics.cog_large_outputs += cp.cog_large_outputs
        metrics.checkpoints.append(cp)

    # Aggregate command distribution from usage.jsonl across all checkpoints.
    command_dist: Counter[str] = Counter()
    total_reads = total_writes = 0
    if problem_dir:
        for cp_dir in problem_dir.glob("checkpoint_*/cog_state"):
            for e in _load_jsonl(cp_dir / "usage.jsonl"):
                verb = e.get("command", "unknown")
                command_dist[verb] += 1
                if verb in READ_COMMANDS:
                    total_reads += 1
                elif verb in WRITE_COMMANDS:
                    total_writes += 1
    metrics.cog_command_dist = dict(command_dist)
    metrics.cog_adoption_rate = (
        metrics.cog_total_calls / metrics.cog_total_tool_calls
        if metrics.cog_total_tool_calls
        else 0.0
    )
    metrics.cog_read_ratio = (
        total_reads / (total_reads + total_writes)
        if (total_reads + total_writes)
        else 0.0
    )
    if metrics.cog_assertions > 0:
        metrics.cog_retract_ratio = (
            sum(cp.cog_retracts for cp in metrics.checkpoints)
            / metrics.cog_total_calls
            if metrics.cog_total_calls
            else 0.0
        )
    return metrics


def _render_run_table(metrics: RunMetrics) -> None:
    t = Table(title=f"Run summary: {Path(metrics.run_dir).name}")
    t.add_column("Metric", justify="left")
    t.add_column("Value", justify="right")
    t.add_row("Mode", metrics.mode)
    t.add_row("Model", metrics.model)
    t.add_row("Agent", metrics.agent)
    t.add_row("Prompt", metrics.prompt)
    t.add_row(
        "Checkpoints",
        f"{metrics.completed_checkpoints}/{metrics.num_checkpoints}",
    )
    t.add_row(
        "Duration (s)",
        f"{metrics.duration_seconds:.1f}" if metrics.duration_seconds else "-",
    )
    t.add_row(
        "Total cost",
        f"${metrics.total_cost:.4f}" if metrics.total_cost else "-",
    )
    t.add_row(
        "Total steps", str(metrics.total_steps) if metrics.total_steps else "-"
    )
    t.add_row(
        "Overall pass rate",
        f"{metrics.overall_pass_rate:.3f}"
        if metrics.overall_pass_rate
        else "-",
    )
    t.add_row(
        "Core solved",
        f"{metrics.checkpoints_core_solved}/{metrics.num_checkpoints}",
    )
    if metrics.mode == "cog":
        t.add_row("Cog calls", str(metrics.cog_total_calls))
        t.add_row(
            "Cog ok/err", f"{metrics.cog_total_ok}/{metrics.cog_total_err}"
        )
        t.add_row("Cog adoption", f"{metrics.cog_adoption_rate:.2%}")
        t.add_row("Cog read ratio", f"{metrics.cog_read_ratio:.2%}")
        t.add_row("Cog coverage", f"{metrics.cog_assertion_coverage:.2%}")
        t.add_row("Cog large outputs", str(metrics.cog_large_outputs))
    console.print(t)

    cp_table = Table(title="Per-checkpoint metrics")
    cp_table.add_column("Checkpoint")
    cp_table.add_column("Strict", justify="right")
    cp_table.add_column("Core", justify="right")
    cp_table.add_column("Regression", justify="right")
    cp_table.add_column("Cost", justify="right")
    cp_table.add_column("Steps", justify="right")
    cp_table.add_column("Cog calls", justify="right")
    cp_table.add_column("Cog ok", justify="right")
    cp_table.add_column("Large outputs", justify="right")
    for cp in metrics.checkpoints:
        cp_table.add_row(
            cp.checkpoint,
            f"{cp.strict_pass_rate:.3f}"
            if cp.strict_pass_rate is not None
            else "-",
            f"{cp.core_pass_rate:.3f}"
            if cp.core_pass_rate is not None
            else "-",
            f"{cp.regression_pass_rate:.3f}"
            if cp.regression_pass_rate is not None
            else "-",
            f"${cp.cost:.4f}" if cp.cost else "-",
            str(cp.steps) if cp.steps else "-",
            str(cp.cog_calls) if cp.cog_calls else "-",
            f"{cp.cog_ok}/{cp.cog_err}" if cp.cog_calls else "-",
            str(cp.cog_large_outputs) if cp.cog_large_outputs else "-",
        )
    console.print(cp_table)

    if metrics.cog_command_dist:
        cmd_table = Table(title="Cog command distribution")
        cmd_table.add_column("Command")
        cmd_table.add_column("Count", justify="right")
        for cmd, cnt in sorted(
            metrics.cog_command_dist.items(), key=lambda x: -x[1]
        ):
            cmd_table.add_row(cmd, str(cnt))
        console.print(cmd_table)


def _render_comparison_table(runs: list[RunMetrics]) -> None:
    t = Table(title="Run comparison")
    t.add_column("Run")
    t.add_column("Mode")
    t.add_column("Completed")
    t.add_column("Cost")
    t.add_column("Steps")
    t.add_column("Pass rate")
    t.add_column("Core solved")
    t.add_column("Cog calls")
    t.add_column("Cog adoption")
    t.add_column("Cog coverage")
    t.add_column("Large outputs")
    for m in runs:
        t.add_row(
            Path(m.run_dir).name,
            m.mode,
            f"{m.completed_checkpoints}/{m.num_checkpoints}",
            f"${m.total_cost:.4f}" if m.total_cost else "-",
            str(m.total_steps) if m.total_steps else "-",
            f"{m.overall_pass_rate:.3f}" if m.overall_pass_rate else "-",
            f"{m.checkpoints_core_solved}/{m.num_checkpoints}"
            if m.checkpoints_core_solved is not None
            else "-",
            str(m.cog_total_calls) if m.cog_total_calls else "-",
            f"{m.cog_adoption_rate:.2%}" if m.cog_adoption_rate else "-",
            f"{m.cog_assertion_coverage:.2%}"
            if m.cog_assertion_coverage
            else "-",
            str(m.cog_large_outputs) if m.cog_large_outputs else "-",
        )
    console.print(t)


def _to_dict(metrics: RunMetrics) -> dict[str, Any]:
    d = asdict(metrics)
    d["checkpoints"] = [asdict(cp) for cp in metrics.checkpoints]
    return d


@app.command()
def analyze(
    run_dir: Path = typer.Argument(
        ..., help="Path to a single run output directory"
    ),
    out: Path | None = typer.Option(
        None, "--out", "-o", help="Write JSON summary to this file"
    ),
    compare: list[Path] = typer.Option(
        [], "--compare", "-c", help="Additional run directories to compare"
    ),
) -> None:
    """Analyze one or more benchmark runs and display summary tables."""
    runs = [_analyze_run(run_dir)]
    for rd in compare:
        runs.append(_analyze_run(rd))

    for metrics in runs:
        _render_run_table(metrics)
    if len(runs) > 1:
        _render_comparison_table(runs)

    if out:
        summary = [_to_dict(m) for m in runs]
        with open(out, "w", encoding="utf-8") as f:
            json.dump(summary, f, indent=2, default=str)
        console.print(f"[green]Wrote summary to {out}[/green]")


if __name__ == "__main__":
    app()
