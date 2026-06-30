"""SWE-CI evaluation entry point.

Usage:
    # Full run (init + evolve + summarize)
    PYTHONPATH=src python -u -m swe_ci.evaluate --config_file config_cog.toml

    # Skip init: clone initialized state from a previous experiment, then evolve
    PYTHONPATH=src python -u -m swe_ci.evaluate --config_file config_cog.toml --clone-init-from cog-pilot
"""

import argparse
import sys
from pathlib import Path


def _parse_extra_args() -> argparse.Namespace:
    """Parse flags that are NOT in the TOML config (e.g. --clone-init-from)."""
    pre_parser = argparse.ArgumentParser(add_help=False)
    pre_parser.add_argument("--config_file", default="config.toml", type=str)
    pre_parser.add_argument(
        "--clone-init-from",
        default=None,
        type=str,
        metavar="EXPERIMENT",
        help="Copy initialized task state from a previous experiment instead of running init",
    )
    args, _ = pre_parser.parse_known_args()
    # Ensure config_file is relative to the SWE-CI root so CONFIG loads correctly
    root = Path(__file__).resolve().parents[2]
    args.config_file = str(root / args.config_file)
    return args


_extra = _parse_extra_args()
# Inject config_file into sys.argv so config.py's load_config() picks it up
sys.argv[1:] = ["--config_file", _extra.config_file]

from swe_ci.benchmark import init_tasks, run_tasks, summarize, clone_init_from


if __name__ == "__main__":
    if _extra.clone_init_from:
        if not clone_init_from(_extra.clone_init_from):
            print(
                "Some tasks could not be cloned. "
                "Run without --clone-init-from to initialize them from scratch.",
                flush=True,
            )
            sys.exit(1)
    elif not init_tasks():
        print("Failed to initialize completely, please try again.", flush=True)
        sys.exit(0)

    while not run_tasks():
        pass
    summarize()
