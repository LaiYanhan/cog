use anyhow::{Result, anyhow};

use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
use crate::repo::sqlite::SqliteRepository;

#[test]
fn upsert_and_fetch_entity() -> Result<()> {
    let store = SqliteRepository::open_in_memory()?;

    let created = store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
    let fetched = store
        .get_entity_by_name("auth::login")?
        .ok_or_else(|| anyhow!("missing entity"))?;

    assert_eq!(created.id, fetched.id);
    assert_eq!(created.qualified_name, fetched.qualified_name);
    Ok(())
}

#[test]
fn create_assertion_with_evidence_and_dependency() -> Result<()> {
    let store = SqliteRepository::open_in_memory()?;

    let entity = store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
    let base = store.create_assertion(
        &entity.id,
        AssertionKind::Contract,
        "returns option token",
        "code:auth::login",
        None,
    )?;

    let dependent = store.create_assertion(
        &entity.id,
        AssertionKind::Invariant,
        "none means failure",
        "test:test_login_fail",
        Some(&base.id),
    )?;

    let evidences = store.get_evidence_for_assertion(&dependent.id)?;
    assert_eq!(evidences.len(), 1);
    let dependencies = store.get_dependencies(&dependent.id)?;
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].id, base.id);

    Ok(())
}
