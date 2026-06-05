pub mod assert_cmd;
pub mod branch_cmd;
pub mod depend;
pub mod entity_cmd;
pub mod export;
pub mod impact;
pub mod index_cmd;
pub mod init_cmd;
pub mod query;
pub mod retract;
pub mod stats;
pub mod trace;
pub mod verify;
pub mod experiment_cmd;
pub mod backup_cmd;

pub struct CommandOutput {
    pub text: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: 0,
        }
    }

    pub fn with_exit_code(text: impl Into<String>, exit_code: i32) -> Self {
        Self {
            text: text.into(),
            exit_code,
        }
    }

    pub fn emit(&self) {
        println!("{}", self.text);
    }
}
