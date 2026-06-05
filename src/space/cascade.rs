use std::collections::{HashSet, VecDeque};

use anyhow::{Result, anyhow, bail};

use crate::domain::*;
use crate::repo::SqliteRepository;

pub struct CascadeEngine;

impl CascadeEngine {
    /// Execute a cascade retraction: retract the assertion and propagate
    /// Uncertain/GroundWeakened status to all transitive dependents.
    pub fn retract(
        repo: &SqliteRepository,
        assertion_id: &str,
        reason: &str,
    ) -> Result<CascadeReport> {
        let current = repo
            .get_assertion(assertion_id)?
            .ok_or_else(|| anyhow!("assertion not found: {assertion_id}"))?;

        if current.status == AssertionStatus::Retracted {
            bail!("assertion already retracted: {assertion_id}");
        }

        // Build the retracted view from `current` — avoids a re-fetch after the transaction.
        let retracted = Assertion {
            status: AssertionStatus::Retracted,
            retraction_reason: Some(reason.to_string()),
            ..current
        };

        let affected = repo.transaction(|| {
            repo.retract_assertion(assertion_id, reason)?;
            repo.append_changelog(ChangelogAction::Retract, assertion_id, reason)?;

            let mut queue = VecDeque::from([assertion_id.to_string()]);
            let mut seen = HashSet::new();
            let mut affected = Vec::new();

            while let Some(current_id) = queue.pop_front() {
                if !seen.insert(current_id.clone()) {
                    continue;
                }

                for dependent in repo.get_dependents(&current_id)? {
                    if dependent.status == AssertionStatus::Retracted {
                        continue;
                    }

                    let dependencies = repo.get_dependencies(&dependent.id)?;
                    let has_independent_active = dependencies.iter().any(|dep| {
                        dep.id != current_id
                            && dep.status != AssertionStatus::Retracted
                            && dep.status != AssertionStatus::Uncertain
                    });

                    if has_independent_active {
                        affected.push(AffectedAssertion {
                            assertion: dependent,
                            cascade_reason: CascadeReason::GroundWeakened,
                        });
                        continue;
                    }

                    let mut updated = dependent;
                    if updated.status != AssertionStatus::Uncertain {
                        repo.update_assertion_status(&updated.id, AssertionStatus::Uncertain)?;
                        updated.status = AssertionStatus::Uncertain;
                    }

                    repo.append_changelog(
                        ChangelogAction::CascadeMark,
                        &updated.id,
                        &format!("marked uncertain due to dependency retraction: {current_id}"),
                    )?;

                    queue.push_back(updated.id.clone());
                    affected.push(AffectedAssertion {
                        assertion: updated,
                        cascade_reason: CascadeReason::MarkedUncertain,
                    });
                }
            }

            Ok(affected)
        })?;

        Ok(CascadeReport {
            retracted,
            affected,
        })
    }
}
