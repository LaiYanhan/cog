import shutil
from pathlib import Path

from swe_ci.config import CONFIG
from swe_ci.benchmark.utils import read_csv, read_jsonl


def clear_unfinished_tasks():

    meta = read_csv(f"metadata/{CONFIG.splitting}.csv")
    task_ids = [task["task_id"] for task in meta]
    
    finished_tids = []
    unfinished_tids = []
    uninitialized_tids = []
    for tid in task_ids:
        task_dir = Path(f"experiments/{CONFIG.experiment_name}/{tid}")
        if not task_dir.is_dir(): 
            uninitialized_tids.append(tid)
            continue
        iter_file = task_dir / "iteration.jsonl"
        if not iter_file.is_file(): continue
        iter_log = read_jsonl(iter_file)
        
        has_done = iter_log[-1]["gap"] == 0
        has_exhausted = len(iter_log) >= 1 + CONFIG.evolve.max_epoch
        if has_done or has_exhausted:
            finished_tids.append(tid)
        else:
            unfinished_tids.append(tid)
    
    print(f"Finished: {len(finished_tids)}")
    print(f"Unfinished: {len(unfinished_tids)}")
    print(f"Uninitialized: {len(uninitialized_tids)}")
    
    for tid in unfinished_tids:
        print(f"   {tid}")
    if len(unfinished_tids) == 0:
        print("No tasks need to be cleaned up.")
        exit(0)
    print(f"Clear these {len(unfinished_tids)} tasks? [y/n]")
    while True:
        answer = input().strip().lower()
        if answer == 'y':
            for tid in unfinished_tids:
                task_dir = Path(f"experiments/{CONFIG.experiment_name}/{tid}")
                shutil.rmtree(task_dir)
                print(f"Removed: {task_dir}")
            break
        elif answer == 'n':
            break


if __name__ == "__main__":
    clear_unfinished_tasks()
