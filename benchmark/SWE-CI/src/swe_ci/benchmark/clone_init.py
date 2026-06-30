"""Clone initialization state from a previous experiment to skip slow init.

Copies current/, target/, and the first line of iteration.jsonl (the initial
gap record) from an old experiment to a new one.  When evaluate.py runs
afterward, init_tasks() sees iteration.jsonl and skips immediately, but
run_tasks() sees only one epoch record and starts evolution fresh.

Handles both live experiments (current/ still present — evolution interrupted)
and completed experiments (current/ renamed to a timestamp — use the earliest
timestamp as the initial state).
"""

import json
import re
import shutil
from pathlib import Path

from .utils import read_csv
from .utils.log import empty_logger, file_handler, console_handler, add_handler, remove_handler
from swe_ci.config import CONFIG

# Timestamp directories created by evolution: YYYY-MM-DD-HH-MM-SS
_TIMESTAMP_RE = re.compile(r"^\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}$")


def _find_initial_current(src_task: Path) -> Path | None:
    """Find the initial post-init current/ state in a (possibly completed) task dir.

    Returns the path to use as current/, or None if nothing suitable exists.

    Priority:
      1. src_task/current/ — evolution was interrupted, current/ still live.
      2. Earliest timestamp directory — current/ was archived after first epoch.
    """
    live = src_task / "current"
    if live.is_dir():
        return live

    # Collect timestamp directories, sort chronologically by name
    timestamps = sorted(
        [d for d in src_task.iterdir() if d.is_dir() and _TIMESTAMP_RE.match(d.name)],
        key=lambda d: d.name,
    )
    if timestamps:
        return timestamps[0]  # earliest = initial post-init state

    return None


def clone_init_from(source_experiment: str) -> bool:
    """Copy initialized task state from source_experiment to current experiment.

    For each task in the current splitting:
      - Locate the initial current/ state (live or from earliest timestamp).
      - Copy it along with target/ and the first iteration.jsonl record.
      - If source lacks needed data → skip.

    Returns True if ALL tasks were successfully cloned.
    """
    experiment_dir = Path("experiments") / CONFIG.experiment_name
    source_dir = Path("experiments") / source_experiment
    experiment_dir.mkdir(parents=True, exist_ok=True)
    from .run import _write_meta_if_missing
    _write_meta_if_missing(experiment_dir)

    metadata_file = Path(CONFIG.save_root_dir) / "metadata" / f"{CONFIG.splitting}.csv"
    metadatas = read_csv(metadata_file)
    num_tasks = len(metadatas)

    # Setup logger
    main_logger = empty_logger("main")
    f_handler = file_handler(experiment_dir / "main.log")
    c_handler = console_handler()
    add_handler(main_logger, f_handler)
    add_handler(main_logger, c_handler)

    main_logger.info(f"{'='*30} Cloning init from '{source_experiment}' {'='*30}")

    n_success, n_skip = 0, 0
    for metadata in metadatas:
        task_id = metadata["task_id"]
        src_task = source_dir / task_id
        dst_task = experiment_dir / task_id

        # Already initialized in destination? Skip.
        if (dst_task / "iteration.jsonl").is_file():
            main_logger.info(f"⏭️  {task_id}: already initialized, skipping")
            n_success += 1
            continue

        # Check source has iteration.jsonl
        if not (src_task / "iteration.jsonl").is_file():
            main_logger.warning(f"⚠️  {task_id}: not initialized in source, skipping")
            n_skip += 1
            continue

        # Locate initial current/ state
        src_current = _find_initial_current(src_task)
        if src_current is None:
            main_logger.warning(f"⚠️  {task_id}: no current/ or timestamp archives in source, skipping")
            n_skip += 1
            continue

        # Check target/ exists
        src_target = src_task / "target"
        if not src_target.is_dir():
            main_logger.warning(f"⚠️  {task_id}: missing target/ in source, skipping")
            n_skip += 1
            continue

        try:
            dst_task.mkdir(parents=True, exist_ok=True)

            # Copy initial current/ state and target/
            shutil.copytree(src_current, dst_task / "current", symlinks=True)
            shutil.copytree(src_target, dst_task / "target", symlinks=True)

            # Copy only the first line of iteration.jsonl (the init record)
            with open(src_task / "iteration.jsonl", "r", encoding="utf-8") as f:
                first_line = f.readline().strip()
            if not first_line:
                main_logger.warning(f"⚠️  {task_id}: empty iteration.jsonl in source, skipping")
                shutil.rmtree(dst_task)
                n_skip += 1
                continue

            (dst_task / "iteration.jsonl").write_text(first_line + "\n", encoding="utf-8")

            label = "live" if src_current.name == "current" else f"from {src_current.name}"
            main_logger.info(f"✅ {task_id}: cloned ({label}, gap={json.loads(first_line).get('gap', '?')})")
            n_success += 1
        except Exception as e:
            main_logger.error(f"❌ {task_id}: clone failed — {e}")
            shutil.rmtree(dst_task, ignore_errors=True)
            n_skip += 1

    main_logger.info(
        f"Totaling {num_tasks} tasks. {n_success} cloned/skipped, {n_skip} skipped (not in source)."
    )

    remove_handler(main_logger, f_handler)
    remove_handler(main_logger, c_handler)
    return n_skip == 0
