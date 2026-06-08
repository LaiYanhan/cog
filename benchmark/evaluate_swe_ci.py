#!/usr/bin/env python3
"""
SWE-CI Experiment Evaluation Script

Usage:
    # Single experiment analysis
    python evaluate_swe_ci.py <experiment_dir>

    # A/B comparison (baseline vs cog)
    python evaluate_swe_ci.py --compare <baseline_dir> <cog_dir>

Examples:
    python evaluate_swe_ci.py SWE-CI/experiments/cog-pilot-v1
    python evaluate_swe_ci.py --compare SWE-CI/experiments/baseline-pilot SWE-CI/experiments/cog-pilot
"""

import json
import sqlite3
from rich.console import Console
from rich.table import Table
import sys
import argparse
from pathlib import Path
from collections import defaultdict


def read_jsonl(path: Path) -> list[dict]:
    if not path.exists():
        return []
    records = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records


def query_db(db_path: Path, sql: str, params=()) -> list[tuple]:
    if not db_path.exists():
        return []
    try:
        conn = sqlite3.connect(str(db_path))
        try:
            return conn.execute(sql, params).fetchall()
        finally:
            conn.close()
    except Exception:
        return []


def query_db_one(db_path: Path, sql: str, params=()):
    rows = query_db(db_path, sql, params)
    return rows[0] if rows else None


# ── Iteration analysis ──────────────────────────────────────────────

def analyze_iterations(task_dir: Path) -> dict:
    iterations = read_jsonl(task_dir / "iteration.jsonl")
    if not iterations:
        return {"error": "no iterations"}

    init = iterations[0]
    evo = iterations[1:]
    init_passed = init.get("pytest", {}).get("passed", 0)
    init_gap = init.get("gap", 0)
    total_tests = init.get("pytest", {}).get("total", init_passed + init_gap)

    epochs = []
    prev_passed = init_passed
    for i, rec in enumerate(evo, 1):
        passed = rec.get("pytest", {}).get("passed", 0)
        gap = rec.get("gap", -1)
        regression = passed < prev_passed
        arch = rec.get("architect", {})
        prog = rec.get("programmer", {})
        epochs.append({
            "epoch": i, "passed": passed, "gap": gap, "regression": regression,
            "arch_in": arch.get("input_tokens", 0), "arch_out": arch.get("output_tokens", 0),
            "prog_in": prog.get("input_tokens", 0), "prog_out": prog.get("output_tokens", 0),
            "arch_t": arch.get("execution_time", 0), "prog_t": prog.get("execution_time", 0),
        })
        prev_passed = passed

    regressions = [{"epoch": e["epoch"], "prev": epochs[i-1]["passed"] if i > 0 else init_passed, "cur": e["passed"]}
                   for i, e in enumerate(epochs) if e["regression"]]

    final_gap = evo[-1].get("gap", -1) if evo else init_gap
    total_in = sum(e["arch_in"] + e["prog_in"] for e in epochs)
    total_out = sum(e["arch_out"] + e["prog_out"] for e in epochs)

    return {
        "init_passed": init_passed, "init_gap": init_gap, "total_tests": total_tests,
        "n_epochs": len(evo), "resolved": final_gap == 0, "final_gap": final_gap,
        "regressions": regressions, "zero_reg": len(regressions) == 0,
        "total_in": total_in, "total_out": total_out,
        "gap_trace": [init_gap] + [e["gap"] for e in epochs],
        "epochs": epochs,
    }


# ── Cog model analysis ──────────────────────────────────────────────

def analyze_cog(task_dir: Path) -> dict:
    """Analyze cog model from the last archived snapshot."""
    snapshots = sorted(
        (d for d in task_dir.iterdir()
         if d.is_dir() and d.name.startswith("20") and (d / "code" / ".cog" / "cog.db").exists()),
        key=lambda d: d.name,
    )
    if not snapshots:
        return {"has_cog": False}

    # Use last snapshot for final state
    last_db = snapshots[-1] / "code" / ".cog" / "cog.db"

    entities = query_db_one(last_db, "SELECT count(*) FROM entities")
    entities = entities[0] if entities else 0

    assertions_raw = query_db(last_db,
        "SELECT kind, status, claim, entity_id FROM assertions ORDER BY rowid")
    assertions = [{"kind": r[0], "status": r[1], "claim": r[2], "entity_id": r[3]} for r in assertions_raw]

    active = [a for a in assertions if a["status"] == "active"]
    retracted = [a for a in assertions if a["status"] == "retracted"]
    kinds = defaultdict(int)
    for a in active:
        kinds[a["kind"]] += 1

    # Per-snapshot entity/assertion counts for growth tracking
    growth = []
    for snap in snapshots:
        db = snap / "code" / ".cog" / "cog.db"
        ent = query_db_one(db, "SELECT count(*) FROM entities")
        act = query_db_one(db, "SELECT count(*) FROM assertions WHERE status='active'")
        ret = query_db_one(db, "SELECT count(*) FROM assertions WHERE status='retracted'")
        growth.append({
            "snapshot": snap.name,
            "entities": ent[0] if ent else 0,
            "active": act[0] if act else 0,
            "retracted": ret[0] if ret else 0,
        })

    return {
        "has_cog": True, "entities": entities,
        "n_active": len(active), "n_retracted": len(retracted),
        "kinds": dict(kinds),
        "assertions": assertions,
        "growth": growth,
    }


# ── Trajectory analysis ─────────────────────────────────────────────

def analyze_trajectories(task_dir: Path) -> dict:
    """Analyze programmer trajectories for cog CLI usage and tool breakdown."""
    traj_base = task_dir / "trajectories"
    if not traj_base.exists():
        return {"has_traj": False}

    epochs = {}
    for epoch_dir in sorted(traj_base.iterdir()):
        if not epoch_dir.is_dir() or not epoch_dir.name.startswith("epoch_"):
            continue
        prog_db = epoch_dir / "programmer" / "opencode.db"
        if not prog_db.exists():
            continue

        epoch_num = int(epoch_dir.name.split("_")[1])

        # Tool usage counts from part table
        tools = query_db(prog_db,
            "SELECT json_extract(data, '$.tool'), count(*) FROM part "
            "WHERE json_extract(data, '$.tool') IS NOT NULL "
            "GROUP BY json_extract(data, '$.tool')")
        tool_counts = {r[0]: r[1] for r in tools if r[0]}

        # Extract all bash commands containing 'cog' from part table
        cog_rows = query_db(prog_db,
            "SELECT json_extract(data, '$.state.input.command') FROM part "
            "WHERE json_extract(data, '$.state.input.command') LIKE '%cog %'")

        # Classify cog commands
        cog_cmds = []
        cog_summary = defaultdict(int)
        for (cmd,) in cog_rows:
            if not cmd:
                continue
            cog_cmds.append(cmd)
            # Classify by subcommand
            for subcmd in ["init", "query", "assert", "retract", "verify", "stats",
                           "index", "impact", "depend", "next", "experiment"]:
                if f"cog {subcmd}" in cmd or f"cog experiment {subcmd}" in cmd:
                    key = subcmd if subcmd != "experiment" else f"exp_{subcmd}"
                    # More specific experiment subcommands
                    if "experiment start" in cmd: key = "exp_start"
                    elif "experiment hypothesize" in cmd: key = "exp_hypothesize"
                    elif "experiment evaluate" in cmd: key = "exp_evaluate"
                    elif "experiment commit" in cmd: key = "exp_commit"
                    elif "experiment discard" in cmd: key = "exp_discard"
                    cog_summary[key] += 1
                    break
            else:
                cog_summary["other"] += 1

        # Token totals from message table
        token_rows = query_db(prog_db,
            "SELECT data FROM message WHERE json_extract(data, '$.tokens') IS NOT NULL")
        total_in = total_out = 0
        for (data_str,) in token_rows:
            d = json.loads(data_str)
            t = d.get("tokens", {})
            total_in += t.get("input", 0)
            total_out += t.get("output", 0)

        # Ordered tool calls for impact timing analysis
        ordered_tools = query_db(prog_db,
            "SELECT json_extract(data, '$.tool'), "
            "COALESCE(json_extract(data, '$.state.input.command'), '') "
            "FROM part WHERE json_extract(data, '$.tool') IS NOT NULL "
            "ORDER BY rowid")
        first_edit_idx = None
        has_impact_before_edit = False
        has_impact_after_edit = False
        for idx, (tool, cmd) in enumerate(ordered_tools):
            t = (tool or "").lower()
            if t == "edit" and first_edit_idx is None:
                first_edit_idx = idx
            if cmd and "cog impact" in cmd:
                if first_edit_idx is None or idx < first_edit_idx:
                    has_impact_before_edit = True
                else:
                    has_impact_after_edit = True

        if has_impact_before_edit and not has_impact_after_edit:
            impact_timing = "before"
        elif has_impact_after_edit and not has_impact_before_edit:
            impact_timing = "after"
        elif has_impact_before_edit and has_impact_after_edit:
            impact_timing = "mixed"
        else:
            impact_timing = None

        epochs[epoch_num] = {
            "tool_counts": tool_counts,
            "cog_cmds": cog_cmds,
            "cog_summary": dict(cog_summary),
            "cog_total": len(cog_cmds),
            "tokens_in": total_in,
            "tokens_out": total_out,
            "impact_timing": impact_timing,
        }

    return {"has_traj": True, "epochs": epochs}


# ── SWE-CI metrics ──────────────────────────────────────────────────

def compute_metrics(it: dict, max_epoch: int = 20) -> dict:
    """Compute metrics matching official SWE-CI summarize.py metrics_func."""
    init_passed = it["init_passed"]
    total_gap = it["init_gap"]
    target = init_passed + total_gap

    if total_gap == 0:
        return {"EvoScore": 1.0, "Resolved": 1.0, "ZeroReg": 1.0, "ZRR": 1.0}

    evo = it["epochs"]
    if not evo:
        return {"EvoScore": 0.0, "Resolved": 0.0, "ZeroReg": 1.0, "ZRR": 0.0}

    # Build evo_seq of passed counts, padded/truncated to max_epoch
    evo_seq = [min(max(0, e["passed"]), target) for e in evo]
    actual_len = len(evo_seq)
    if actual_len < max_epoch:
        fill_value = evo_seq[-1] if actual_len > 0 else init_passed
        evo_seq = evo_seq + [fill_value] * (max_epoch - actual_len)
    else:
        evo_seq = evo_seq[:max_epoch]

    # Relative changes
    rela_changes = []
    for p in evo_seq:
        if p > init_passed:
            change = (p - init_passed) / total_gap if total_gap > 0 else 1.0
        elif p < init_passed:
            change = (p - init_passed) / init_passed if init_passed > 0 else -1.0
        else:
            change = 0
        rela_changes.append(change)
    evo_score = sum(rela_changes) / max_epoch

    # Resolved: any point in [init_pass] + evo_seq reached target
    seq_with_init = [init_passed] + evo_seq
    resolved = float(max(seq_with_init) == target)

    # Zero regression: no decrease across entire sequence including init
    zero_reg = 1.0
    for i in range(len(seq_with_init) - 1):
        if seq_with_init[i + 1] < seq_with_init[i]:
            zero_reg = 0.0
            break

    return {
        "EvoScore": round(evo_score, 4),
        "Resolved": resolved,
        "ZeroReg": zero_reg,
        "ZRR": float(resolved and zero_reg),
    }


# ── Single experiment analysis ──────────────────────────────────────

def analyze_experiment(exp_dir: Path, max_epoch: int = 20) -> dict:
    exp_dir = exp_dir.resolve()
    if not exp_dir.exists():
        print(f"Error: {exp_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    tasks = sorted(
        [(d.name, d) for d in exp_dir.iterdir()
         if d.is_dir() and (d / "iteration.jsonl").exists()],
        key=lambda x: x[0],
    )
    if not tasks:
        print(f"No task directories found in {exp_dir}", file=sys.stderr)
        sys.exit(1)

    results = {}
    for task_id, task_dir in tasks:
        it = analyze_iterations(task_dir)
        cog = analyze_cog(task_dir)
        traj = analyze_trajectories(task_dir)
        metrics = compute_metrics(it, max_epoch=max_epoch)
        results[task_id] = {"iterations": it, "cog": cog, "trajectories": traj, "metrics": metrics}

    return results


# ── Formatters ───────────────────────────────────────────────────────

def _short_task(task_id: str) -> str:
    """Truncate task ID to 8 chars for compact display."""
    return task_id[:8] if len(task_id) > 8 else task_id


def print_summary_table(results: dict, title: str = ""):
    console = Console()

    if title:
        console.print(f"\n  {title}", style="bold")
        console.print("─" * 80)

    table = Table(show_header=True, header_style="bold", show_lines=False,
                  border_style="dim", padding=(0, 1), expand=False)
    table.add_column("Task", style="cyan", no_wrap=True)
    table.add_column("EvoSc", justify="right", min_width=6)
    table.add_column("Res", justify="right", min_width=4)
    table.add_column("ZReg", justify="right", min_width=4)
    table.add_column("ZRR", justify="right", min_width=4)
    table.add_column("Ep", justify="right", min_width=3)
    table.add_column("Gap", no_wrap=True)
    table.add_column("InTok", justify="right", min_width=9)
    table.add_column("OutTok", justify="right", min_width=9)

    totals = defaultdict(float)
    n = len(results)
    for task_id, r in results.items():
        m = r["metrics"]
        it = r["iterations"]
        gap_trace = "→".join(str(g) for g in it.get("gap_trace", []))
        if len(gap_trace) > 30:
            gap_trace = gap_trace[:27] + "…"
        table.add_row(
            _short_task(task_id),
            f"{m['EvoScore']:.3f}",
            str(int(m['Resolved'])),
            str(int(m['ZeroReg'])),
            str(int(m['ZRR'])),
            str(it['n_epochs']),
            gap_trace,
            f"{it['total_in']:,}",
            f"{it['total_out']:,}",
        )
        for k in ["EvoScore", "Resolved", "ZeroReg", "ZRR"]:
            totals[k] += m[k]
        totals["in"] += it["total_in"]
        totals["out"] += it["total_out"]

    if n > 0:
        table.add_row(
            f"AVERAGE ({n} tasks)",
            f"{totals['EvoScore']/n:.3f}",
            f"{totals['Resolved']/n:.2f}",
            f"{totals['ZeroReg']/n:.2f}",
            f"{totals['ZRR']/n:.2f}",
            "",
            "",
            f"{totals['in']:,.0f}",
            f"{totals['out']:,.0f}",
            style="bold",
        )

    console.print(table)


def print_cog_detail(results: dict):
    """Print cog usage details per task."""
    has_any_cog = any(r["cog"].get("has_cog") for r in results.values())
    if not has_any_cog:
        print("\n  No cog model data found.")
        return

    print(f"\n{'─'*80}")
    print("  Cog Model Usage")
    print(f"{'─'*80}")

    for task_id, r in results.items():
        cog = r["cog"]
        if not cog.get("has_cog"):
            continue

        print(f"\n  {_short_task(task_id)}")
        print(f"    Entities: {cog['entities']}, Assertions: {cog['n_active']} active, {cog['n_retracted']} retracted")
        if cog["kinds"]:
            print(f"    Kinds: {cog['kinds']}")

        # Trajectory cog usage
        traj = r["trajectories"]
        if traj.get("has_traj"):
            for ep_num in sorted(traj["epochs"]):
                ep = traj["epochs"][ep_num]
                if ep["cog_total"] > 0:
                    print(f"    Epoch {ep_num}: {ep['cog_total']} cog calls — {ep['cog_summary']}")

        # Growth across snapshots
        if cog["growth"]:
            g = cog["growth"]
            print(f"    Growth: {g[0]['entities']}→{g[-1]['entities']} entities, "
                  f"{g[0]['active']}→{g[-1]['active']} assertions "
                  f"across {len(g)} snapshots")

        # Assertion details (truncated claims)
        if cog["assertions"]:
            active = [a for a in cog["assertions"] if a["status"] == "active"]
            if active:
                print(f"    Active assertions:")
                for a in active[:8]:
                    claim = a["claim"][:90] + "…" if len(a["claim"]) > 90 else a["claim"]
                    print(f"      [{a['kind']}] {claim}")
                if len(active) > 8:
                    print(f"      ... and {len(active) - 8} more")


def print_regressions(results: dict):
    """Print regression details."""
    has_reg = False
    for r in results.values():
        for reg in r["iterations"].get("regressions", []):
            if not has_reg:
                print(f"\n{'─'*80}")
                print("  Regressions")
                print(f"{'─'*80}")
                has_reg = True
            print(f"    Task {_short_task(r.get('_task_id', '?'))}: Epoch {reg['epoch']}, "
                  f"passed {reg['prev']}→{reg['cur']}")


# ── Cog Quality Analysis ─────────────────────────────────────────────

def analyze_quality(results: dict) -> dict:
    """Analyze cog usage quality signals per task (§5.5 of SWE-CI-GUIDE.md)."""
    quality = {}
    for task_id, r in results.items():
        q = {
            "has_cog": False,
            "persistence_rate": 0.0,
            "persistence_detail": "",
            "assertion_growth": False,
            "assertion_growth_delta": 0,
            "kind_quality": 0.0,
            "kind_detail": "",
            "command_diversity": 0,
            "command_detail": "",
            "impact_before": 0,
            "impact_after": 0,
            "impact_detail": "",
            "next_used": 0,
            "has_retract": False,
            "good_signals": [],
            "bad_signals": [],
        }

        cog = r["cog"]
        traj = r["trajectories"]

        if not cog.get("has_cog"):
            quality[task_id] = q
            continue

        q["has_cog"] = True

        # Persistence rate: fraction of archived snapshots with cog data
        growth = cog.get("growth", [])
        total_snaps = len(growth)
        if total_snaps > 0:
            snaps_with_data = sum(1 for g in growth if g["entities"] > 0)
            q["persistence_rate"] = snaps_with_data / total_snaps
            q["persistence_detail"] = f"{snaps_with_data}/{total_snaps} snapshots"

        # Assertion growth across snapshots
        if len(growth) > 1:
            first_active = growth[0]["active"]
            last_active = growth[-1]["active"]
            q["assertion_growth_delta"] = last_active - first_active
            q["assertion_growth"] = last_active > first_active

        # Kind quality: fraction of contract + invariant
        kinds = cog.get("kinds", {})
        total_assertions = sum(kinds.values())
        if total_assertions > 0:
            high_value = kinds.get("contract", 0) + kinds.get("invariant", 0)
            q["kind_quality"] = high_value / total_assertions
            q["kind_detail"] = ", ".join(f"{k}:{v}" for k, v in sorted(kinds.items()))
        else:
            q["kind_detail"] = "no assertions"

        # Trajectory-based analysis
        if traj.get("has_traj"):
            all_subcmds = set()
            for ep_num in sorted(traj["epochs"]):
                ep = traj["epochs"][ep_num]
                summary = ep.get("cog_summary", {})
                all_subcmds.update(summary.keys())
                q["next_used"] += summary.get("next", 0)
                if "retract" in summary:
                    q["has_retract"] = True

                # Impact timing
                timing = ep.get("impact_timing")
                if timing == "before":
                    q["impact_before"] += 1
                elif timing == "after":
                    q["impact_after"] += 1
                elif timing == "mixed":
                    q["impact_before"] += 1
                    q["impact_after"] += 1

            q["command_diversity"] = len(all_subcmds)
            q["command_detail"] = ", ".join(sorted(all_subcmds)) if all_subcmds else "none"

            # Build impact detail string
            total_impact = q["impact_before"] + q["impact_after"]
            if total_impact > 0:
                before_only = sum(1 for ep in traj["epochs"].values()
                                  if ep.get("impact_timing") == "before")
                after_only = sum(1 for ep in traj["epochs"].values()
                                 if ep.get("impact_timing") == "after")
                mixed = sum(1 for ep in traj["epochs"].values()
                            if ep.get("impact_timing") == "mixed")
                parts = []
                if before_only:
                    parts.append(f"{before_only} before")
                if after_only:
                    parts.append(f"{after_only} after")
                if mixed:
                    parts.append(f"{mixed} mixed")
                q["impact_detail"] = ", ".join(parts) + " edit"
            else:
                q["impact_detail"] = "no impact calls"

            # ── Good signals ──
            if q["persistence_rate"] >= 0.8:
                q["good_signals"].append("cog DB persists across iterations")
            if q["assertion_growth"]:
                q["good_signals"].append(f"assertions grow ({q['assertion_growth_delta']:+d})")
            if q["kind_quality"] >= 0.3:
                q["good_signals"].append(f"high-value kinds {q['kind_quality']:.0%}")
            if q["command_diversity"] >= 4:
                q["good_signals"].append(f"diverse usage ({q['command_diversity']} cmds)")
            if q["impact_before"] > q["impact_after"]:
                q["good_signals"].append("impact used before edits")
            if q["next_used"] > 0:
                q["good_signals"].append(f"cog next used ({q['next_used']}x)")
            if q["has_retract"]:
                q["good_signals"].append("retract used on behavior change")

            # ── Bad signals ──
            if q["command_diversity"] <= 1:
                q["bad_signals"].append("only cog init, no query/assert/impact")
            elif "query" not in all_subcmds and "assert" not in all_subcmds:
                q["bad_signals"].append("no query or assert calls")
            if not q["assertion_growth"] and len(growth) > 1 and total_assertions > 0:
                q["bad_signals"].append("assertion count flat across iterations")
            if q["kind_quality"] < 0.2 and total_assertions > 3:
                q["bad_signals"].append("no contract/invariant assertions")
            if q["impact_after"] > q["impact_before"] and q["impact_after"] > 0:
                q["bad_signals"].append("impact mostly AFTER edits (too late)")

        quality[task_id] = q
    return quality


def print_quality_report(results: dict):
    """Print cog quality metrics table and signal analysis."""
    quality = analyze_quality(results)
    console = Console()

    cog_tasks = {tid: q for tid, q in quality.items() if q["has_cog"]}
    if not cog_tasks:
        console.print("\n  No cog data — skipping quality analysis.")
        return

    console.print(f"\n  Cog Quality Metrics", style="bold")
    console.print("─" * 80)

    table = Table(show_header=True, header_style="bold", show_lines=False,
                  border_style="dim", padding=(0, 1), expand=False)
    table.add_column("Task", style="cyan", no_wrap=True)
    table.add_column("Persist", justify="center", min_width=7)
    table.add_column("Growth", justify="center", min_width=8)
    table.add_column("Kind%", justify="right", min_width=5)
    table.add_column("Diversity", justify="center", min_width=8)
    table.add_column("Impact Timing", min_width=14)
    table.add_column("Next", justify="right", min_width=4)
    table.add_column("Retract", justify="center", min_width=6)

    for tid, q in cog_tasks.items():
        persist_str = f"{q['persistence_rate']:.0%}" if q["persistence_detail"] else "—"
        growth_str = f"+{q['assertion_growth_delta']}" if q["assertion_growth"] else (
            str(q["assertion_growth_delta"]) if q["assertion_growth_delta"] != 0 else "0")
        kind_str = f"{q['kind_quality']:.0%}" if q["kind_detail"] != "no assertions" else "—"
        div_str = str(q["command_diversity"])
        retract_str = "✓" if q["has_retract"] else "—"
        next_str = str(q["next_used"]) if q["next_used"] > 0 else "—"

        table.add_row(
            _short_task(tid),
            persist_str,
            growth_str,
            kind_str,
            div_str,
            q["impact_detail"] or "—",
            next_str,
            retract_str,
        )

    console.print(table)

    # Quality signals summary
    all_good = []
    all_bad = []
    for tid, q in cog_tasks.items():
        for s in q["good_signals"]:
            all_good.append((_short_task(tid), s))
        for s in q["bad_signals"]:
            all_bad.append((_short_task(tid), s))

    if all_good:
        console.print(f"\n  Good signals:", style="bold green")
        for task, signal in all_good:
            console.print(f"    [green]✓[/green] {task}: {signal}")

    if all_bad:
        console.print(f"\n  Bad signals:", style="bold yellow")
        for task, signal in all_bad:
            console.print(f"    [yellow]⚠[/yellow] {task}: {signal}")

    if not all_bad:
        console.print(f"\n  [green]No bad signals detected.[/green]")

    return quality

# ── A/B Comparison ──────────────────────────────────────────────────

def print_comparison(baseline: dict, cog: dict, baseline_dir: str, cog_dir: str):
    """Print A/B comparison table."""
    console = Console()
    common_tasks = sorted(set(baseline) & set(cog))
    baseline_only = sorted(set(baseline) - set(cog))
    cog_only = sorted(set(cog) - set(baseline))

    console.print(f"\n  A/B COMPARISON", style="bold")
    console.print(f"  Baseline: {baseline_dir}")
    console.print(f"  Cog:      {cog_dir}")
    console.print("═" * 90)

    if baseline_only:
        console.print(f"\n  Baseline-only tasks: {[_short_task(t) for t in baseline_only]}")
    if cog_only:
        console.print(f"\n  Cog-only tasks: {[_short_task(t) for t in cog_only]}")

    if not common_tasks:
        console.print("\n  No common tasks to compare.")
        return

    table = Table(show_header=True, header_style="bold", show_lines=False,
                  border_style="dim", padding=(0, 1), expand=False)
    table.add_column("Task", style="cyan", no_wrap=True)
    table.add_column("Base EvoS", justify="right", min_width=7)
    table.add_column("Cog EvoS", justify="right", min_width=7)
    table.add_column("Base Res", justify="right", min_width=5)
    table.add_column("Cog Res", justify="right", min_width=5)
    table.add_column("Base ZReg", justify="right", min_width=5)
    table.add_column("Cog ZReg", justify="right", min_width=5)
    table.add_column("Base ZRR", justify="right", min_width=5)
    table.add_column("Cog ZRR", justify="right", min_width=5)
    table.add_column("ΔTok Out", justify="right", min_width=10)

    b_totals = defaultdict(float)
    c_totals = defaultdict(float)

    for tid in common_tasks:
        bm = baseline[tid]["metrics"]
        cm = cog[tid]["metrics"]
        bi = baseline[tid]["iterations"]
        ci = cog[tid]["iterations"]

        b_totals["EvoScore"] += bm["EvoScore"]
        b_totals["Resolved"] += bm["Resolved"]
        b_totals["ZeroReg"] += bm["ZeroReg"]
        b_totals["ZRR"] += bm["ZRR"]
        c_totals["EvoScore"] += cm["EvoScore"]
        c_totals["Resolved"] += cm["Resolved"]
        c_totals["ZeroReg"] += cm["ZeroReg"]
        c_totals["ZRR"] += cm["ZRR"]

        delta_tok = ci["total_out"] - bi["total_out"]
        delta_str = f"{delta_tok:+,}" if delta_tok != 0 else "="

        table.add_row(
            _short_task(tid),
            f"{bm['EvoScore']:.3f}", f"{cm['EvoScore']:.3f}",
            str(int(bm['Resolved'])), str(int(cm['Resolved'])),
            str(int(bm['ZeroReg'])), str(int(cm['ZeroReg'])),
            str(int(bm['ZRR'])), str(int(cm['ZRR'])),
            delta_str,
        )

    n = len(common_tasks)
    table.add_row(
        f"AVERAGE ({n})",
        f"{b_totals['EvoScore']/n:.3f}", f"{c_totals['EvoScore']/n:.3f}",
        f"{b_totals['Resolved']/n:.2f}", f"{c_totals['Resolved']/n:.2f}",
        f"{b_totals['ZeroReg']/n:.2f}", f"{c_totals['ZeroReg']/n:.2f}",
        f"{b_totals['ZRR']/n:.2f}", f"{c_totals['ZRR']/n:.2f}",
        "",
        style="bold",
    )

    console.print(table)

    # Verbose: gap traces side by side
    console.print(f"\n  Gap Trace Comparison:", style="bold")
    for tid in common_tasks:
        b_trace = "→".join(str(g) for g in baseline[tid]["iterations"].get("gap_trace", []))
        c_trace = "→".join(str(g) for g in cog[tid]["iterations"].get("gap_trace", []))
        b_epochs = baseline[tid]["iterations"]["n_epochs"]
        c_epochs = cog[tid]["iterations"]["n_epochs"]
        console.print(f"    {_short_task(tid)}")
        console.print(f"      baseline ({b_epochs}ep): {b_trace}")
        console.print(f"      cog      ({c_epochs}ep): {c_trace}")

    # Cog-specific metrics
    console.print(f"\n  Cog-specific (treatment only):", style="bold")
    cog_tasks_with_cog = [(tid, cog[tid]) for tid in common_tasks if cog[tid]["cog"].get("has_cog")]
    if cog_tasks_with_cog:
        for tid, r in cog_tasks_with_cog:
            c = r["cog"]
            traj = r["trajectories"]
            total_cog_calls = sum(ep["cog_total"] for ep in traj.get("epochs", {}).values()) if traj.get("has_traj") else 0
            console.print(f"    {_short_task(tid)}: {c['entities']} ents, {c['n_active']} asserts, {total_cog_calls} cog CLI calls")
    else:
        console.print("    No cog data found in treatment group.")


# ── Main ─────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Evaluate SWE-CI experiment results")
    parser.add_argument("dirs", nargs="+", type=Path, help="Experiment directory(s)")
    parser.add_argument("--compare", action="store_true",
                        help="A/B comparison: first dir=baseline, second dir=cog")
    parser.add_argument("--max-epoch", type=int, default=20,
                        help="Max evolution epochs (must match config.toml, default: 20)")
    parser.add_argument("--json", action="store_true", help="Also output JSON report")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show per-task detail")
    args = parser.parse_args()

    if args.compare and len(args.dirs) != 2:
        parser.error("--compare requires exactly 2 directories")
        return

    if args.compare:
        baseline = analyze_experiment(args.dirs[0], max_epoch=args.max_epoch)
        cog = analyze_experiment(args.dirs[1], max_epoch=args.max_epoch)
        print_comparison(baseline, cog, str(args.dirs[0]), str(args.dirs[1]))

        has_any_cog = any(r["cog"].get("has_cog") for r in cog.values())
        if has_any_cog:
            quality = print_quality_report(cog)

        if args.verbose:
            print_cog_detail(cog)
            print_regressions(baseline)
            print_regressions(cog)

        if args.json:
            path = args.dirs[1].resolve() / "evaluation_comparison.json"
            with open(path, "w") as f:
                json.dump({"baseline": baseline, "cog": cog}, f, indent=2, default=str)
            print(f"\n  JSON report: {path}")
    else:
        for exp_dir in args.dirs:
            results = analyze_experiment(exp_dir, max_epoch=args.max_epoch)
            print_summary_table(results, title=str(exp_dir))

            # Always show quality report when cog data exists
            has_any_cog = any(r["cog"].get("has_cog") for r in results.values())
            if has_any_cog:
                quality = print_quality_report(results)

            if args.verbose:
                print_cog_detail(results)
                for tid, r in results.items():
                    r["_task_id"] = tid
                print_regressions(results)

            if args.json:
                path = exp_dir.resolve() / "evaluation_report.json"
                report = dict(results)
                if has_any_cog:
                    report["_quality"] = quality
                with open(path, "w") as f:
                    json.dump(report, f, indent=2, default=str)
                print(f"\n  JSON report: {path}")


if __name__ == "__main__":
    main()
