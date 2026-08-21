//! Integration tests for `topos compiled` CLI subcommands.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_compiled_cli_evaluate_and_plan_flow() {
    let dir = tempdir().expect("failed to create temp dir");
    let test_file = dir.path().join("sample.rs");

    let sample_code = r#"
fn compute_sum(n: u64) -> u64 {
    let mut acc = 0;
    for i in 0..n {
        acc += i;
    }
    acc
}

fn main() {
    println!("{}", compute_sum(100));
}
"#;
    fs::write(&test_file, sample_code).expect("failed to write sample code");

    // 1. Test `topos compiled evaluate`
    let mut cmd_eval = Command::cargo_bin("topos").unwrap();
    let assert_eval = cmd_eval
        .arg("compiled")
        .arg("evaluate")
        .arg(&test_file)
        .arg("--json")
        .assert();

    assert_eval.success();

    // 2. Test `topos compiled plan`
    let plan_file = dir.path().join("plan.json");
    let mut cmd_plan = Command::cargo_bin("topos").unwrap();
    let assert_plan = cmd_plan
        .arg("compiled")
        .arg("plan")
        .arg(&test_file)
        .arg("--out")
        .arg(&plan_file)
        .assert();

    assert_plan.success();
    assert!(plan_file.exists(), "Optimization plan file should exist");

    // 3. Test `topos compiled inspect`
    let mut cmd_inspect = Command::cargo_bin("topos").unwrap();
    let assert_inspect = cmd_inspect
        .arg("compiled")
        .arg("inspect")
        .arg(&test_file)
        .assert();

    assert_inspect.success();
}
