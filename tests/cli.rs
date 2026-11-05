//! Black-box tests of the built binary.
#![expect(clippy::unwrap_used, reason = "test helpers should fail loudly")]

mod common;

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Command, Output, Stdio};

use common::{TreeBuilder, assert_trees_equal, sample_tree};

const BIN: &str = env!("CARGO_BIN_EXE_fastcp");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env_remove("FASTCP_CHUNK")
        .env_remove("FASTCP_MEM")
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn quiet_pipeline_copy_succeeds() {
    let src = sample_tree();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let result = run(&[
        "-q",
        "--mode",
        "pipeline",
        "--chunk-size",
        "64K",
        src.path().to_str().unwrap(),
        dst.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn explicit_copy_subcommand_succeeds() {
    let src = sample_tree();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let result = run(&[
        "copy",
        "-q",
        src.path().to_str().unwrap(),
        dst.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_trees_equal(src.path(), &dst);
}

#[test]
fn dry_run_writes_nothing() {
    let src = sample_tree();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let result = run(&["-n", src.path().to_str().unwrap(), dst.to_str().unwrap()]);
    assert!(result.status.success());
    assert!(!dst.exists());
}

#[test]
fn dry_run_prints_the_plan() {
    let src = sample_tree();
    let result = run(&["-n", src.path().to_str().unwrap(), "/nonexistent-dst"]);
    assert!(stderr(&result).contains("plan"), "{}", stderr(&result));
}

#[test]
fn probe_prints_device_table() {
    let dir = tempfile::tempdir().unwrap();
    let result = run(&["probe", dir.path().to_str().unwrap(), "/"]);
    assert!(result.status.success());
    assert!(stderr(&result).contains("copy"), "{}", stderr(&result));
}

#[test]
fn completions_bash_emits_script() {
    let result = run(&["completions", "bash"]);
    assert!(String::from_utf8_lossy(&result.stdout).contains("complete"));
}

#[test]
fn bad_chunk_size_is_usage_error() {
    let result = run(&["a", "b", "--chunk-size", "12X"]);
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn bad_chunk_size_explains_units() {
    let result = run(&["a", "b", "--chunk-size", "12X"]);
    assert!(stderr(&result).contains("use K, M, G or T"));
}

#[test]
fn missing_source_exits_1() {
    let result = run(&["/definitely/missing", "/tmp/whatever"]);
    assert_eq!(result.status.code(), Some(1));
}

#[test]
fn dst_inside_src_is_refused() {
    let src = sample_tree();
    let inside = src.path().join("backup");
    let result = run(&[src.path().to_str().unwrap(), inside.to_str().unwrap()]);
    assert_eq!(result.status.code(), Some(1));
}

#[test]
fn if_exists_fail_refuses_existing_destination() {
    let src = sample_tree();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");
    let (s, d) = (src.path().to_str().unwrap(), dst.to_str().unwrap());
    assert!(run(&["-q", s, d]).status.success());
    assert_eq!(
        run(&["-q", "--if-exists", "fail", s, d]).status.code(),
        Some(1)
    );
}

#[test]
fn ctrl_c_exit_code_is_130_and_partial_files_are_removed() {
    // A large sparse file copied in tiny chunks keeps the copy busy long enough to interrupt.
    let src = TreeBuilder::default().dir("d").build();
    fs::File::create(src.path().join("huge.bin"))
        .unwrap()
        .set_len(2 << 30)
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dst = out.path().join("copy");

    let mut child = Command::new(BIN)
        .args([
            "--mode",
            "pipeline",
            "--chunk-size",
            "4K",
            "--mem",
            "64K",
            "--color",
            "never",
        ])
        .arg(src.path())
        .arg(&dst)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // The plan line is printed after the signal handler is installed and before copying starts.
    let stderr = child.stderr.take().unwrap();
    let mut lines = BufReader::new(stderr).lines();
    assert!(lines.any(|line| line.unwrap().contains("plan")));

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    lines.for_each(drop);

    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    assert!(!dst.join("huge.bin").exists(), "partial file left behind");
}
