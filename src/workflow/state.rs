use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Workflow state — serialized to `.cog/workflow_state.json`.
///
/// Loaded on each command invocation, updated based on transition rules,
/// written back when state changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowState {
    /// Project not initialized (or .cog/ directory missing)
    Uninit,
    /// Initialized, model available
    Ready { phase: WorkflowPhase },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowPhase {
    /// Just init'd, no assertions yet
    FreshScan,
    /// Browsing, querying, recording assertions — all model interaction
    Exploring,
    /// Experiment committed — model updated but code not yet changed.
    /// Agent must implement planned changes, then sync.
    PendingImplement,
    /// Code just modified (sync detected drift), awaiting model reconciliation
    PostChange,
    /// Problem found — retract triggered TMS cascade, or verify found inconsistency
    Debugging,
}

impl WorkflowState {
    /// Load from `.cog/workflow_state.json` relative to the given directory.
    /// Returns `Uninit` if file doesn't exist or is unreadable.
    pub fn load(cog_dir: &Path) -> Self {
        let path = cog_dir.join("workflow_state.json");
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(WorkflowState::Uninit),
            Err(_) => WorkflowState::Uninit,
        }
    }

    /// Save to `.cog/workflow_state.json` relative to the given directory.
    pub fn save(&self, cog_dir: &Path) -> Result<()> {
        let path = cog_dir.join("workflow_state.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    // ── Top-level transitions ──

    /// After `init` — only valid from Uninit.
    pub fn transition_init(&mut self) -> Result<()> {
        match self {
            WorkflowState::Uninit => {
                *self = WorkflowState::Ready {
                    phase: WorkflowPhase::FreshScan,
                };
                Ok(())
            }
            _ => anyhow::bail!("project already initialized"),
        }
    }

    // ── Command-triggered phase transitions ──

    /// After `verify` passes.
    /// Debugging + passed → Exploring. Debugging + failed → stays Debugging.
    pub fn transition_verify(&mut self, passed: bool) {
        if let WorkflowState::Ready {
            phase: WorkflowPhase::Debugging,
        } = self
            && passed
        {
            *self = WorkflowState::Ready {
                phase: WorkflowPhase::Exploring,
            };
        }
        // fail → stay Debugging
    }

    /// After `retract` — always enters Debugging.
    pub fn transition_retract(&mut self) {
        if let WorkflowState::Ready { .. } = self {
            *self = WorkflowState::Ready {
                phase: WorkflowPhase::Debugging,
            };
        }
    }

    pub fn transition_explore(&mut self) {
        if let WorkflowState::Ready { phase } = self {
            match phase {
                WorkflowPhase::FreshScan | WorkflowPhase::PostChange => {
                    *phase = WorkflowPhase::Exploring;
                }
                WorkflowPhase::Exploring
                | WorkflowPhase::Debugging
                | WorkflowPhase::PendingImplement => {}
            }
        }
    }

    /// After `sync` — if drift is detected, enter PostChange.
    pub fn transition_sync(&mut self, drift_detected: bool) {
        if drift_detected && let WorkflowState::Ready { .. } = self {
            *self = WorkflowState::Ready {
                phase: WorkflowPhase::PostChange,
            };
        }
    }

    /// After `index`, `stats`, `export` — browsing, no phase change.
    pub fn transition_browse(&mut self) {
        // no-op: browsing doesn't change state
    }

    /// Human-readable description of current state.
    pub fn describe(&self) -> String {
        match self {
            WorkflowState::Uninit => "uninitialized".into(),
            WorkflowState::Ready { phase } => format!("ready ({})", phase_label(phase)),
        }
    }
}

fn phase_label(phase: &WorkflowPhase) -> &'static str {
    match phase {
        WorkflowPhase::FreshScan => "fresh_scan",
        WorkflowPhase::Exploring => "exploring",
        WorkflowPhase::PendingImplement => "pending_implement",
        WorkflowPhase::PostChange => "post_change",
        WorkflowPhase::Debugging => "debugging",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_from_uninit() {
        let mut state = WorkflowState::Uninit;
        state.transition_init().unwrap();
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::FreshScan
            }
        );
    }

    #[test]
    fn init_from_ready_fails() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::FreshScan,
        };
        let err = state.transition_init().unwrap_err();
        assert!(err.to_string().contains("project already initialized"));
    }

    #[test]
    fn verify_pass_from_debugging() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Debugging,
        };
        state.transition_verify(true);
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Exploring
            }
        );
    }

    #[test]
    fn verify_fail_from_debugging() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Debugging,
        };
        state.transition_verify(false);
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Debugging
            }
        );
    }

    #[test]
    fn verify_from_exploring_no_change() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Exploring,
        };
        state.transition_verify(true);
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Exploring
            }
        );
    }

    #[test]
    fn retract_enters_debugging() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Exploring,
        };
        state.transition_retract();
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Debugging
            }
        );
    }

    #[test]
    fn retract_from_uninit_noop() {
        let mut state = WorkflowState::Uninit;
        state.transition_retract();
        assert_eq!(state, WorkflowState::Uninit);
    }

    #[test]
    fn explore_from_fresh_scan() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::FreshScan,
        };
        state.transition_explore();
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Exploring
            }
        );
    }

    #[test]
    fn explore_from_post_change() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::PostChange,
        };
        state.transition_explore();
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Exploring
            }
        );
    }

    #[test]
    fn explore_from_debugging_stays() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Debugging,
        };
        state.transition_explore();
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Debugging
            }
        );
    }

    #[test]
    fn sync_drift_enters_post_change() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Exploring,
        };
        state.transition_sync(true);
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::PostChange
            }
        );
    }

    #[test]
    fn sync_no_drift_stays() {
        let mut state = WorkflowState::Ready {
            phase: WorkflowPhase::Exploring,
        };
        state.transition_sync(false);
        assert_eq!(
            state,
            WorkflowState::Ready {
                phase: WorkflowPhase::Exploring
            }
        );
    }

    #[test]
    fn describe_uninit() {
        let state = WorkflowState::Uninit;
        assert_eq!(state.describe(), "uninitialized");
    }

    #[test]
    fn describe_ready_exploring() {
        let state = WorkflowState::Ready {
            phase: WorkflowPhase::Exploring,
        };
        assert_eq!(state.describe(), "ready (exploring)");
    }
}
