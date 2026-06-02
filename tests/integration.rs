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
    let raw = output
        .lines()
        .find_map(|line| line.strip_prefix("- id: "))
        .expect("assert output should contain assertion id");
    // format: "short_id (full_uuid)" — extract the full uuid
    if let Some(start) = raw.find('(') {
        if let Some(end) = raw.find(')') {
            if end > start {
                return raw[start + 1..end].to_owned();
            }
        }
    }
    raw.to_owned()
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
            "code:src/auth.rs:10",
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
    assert!(query_output.contains("entity: auth::login"));
    assert!(query_output.contains("none means failure"));

    let impact_output = run_ok(&db, &["impact", "auth::login"]);
    assert!(impact_output.contains("impact_from: auth::login"));

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
            "code:src/auth.rs:1",
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

#[test]
fn branch_create_diff_merge_workflow() {
    let tmp = tempdir().expect("tempdir should be created");
    let db = tmp.path().join("cog.db");

    // Populate initial model
    run_ok(
        &db,
        &[
            "assert",
            "auth::login",
            "--kind",
            "contract",
            "--claim",
            "returns token",
            "--grounds",
            "code:src/auth.rs:1",
        ],
    );

    // Create a branch
    let create_output = run_ok(&db, &["branch", "create", "--name", "refactor"]);
    assert!(create_output.contains("branch created: refactor"));

    // List branches
    let list_output = run_ok(&db, &["branch", "list"]);
    assert!(list_output.contains("refactor"));

    // Switch to branch
    let switch_output = run_ok(&db, &["branch", "switch", "refactor"]);
    assert!(switch_output.contains("switched to branch: refactor"));

    // Make changes in the branch (new entity + assertion)
    run_ok(
        &db,
        &[
            "assert",
            "auth::logout",
            "--kind",
            "intent",
            "--claim",
            "clears session",
            "--grounds",
            "code:src/auth.rs:50",
        ],
    );

    // Switch back to main (this saves the branch state to the branch file)
    let back_output = run_ok(&db, &["branch", "switch", "_main"]);
    assert!(back_output.contains("switched back to main"));

    // Verify main doesn't have the new entity yet
    let index_output = run_ok(&db, &["index"]);
    assert!(!index_output.contains("auth::logout"));

    // Diff against the branch — should show the new entity and assertion
    let diff_output = run_ok(&db, &["branch", "diff", "refactor"]);
    assert!(diff_output.contains("diff:"));
    assert!(diff_output.contains("auth::logout"));

    // Inspect specific item
    let item_output = run_ok(&db, &["branch", "diff", "refactor", "--item", "0"]);
    assert!(item_output.contains("[0]"));

    // Show merge plan
    let plan_output = run_ok(&db, &["branch", "merge", "refactor"]);
    assert!(plan_output.contains("pending"));

    // Apply all changes
    let merge_output = run_ok(&db, &["branch", "merge", "refactor", "--apply-all"]);
    assert!(merge_output.contains("applied"));

    // Now the new entity should exist in main
    let query_after = run_ok(&db, &["query", "auth::logout"]);
    assert!(query_after.contains("auth::logout"));

    // Drop the branch
    let drop_output = run_ok(&db, &["branch", "drop", "refactor"]);
    assert!(drop_output.contains("branch dropped: refactor"));

    // Verify branch is gone
    let list_after = run_ok(&db, &["branch", "list"]);
    assert!(list_after.contains("(none)"));
}
