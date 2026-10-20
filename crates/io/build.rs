//! The commit the model cache is keyed by (`SOLIDCRAFT_SOURCE_COMMIT`): a cache is only the
//! result of evaluating its design when the same source computed it. "unknown" without git (a
//! source tarball), which never trusts a cache.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=SOLIDCRAFT_SOURCE_COMMIT={commit}");
    // A new commit in this checkout (or worktree) restamps the build.
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
