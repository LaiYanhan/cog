use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
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
    /// Change in progress — code modified, awaiting verification
    Changing {
        description: String,
        started_at: DateTime<Utc>,
        affected_entities: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowPhase {
    /// Just init'd, no assertions yet
    FreshScan,
    /// Browsing, querying, recording assertions — all model interaction
    Exploring,
    /// Running impact/trace to assess blast radius
    Assessing,
    /// Code just modified, verify passed, awaiting correction recording
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

    /// Enter change-tracking mode.
    pub fn transition_start_change(
        &mut self,
        description: String,
        affected_entities: Vec<String>,
    ) -> Result<()> {
        match self {
            WorkflowState::Ready { .. } => {
                *self = WorkflowState::Changing {
                    description,
                    started_at: Utc::now(),
                    affected_entities,
                };
                Ok(())
            }
            WorkflowState::Changing { .. } => {
                anyhow::bail!("already in a change cycle; finish or abort first")
            }
            WorkflowState::Uninit => anyhow::bail!("project not initialized"),
        }
    }

    /// Finish change cycle — back to Ready { Exploring }.
    pub fn transition_finish_change(&mut self) -> Result<()> {
        match self {
            WorkflowState::Changing { .. } => {
                *self = WorkflowState::Ready {
                    phase: WorkflowPhase::Exploring,
                };
                Ok(())
            }
            _ => anyhow::bail!("not in a change cycle"),
        }
    }

    /// Abort change cycle — back to Ready { Exploring }.
    pub fn transition_abort_change(&mut self) -> Result<()> {
        match self {
            WorkflowState::Changing { .. } => {
                *self = WorkflowState::Ready {
                    phase: WorkflowPhase::Exploring,
                };
                Ok(())
            }
            _ => anyhow::bail!("not in a change cycle"),
        }
    }

    // ── Command-triggered phase transitions ──

    /// After `verify` passes.
    pub fn transition_verify(&mut self, passed: bool) {
        match self {
            WorkflowState::Changing { .. } => {
                if passed {
                    *self = WorkflowState::Ready {
                        phase: WorkflowPhase::PostChange,
                    };
                }
                // fail → stay Changing
            }
            WorkflowState::Ready {
                phase: WorkflowPhase::Debugging,
            } if passed => {
                *self = WorkflowState::Ready {
                    phase: WorkflowPhase::Exploring,
                };
            }
            // fail → stay Debugging
            _ => {}
        }
    }

    /// After `retract` — always enters Debugging.
    pub fn transition_retract(&mut self) {
        if let WorkflowState::Ready { .. } = self {
            *self = WorkflowState::Ready {
                phase: WorkflowPhase::Debugging,
            };
        }
    }

    /// After `impact` or `trace` — enters Assessing.
    pub fn transition_assess(&mut self) {
        if let WorkflowState::Ready { phase } = self {
            *phase = WorkflowPhase::Assessing;
        }
        // Changing stays Changing
    }

    /// After `query`, `assert`, `depend` — transitions from FreshScan to Exploring.
    /// Exploring stays Exploring. Assessing transitions back to Exploring.
    /// PostChange transitions to Exploring. Debugging stays Debugging.
    pub fn transition_explore(&mut self) {
        if let WorkflowState::Ready { phase } = self {
            match phase {
                WorkflowPhase::FreshScan | WorkflowPhase::Assessing | WorkflowPhase::PostChange => {
                    *phase = WorkflowPhase::Exploring;
                }
                WorkflowPhase::Exploring | WorkflowPhase::Debugging => {}
            }
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
            WorkflowState::Changing { description, .. } => {
                format!("changing: {description}")
            }
        }
    }
}

fn phase_label(phase: &WorkflowPhase) -> &'static str {
    match phase {
        WorkflowPhase::FreshScan => "fresh_scan",
        WorkflowPhase::Exploring => "exploring",
        WorkflowPhase::Assessing => "assessing",
        WorkflowPhase::PostChange => "post_change",
        WorkflowPhase::Debugging => "debugging",
    }
}
