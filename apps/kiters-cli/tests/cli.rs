//! The command line builds a site from the shell.

use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn builds_from_the_command_line() {
    let fixture: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixture")
        .canonicalize()
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_kiters"))
        .args(["build", "--site", fixture.to_str().unwrap(), "--out", out.path().to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(out.path().join("posts/hello-world/index.html").is_file());
    assert!(out.path().join("sitemap.xml").is_file());
}
