//! Assertion-aware entity display helpers.
//!
//! Several output modules (impact, query relations, scout) share the same
//! spatial-compression pattern: entities with active assertions ("covered")
//! are expanded in full, while entities without ("blind") are collapsed to
//! a count + sample names.  These helpers factor out that logic so every
//! renderer uses the same partitioning and threshold rules.

use crate::domain::Entity;

/// An entity paired with its active-assertion count — the universal unit
/// for assertion-aware display compression across impact, query, and scout.
#[derive(Debug, Clone)]
pub struct AssertedEntity {
    pub entity: Entity,
    pub active_assertions: usize,
}

impl AssertedEntity {
    pub fn is_asserted(&self) -> bool {
        self.active_assertions > 0
    }
}

/// Partition a collection of (Entity, assertion_count) pairs into
/// `(asserted, blind)` groups.
///
/// Callers (impact, query) build this list from their respective data
/// sources; the renderer then expands asserted entries and collapses
/// blind ones.
pub fn partition_by_assertion(
    items: impl IntoIterator<Item = (Entity, usize)>,
) -> (Vec<AssertedEntity>, Vec<AssertedEntity>) {
    items.into_iter().fold(
        (vec![], vec![]),
        |(mut asserted, mut blind), (entity, count)| {
            let ae = AssertedEntity {
                entity,
                active_assertions: count,
            };
            if ae.is_asserted() {
                asserted.push(ae);
            } else {
                blind.push(ae);
            }
            (asserted, blind)
        },
    )
}

/// Maximum asserted entities to show per group before folding the rest.
///
/// TODO: this (and the sample caps in the renderers) should eventually
/// live in a user-facing configuration file rather than being hard-coded.
pub const MAX_ASSERTED: usize = 15;

// ── Pluralisation helpers ───────────────────────────────────────────────

/// Returns `""` for a single item, `"s"` otherwise.
pub fn plural_s(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// Returns `"entity"` for count 1, `"entities"` otherwise.
pub fn entities_word(count: usize) -> &'static str {
    if count == 1 { "entity" } else { "entities" }
}
