use anyhow::Result;

use crate::model::Store;
use crate::model::types::ChangelogAction;

pub struct Changelog;

impl Changelog {
    pub fn append(
        store: &Store,
        action: ChangelogAction,
        target_id: &str,
        detail: &str,
    ) -> Result<()> {
        store.append_changelog(action, target_id, detail)
    }
}
