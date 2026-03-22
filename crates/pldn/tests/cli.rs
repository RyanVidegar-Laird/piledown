use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_bam() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/SRR21778056-sorted-subsample.bam")
}

fn regions_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/regions.tsv")
}

#[test]
fn single_region_tsv_output() {
    Command::cargo_bin("pldn")
        .unwrap()
        .args([
            test_bam().to_str().unwrap(),
            "-l", "isr",
            "-s", "reverse",
            "-r", "chr1:14900-15200",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("name\tseq\tstrand\tpos\tup\tdown\n"));
}

#[test]
fn regions_file_mode() {
    Command::cargo_bin("pldn")
        .unwrap()
        .args([
            test_bam().to_str().unwrap(),
            "-l", "isr",
            "--regions-file", regions_file().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("name\tseq\tstrand\tpos\tup\tdown\n"));
}

#[test]
fn missing_bam_error() {
    Command::cargo_bin("pldn")
        .unwrap()
        .args([
            "/nonexistent/path.bam",
            "-l", "isr",
            "-s", "reverse",
            "-r", "chr1:100-200",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn missing_strand_with_region() {
    Command::cargo_bin("pldn")
        .unwrap()
        .args([
            test_bam().to_str().unwrap(),
            "-l", "isr",
            "-r", "chr1:100-200",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strand"));
}

#[test]
fn no_region_source_error() {
    Command::cargo_bin("pldn")
        .unwrap()
        .args([
            test_bam().to_str().unwrap(),
            "-l", "isr",
        ])
        .assert()
        .failure();
}
