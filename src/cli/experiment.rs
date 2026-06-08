use clap::Subcommand;

use crate::domain::AssertionKind;

#[derive(Debug, Subcommand)]
pub enum ExperimentAction {
    /// Start a new experiment focused on an entity
    /// Quick sandbox: start + hypothesize + evaluate in one command
    Try {
        /// Entity to focus the experiment on
        entity: String,
        /// Kind of hypothetical assertion
        #[arg(long)]
        kind: AssertionKind,
        /// The claim
        #[arg(long)]
        claim: String,
        /// Grounds for the claim
        #[arg(long)]
        grounds: String,
        /// Optional experiment description (default: "<entity>: <claim>")
        #[arg(long)]
        desc: Option<String>,
        /// ID of another assertion this hypothesis depends on
        #[arg(long)]
        depends_on: Option<String>,
    },
    /// Start a new experiment on an entity. For complex scenarios
    /// requiring multiple hypotheses — use `try` for quick one-liner.
    Start {
        /// Entity to focus the experiment on
        entity: String,
        /// Description of the experiment
        #[arg(long)]
        description: Option<String>,
        /// Maximum nodes to load into the experiment subgraph
        #[arg(long, default_value = "500")]
        max_nodes: usize,
    },
    /// Add a hypothetical assertion to the experiment
    Hypothesize {
        /// Experiment ID (short or full)
        id: String,
        /// Entity to assert about
        #[arg(long)]
        entity: String,
        /// Kind of assertion
        #[arg(long)]
        kind: AssertionKind,
        /// The claim
        #[arg(long)]
        claim: String,
        /// Grounds for the claim
        #[arg(long)]
        grounds: String,
    },
    /// Stage a hypothetical entity relation
    HypotheticalRelation {
        /// Experiment id
        #[arg(long)]
        id: String,
        /// Source entity
        #[arg(long)]
        from: String,
        /// Target entity
        #[arg(long)]
        to: String,
        /// Relation kind (contains, calls, uses)
        #[arg(long)]
        kind: crate::domain::EntityRelationKind,
    },
    /// Stage a hypothetical entity deletion
    HypotheticalDelete {
        /// Experiment id
        #[arg(long)]
        id: String,
        /// Entity to hypothetically delete
        #[arg(long)]
        entity: String,
    },
    /// Evaluate the experiment — simulate cascade and detect contradictions
    Evaluate {
        /// Experiment ID
        id: String,
    },
    /// Show the experiment report
    Report {
        /// Experiment ID
        id: String,
    },
    /// Commit the experiment to the real model
    Commit {
        /// Experiment ID
        id: String,
    },
    /// Discard the experiment without changes
    Discard {
        /// Experiment ID
        id: String,
    },
    /// List all saved experiments
    List,
    /// Save the current experiment state to disk
    Save {
        /// Experiment ID
        id: String,
    },
    /// Load a saved experiment from disk
    Load {
        /// Experiment ID
        id: String,
    },
}
