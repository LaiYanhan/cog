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
    if let Some(start) = raw.find('(')
        && let Some(end) = raw.find(')')
        && end > start
    {
        return raw[start + 1..end].to_owned();
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
