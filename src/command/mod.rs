pub mod assert_cmd;
pub mod depend;
pub mod export;
pub mod impact;
pub mod index_cmd;
pub mod query;
pub mod retract;
pub mod stats;
pub mod trace;
pub mod verify;

use crate::model::EntityKind;

pub(crate) fn infer_entity_kind(qualified_name: &str) -> EntityKind {
    let symbol = qualified_name.rsplit("::").next().unwrap_or(qualified_name);

    if symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
        EntityKind::Type
    } else if qualified_name.contains("::") {
        EntityKind::Function
    } else {
        EntityKind::Module
    }
}

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
