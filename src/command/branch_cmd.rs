use anyhow::{Result, bail};
use chrono::Utc;

use crate::cli::BranchAction;
use crate::command::CommandOutput;
use crate::format;
use crate::model::{BranchManager, ModelDiff, Store};

pub fn execute(store: &Store, mgr: &BranchManager, action: &BranchAction) -> Result<CommandOutput> {
    match action {
        BranchAction::Create { name } => create_branch(store, mgr, name),
        BranchAction::List => list_branches(mgr),
        BranchAction::Switch { name } => switch_branch(mgr, name),
        BranchAction::Diff { name, item } => diff_branch(store, mgr, name, *item),
        BranchAction::Merge {
            name,
            apply,
            reject,
            apply_all,
        } => merge_branch(store, mgr, name, *apply, *reject, *apply_all),
        BranchAction::Drop { name } => drop_branch(mgr, name),
    }
}

fn create_branch(
    store: &Store,
    mgr: &BranchManager,
    name: &Option<String>,
) -> Result<CommandOutput> {
    let final_name = match name {
        Some(n) => n.clone(),
        None => {
            let ts = Utc::now().format("%Y%m%d_%H%M%S");
            format!("branch_{ts}")
        }
    };

    let info = mgr.create(store, &final_name)?;
    Ok(CommandOutput::success(format!(
        "branch created: {} ({}KB)",
        info.name,
        info.size_bytes / 1024
    )))
}

fn list_branches(mgr: &BranchManager) -> Result<CommandOutput> {
    let branches = mgr.list()?;
    Ok(CommandOutput::success(format::branch_list_report(
        &branches,
    )))
}

fn switch_branch(mgr: &BranchManager, name: &str) -> Result<CommandOutput> {
    if name == "_main" {
        let active = mgr.active_branch();
        mgr.switch_to_main(active.as_deref())?;
        return Ok(CommandOutput::success("switched back to main"));
    }

    mgr.switch_to_branch(name)?;
    Ok(CommandOutput::success(format!(
        "switched to branch: {name}"
    )))
}

fn diff_branch(
    store: &Store,
    mgr: &BranchManager,
    name: &str,
    item_index: Option<usize>,
) -> Result<CommandOutput> {
    let branch_store = mgr.load_branch_store(name)?;
    let base_snapshot = store.snapshot()?;
    let branch_snapshot = branch_store.snapshot()?;

    // Diff: main (base) vs branch (with edits) — what the branch adds/removes/changes
    let diff = ModelDiff::diff(&base_snapshot, &branch_snapshot);

    if let Some(idx) = item_index {
        let items = diff.items();
        if idx >= items.len() {
            bail!("item index {} out of range (0..{})", idx, items.len());
        }
        return Ok(CommandOutput::success(format::diff_item_detail(
            idx,
            &items[idx],
        )));
    }

    let summary = diff.summary_counts();
    if diff.is_empty() {
        return Ok(CommandOutput::success("diff: no changes"));
    }

    // Show summary + numbered item list
    let mut out = format::diff_summary(&summary);
    let items = diff.items();
    for (i, item) in items.iter().enumerate() {
        out.push_str(&format!("  [{}] {}\n", i, format::item_label(item)));
    }
    out.push_str("use --item <N> to inspect a specific change\n");

    Ok(CommandOutput::success(out))
}

fn merge_branch(
    store: &Store,
    mgr: &BranchManager,
    name: &str,
    apply: Option<usize>,
    reject: Option<usize>,
    apply_all: bool,
) -> Result<CommandOutput> {
    let branch_store = mgr.load_branch_store(name)?;
    let base_snapshot = store.snapshot()?;
    let branch_snapshot = branch_store.snapshot()?;

    // Diff: main (base) vs branch (with edits)
    let diff = ModelDiff::diff(&base_snapshot, &branch_snapshot);
    let items = diff.items();

    if apply_all {
        let mut applied = 0;
        let mut skipped = 0;
        for item in diff.items() {
            match apply_item(store, &item) {
                Ok(true) => applied += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    return Err(e.context(format!(
                        "merge failed on item [{}] ({})",
                        applied + skipped,
                        format::item_label(&item),
                    )));
                }
            }
        }
        return Ok(CommandOutput::success(format!(
            "merge: applied {}, skipped {}",
            applied, skipped
        )));
    }

    if let Some(idx) = apply {
        if idx >= items.len() {
            bail!("item index {} out of range (0..{})", idx, items.len());
        }
        match apply_item(store, &items[idx])? {
            true => Ok(CommandOutput::success(format!(
                "merge: applied item [{}]",
                idx
            ))),
            false => Ok(CommandOutput::success(format!(
                "merge: item [{}] skipped (entity removal requires manual handling)",
                idx
            ))),
        }
    } else if let Some(idx) = reject {
        if idx >= items.len() {
            bail!("item index {} out of range (0..{})", idx, items.len());
        }
        Ok(CommandOutput::success(format!(
            "merge: rejected item [{}]",
            idx
        )))
    } else {
        // Show plan
        Ok(CommandOutput::success(format::merge_plan(&diff)))
    }
}

fn apply_item(store: &Store, item: &crate::model::DiffItem) -> Result<bool> {
    use crate::model::DiffItem;

    match item {
        DiffItem::EntityAdded(e) => {
            // Insert with original UUID so cross-references (assertions, relations) remain valid
            store.insert_entity(e)
        }
        DiffItem::EntityRemoved(_) => {
            // Cannot safely remove entities with assertions
            Ok(false)
        }
        DiffItem::AssertionAdded(a) => {
            // Entity must exist (inserted via EntityAdded with original UUID above)
            if store.get_entity(&a.entity_id)?.is_none() {
                return Ok(false);
            }
            store.insert_assertion(a)
        }
        DiffItem::AssertionRemoved(a) => {
            if store.get_assertion(&a.id)?.is_some() {
                store.retract_assertion(&a.id, "merged: removed in branch")?;
            }
            Ok(true)
        }
        DiffItem::AssertionChanged(change) => {
            if change.before.status != change.after.status
                && store.get_assertion(&change.before.id)?.is_some()
            {
                use crate::model::AssertionStatus;
                match change.after.status {
                    AssertionStatus::Retracted => {
                        store.retract_assertion(
                            &change.before.id,
                            "merged: status change from branch",
                        )?;
                    }
                    AssertionStatus::Uncertain => {
                        store.update_assertion_status(
                            &change.before.id,
                            AssertionStatus::Uncertain,
                        )?;
                    }
                    AssertionStatus::Active => {
                        store
                            .update_assertion_status(&change.before.id, AssertionStatus::Active)?;
                    }
                }
            }
            Ok(true)
        }
        DiffItem::EvidenceAdded(e) => {
            if store.get_assertion(&e.assertion_id)?.is_some() {
                store.insert_evidence(e)
            } else {
                Ok(false)
            }
        }
        DiffItem::EvidenceRemoved(_) => Ok(false),
        DiffItem::EntityRelationAdded(r) => {
            if store.get_entity(&r.from_entity)?.is_some()
                && store.get_entity(&r.to_entity)?.is_some()
            {
                store.add_entity_relation(&r.from_entity, &r.to_entity, r.kind)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        DiffItem::EntityRelationRemoved(_) => Ok(false),
        DiffItem::AssertionRelationAdded(r) => {
            if store.get_assertion(&r.from_assertion)?.is_some()
                && store.get_assertion(&r.to_assertion)?.is_some()
            {
                store.add_assertion_dependency(&r.from_assertion, &r.to_assertion)?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        DiffItem::AssertionRelationRemoved(_) => Ok(false),
    }
}

fn drop_branch(mgr: &BranchManager, name: &str) -> Result<CommandOutput> {
    mgr.drop(name)?;
    Ok(CommandOutput::success(format!("branch dropped: {name}")))
}
