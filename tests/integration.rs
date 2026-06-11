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
