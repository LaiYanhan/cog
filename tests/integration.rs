use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;

fn cog_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_cog")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_cog should be set by cargo test")
}

fn run(db: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(cog_bin());
    cmd.arg("--db").arg(db).args(args);
    cmd.output().expect("failed to execute cog binary")
}

fn run_ok(db: &Path, args: &[&str]) -> String {
    let output = run(db, args);
    assert!(
        output.status.success(),
        "command failed\nargs: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn parse_assertion_id(output: &str) -> String {
    // New V2 format: "Created <short_id> [kind] on <entity>"
    let raw = output
        .lines()
        .find_map(|line| line.strip_prefix("Created "))
        .expect("assert output should contain 'Created <id>'");
    // Extract the first word (short_id) before the next space
    raw.split_whitespace()
        .next()
        .expect("should have short_id after 'Created '")
        .to_owned()
}

#[test]
fn full_cli_workflow_happy_path() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    let base_assert = run_ok(
        &db,
        &[
            "assert",
            "auth::login",
            "--kind",
            "contract",
            "--claim",
            "returns token",
            "--grounds",
            "code:auth::login",
        ],
    );
    let base_id = parse_assertion_id(&base_assert);

    run_ok(
        &db,
        &[
            "assert",
            "auth::login",
            "--kind",
            "invariant",
            "--claim",
            "none means failure",
            "--grounds",
            "test:test_login_fail",
            "--depends-on",
            &base_id,
        ],
    );

    run_ok(
        &db,
        &[
            "depend",
            "auth::login",
            "--on",
            "AuthToken",
            "--kind",
            "uses",
        ],
    );

    let query_output = run_ok(&db, &["query", "auth::login"]);
    assert!(query_output.contains("auth::login"));
    assert!(query_output.contains("none means failure"));

    let impact_output = run_ok(&db, &["impact", "auth::login"]);
    assert!(impact_output.contains("Impact for: auth::login"));

    let trace_output = run_ok(&db, &["trace", "auth::login"]);
    assert!(trace_output.contains("trace_entity: auth::login"));

    let stats_output = run_ok(&db, &["stats"]);
    assert!(stats_output.contains("entities:"));
    assert!(stats_output.contains("assertions:"));

    let export_output = run_ok(&db, &["export", "--format", "json"]);
    assert!(export_output.contains("\"entities\""));

    let verify_output = run_ok(&db, &["verify"]);
    assert!(verify_output.contains("verify: ok"));
}

#[test]
fn verify_reports_dependency_on_retracted() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    let base_output = run_ok(
        &db,
        &[
            "assert",
            "auth::login",
            "--kind",
            "contract",
            "--claim",
            "root assumption",
            "--grounds",
            "code:auth::login",
        ],
    );
    let base_id = parse_assertion_id(&base_output);

    run_ok(
        &db,
        &["retract", &base_id, "--reason", "invalid assumption"],
    );

    run_ok(
        &db,
        &[
            "assert",
            "auth::login",
            "--kind",
            "invariant",
            "--claim",
            "dependent on retracted",
            "--grounds",
            "test:late-check",
            "--depends-on",
            &base_id,
        ],
    );

    let verify = run(&db, &["verify"]);
    assert!(
        !verify.status.success(),
        "verify should fail when issues exist"
    );
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("DependencyOnRetracted"));
}

// ── Experiment boundary_count tests ────────────────────────────────────────

#[test]
fn experiment_try_shows_boundary_count_not_uuids() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    // Create enough entities to populate the model
    run_ok(
        &db,
        &[
            "assert",
            "alpha",
            "--kind",
            "contract",
            "--claim",
            "c1",
            "--grounds",
            "code:alpha",
        ],
    );
    run_ok(
        &db,
        &[
            "assert",
            "beta",
            "--kind",
            "contract",
            "--claim",
            "c2",
            "--grounds",
            "code:beta",
        ],
    );

    let output = run_ok(
        &db,
        &[
            "experiment",
            "try",
            "alpha",
            "--kind",
            "correction",
            "--claim",
            "fix",
            "--grounds",
            "code:alpha",
            "--desc",
            "boundary test",
        ],
    );
    // Should NOT contain raw UUID patterns for boundary entities
    assert!(
        !output.contains("boundary_entities"),
        "should not reference boundary_entities field, got:\n{output}"
    );
}

#[test]
fn experiment_try_scout_has_no_read_action() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    run_ok(
        &db,
        &[
            "assert",
            "x",
            "--kind",
            "contract",
            "--claim",
            "c",
            "--grounds",
            "code:x",
        ],
    );

    let output = run_ok(
        &db,
        &[
            "experiment",
            "try",
            "x",
            "--kind",
            "correction",
            "--claim",
            "fix",
            "--grounds",
            "code:x",
            "--desc",
            "scout test",
        ],
    );

    // Scout section should only have [verify] or [assert], never [read]
    assert!(
        !output.contains("[read]"),
        "scout output should not contain [read] actions, got:\n{output}"
    );
}

// ── Debugging state suggestion tests ───────────────────────────────────────

/// Write a workflow_state.json into the cog_dir (db's parent) to set up phase.
fn set_workflow_state(db: &Path, phase: &str) {
    let cog_dir = db.parent().unwrap();
    let state = format!("{{\"Ready\":{{\"phase\":\"{phase}\"}}}}");
    std::fs::write(cog_dir.join("workflow_state.json"), state).unwrap();
}

#[test]
fn next_in_debugging_after_retract_shows_context() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    // Setup: create entity via assert, set workflow to Exploring, then retract
    let assert_output = run_ok(
        &db,
        &[
            "assert",
            "my::func",
            "--kind",
            "contract",
            "--claim",
            "does stuff",
            "--grounds",
            "code:my::func",
        ],
    );
    let assert_id = parse_assertion_id(&assert_output);

    // Workflow starts as Uninit after assert; manually set to Exploring
    // so retract can transition to Debugging.
    set_workflow_state(&db, "Exploring");

    run_ok(&db, &["retract", &assert_id, "--reason", "broken"]);

    let next_output = run_ok(&db, &["next"]);

    // Should be in debugging state
    assert!(
        next_output.contains("debugging"),
        "should be in debugging state after retract, got:\n{next_output}"
    );

    // Should show retraction review, not generic trace/verify
    assert!(
        next_output.contains("retracted"),
        "should mention retracted assertions, got:\n{next_output}"
    );
    assert!(
        !next_output.contains("[trace]"),
        "should not suggest generic trace in debugging, got:\n{next_output}"
    );
    assert!(
        !next_output.contains("cog verify"),
        "should not suggest generic verify in debugging, got:\n{next_output}"
    );
}

#[test]
fn next_in_debugging_after_correction_shows_entity_names() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    // Setup: create entity via assert, set workflow to Exploring, then retract
    let assert_output = run_ok(
        &db,
        &[
            "assert",
            "svc::handler",
            "--kind",
            "contract",
            "--claim",
            "handles requests",
            "--grounds",
            "code:svc::handler",
        ],
    );
    let assert_id = parse_assertion_id(&assert_output);

    set_workflow_state(&db, "Exploring");
    run_ok(&db, &["retract", &assert_id, "--reason", "broken"]);

    // Add a correction (this calls transition_explore which stays in Debugging)
    run_ok(
        &db,
        &[
            "assert",
            "svc::handler",
            "--kind",
            "correction",
            "--claim",
            "fixed null check",
            "--grounds",
            "code:svc::handler",
        ],
    );

    let next_output = run_ok(&db, &["next"]);

    // Should show [recover] with the entity name
    assert!(
        next_output.contains("[recover]"),
        "should show [recover] action when corrections exist, got:\n{next_output}"
    );
    assert!(
        next_output.contains("svc::handler"),
        "should mention the corrected entity name, got:\n{next_output}"
    );
    assert!(
        next_output.contains("cog query svc::handler"),
        "should suggest querying the specific entity, got:\n{next_output}"
    );
}

// ── from-scratch bootstrap + sync stale-removal regressions ────────────────

#[test]
fn sync_init_on_empty_dir_succeeds_and_advances_workflow() {
    // #1/#2: empty-dir sync --init used to exit 1 and leave the workflow stuck
    // at Uninit, dead-locking `cog next` into "run sync" forever. It must now
    // succeed so the design phase can bootstrap via `cog assert ... --grounds plan:`.
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let cog_dir = root.join(".cog");
    std::fs::create_dir_all(&cog_dir).unwrap();
    let db = cog_dir.join("cog.db");

    let out = run(&db, &["sync", "--init"]);
    assert!(
        out.status.success(),
        "sync --init on empty dir should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("No source files found yet"),
        "expected design-phase bootstrap hint, got: {}",
        stdout
    );

    // `next` must not dead-loop back into "run sync" / "no cognitive model".
    let next = run_ok(&db, &["next"]);
    assert!(
        !next.contains("No cognitive model found"),
        "next still dead-loops to sync:\n{next}"
    );
    assert!(
        next.contains("fresh_scan"),
        "expected workflow to advance to fresh_scan, got:\n{next}"
    );
}

#[test]
fn sync_removes_stale_entity_without_transaction_crash() {
    // #3: sync was wrapped in an outer transaction while delete_entity opens
    // its own → nested BEGIN crashed ("cannot start a transaction within a
    // transaction") whenever a stale entity had to be removed. This is a
    // subprocess test so it exercises the real CLI dispatch path.
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let cog_dir = root.join(".cog");
    std::fs::create_dir_all(&cog_dir).unwrap();
    let db = cog_dir.join("cog.db");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .unwrap();

    let init = run(&db, &["sync", "--init"]);
    assert!(
        init.status.success(),
        "initial sync failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Remove beta → it becomes a stale Scan-origin entity.
    std::fs::write(root.join("src/lib.rs"), "pub fn alpha() {}\n").unwrap();

    let out = run(&db, &["sync"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "sync after removing code must not crash (transaction regression):\n{stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("transaction"),
        "sync must not error on transactions, got: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("stale"),
        "expected stale removal report, got: {stdout}"
    );

    let idx = run_ok(&db, &["index", "--verbose"]);
    assert!(
        !idx.contains("beta"),
        "stale entity beta still present:\n{idx}"
    );
    assert!(idx.contains("alpha"), "alpha should remain:\n{idx}");
}

#[test]
fn migrate_moves_assertions_from_manual_to_scan_entity() {
    // #4: design-phase assertions recorded against a Manual entity (logical name)
    // become orphans once sync produces the path-named Scan entity. `cog migrate`
    // must re-assign the assertions onto the real entity and delete the orphan.
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let cog_dir = root.join(".cog");
    std::fs::create_dir_all(&cog_dir).unwrap();
    let db = cog_dir.join("cog.db");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub struct Lexer {}\nimpl Lexer { pub fn tokenize(&self) {} }\n",
    )
    .unwrap();

    run_ok(&db, &["sync", "--init"]);

    // Design-phase assertion on a logical name that won't match the scan name.
    run_ok(
        &db,
        &[
            "assert",
            "myapp::lexer::Lexer",
            "--kind",
            "contract",
            "--claim",
            "single-pass tokenizer",
            "--grounds",
            "plan:design",
        ],
    );
    run_ok(
        &db,
        &[
            "assert",
            "myapp::lexer::Lexer",
            "--kind",
            "fragility",
            "--claim",
            "assumes utf8",
            "--grounds",
            "plan:design",
        ],
    );

    // The scan entity is path-named (src::lib::Lexer); the manual one is an orphan.
    // verify exits non-zero when issues exist, so use `run` and inspect stdout.
    let v_out = run(&db, &["verify"]);
    let v = String::from_utf8_lossy(&v_out.stdout);
    assert!(
        v.contains("OrphanManualEntity") && v.contains("myapp::lexer::Lexer"),
        "expected orphan before migrate, got: {v}"
    );

    let out = run_ok(&db, &["migrate", "myapp::lexer::Lexer", "src::lib::Lexer"]);
    assert!(
        out.contains("Migrated 2 assertion"),
        "expected 2 migrated, got: {out}"
    );
    assert!(
        out.contains("Source entity deleted"),
        "expected source deletion, got: {out}"
    );

    // Assertions now live on the real entity; orphan cleared.
    let q = run_ok(&db, &["query", "src::lib::Lexer"]);
    assert!(
        q.contains("single-pass tokenizer") && q.contains("assumes utf8"),
        "assertions should have moved to src::lib::Lexer, got: {q}"
    );

    let v2_out = run(&db, &["verify"]);
    let v2 = String::from_utf8_lossy(&v2_out.stdout);
    assert!(
        !v2.contains("myapp::lexer::Lexer"),
        "orphan should be cleared, got: {v2}"
    );
}
