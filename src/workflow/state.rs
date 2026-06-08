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

    /// After `query`, `assert`, `depend` — transitions from FreshScan to Exploring.
    /// Exploring stays Exploring. PostChange transitions to Exploring.
    /// Debugging stays Debugging.
    pub fn transition_explore(&mut self) {
        if let WorkflowState::Ready { phase } = self {
            match phase {
                WorkflowPhase::FreshScan | WorkflowPhase::PostChange => {
                    *phase = WorkflowPhase::Exploring;
                }
                WorkflowPhase::Exploring | WorkflowPhase::Debugging => {}
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
        WorkflowPhase::PostChange => "post_change",
        WorkflowPhase::Debugging => "debugging",
    }
}
