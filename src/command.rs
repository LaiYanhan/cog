pub mod assert_cmd;
pub mod backup_cmd;
pub mod depend;
pub mod entity_cmd;
pub mod experiment_cmd;
pub mod export;
pub mod impact;
pub mod index_cmd;
pub mod migrate_cmd;
pub mod next_cmd;
pub mod query;
pub mod recover;
pub mod retract;
pub mod stats;
pub mod sync_cmd;
pub mod trace;
pub mod verify;

pub struct CommandOutput {
    pub text: String,
    pub exit_code: i32,
    /// Set by `sync` to indicate whether code drift was detected.
    /// Used by the CLI dispatch to drive WorkflowState transitions
    /// without parsing the formatted text output.
    pub has_drift: bool,
}
impl CommandOutput {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: 0,
            has_drift: false,
        }
    }

    pub fn with_exit_code(text: impl Into<String>, exit_code: i32) -> Self {
        Self {
            text: text.into(),
            exit_code,
            has_drift: false,
        }
    }

    pub fn emit(&self) {
        println!("{}", self.text);
    }
}
