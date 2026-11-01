//! End-to-end integration tests for the `dots` binary.
//!
//! Each test runs in an isolated tempdir with HOME overridden to a fake home
//! directory, so we never touch the real user's dotfiles.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dots"))
}

struct Harness {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().join("home");
        let repo = tmp.path().join("dotfiles");
        fs::create_dir_all(&home).expect("mkdir home");
        Harness {
            _tmp: tmp,
            home,
            repo,
        }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = Command::new(bin());
        cmd.arg("--repo").arg(&self.repo).args(args);
        // Override HOME on both unix and windows so dirs::home_dir() uses our fake home.
        #[cfg(unix)]
        cmd.env("HOME", &self.home);
        #[cfg(windows)]
        {
            cmd.env("USERPROFILE", &self.home);
            // dirs on Windows prefers USERPROFILE but may read FOLDERID_Profile via WinAPI.
            // Clearing HOMEPATH/HOMEDRIVE and setting USERPROFILE is the standard override.
            cmd.env("HOMEDRIVE", "");
            cmd.env("HOMEPATH", "");
        }
        cmd.output().expect("spawn")
    }

    fn write_home_file(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.home.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        fs::write(&path, contents).expect("write");
        path
    }
}

fn assert_ok(output: &std::process::Output, label: &str) {
    if !output.status.success() {
        panic!(
            "{} failed:\n  stdout: {}\n  stderr: {}",
            label,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn is_symlink(p: &Path) -> bool {
    fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

#[test]
fn init_creates_config_file() {
    let h = Harness::new();
    let out = h.run(&["init"]);
    assert_ok(&out, "init");
    assert!(h.repo.join("dots.toml").is_file());
}

#[test]
fn init_refuses_to_clobber_existing_config() {
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init 1");
    let second = h.run(&["init"]);
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already exists"));
}

#[test]
fn list_shows_default_profile() {
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");
    let out = h.run(&["list"]);
    assert_ok(&out, "list");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("default"));
}

/// On Windows, these tests require Developer Mode or admin privileges to create symlinks.
/// Skip them unless the symlink probe succeeds.
#[cfg(windows)]
fn can_symlink() -> bool {
    let tmp = match tempfile::tempdir() {
        Ok(t) => t,
        Err(_) => return false,
    };
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::write(&src, "x").ok();
    std::os::windows::fs::symlink_file(&src, &dst).is_ok()
}

#[cfg(unix)]
fn can_symlink() -> bool {
    true
}

#[test]
fn add_moves_file_into_repo_and_symlinks_back() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable (Windows needs Developer Mode or admin)");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");

    let bashrc = h.write_home_file(".bashrc", "export EDITOR=vim\n");
    let out = h.run(&["add", bashrc.to_str().unwrap()]);
    assert_ok(&out, "add");

    assert!(h.repo.join("bashrc").is_file(), "repo file should exist");
    assert!(is_symlink(&bashrc), "home path should now be a symlink");

    // The symlink should resolve back to the moved content.
    let resolved = fs::read_to_string(&bashrc).expect("read via symlink");
    assert_eq!(resolved, "export EDITOR=vim\n");
}

#[test]
fn link_and_unlink_roundtrip() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");

    let bashrc = h.write_home_file(".bashrc", "A\n");
    assert_ok(&h.run(&["add", bashrc.to_str().unwrap()]), "add");

    // Remove the symlink to simulate a fresh machine, then re-link.
    fs::remove_file(&bashrc).expect("remove symlink");
    assert!(!bashrc.exists());

    assert_ok(&h.run(&["link"]), "link");
    assert!(is_symlink(&bashrc), "link should restore symlink");

    assert_ok(&h.run(&["unlink"]), "unlink");
    assert!(!bashrc.exists(), "unlink should remove symlink");

    // The repo file is untouched by unlink.
    assert!(h.repo.join("bashrc").is_file());
}

#[test]
fn link_detects_conflict_without_force() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");

    // Pre-track a file.
    let bashrc = h.write_home_file(".bashrc", "repo-content\n");
    assert_ok(&h.run(&["add", bashrc.to_str().unwrap()]), "add");
    fs::remove_file(&bashrc).expect("remove symlink");

    // Now a real file exists at the target that was NOT created by us.
    fs::write(&bashrc, "pre-existing\n").expect("write existing");

    // link without --force should refuse and exit non-zero.
    let out = h.run(&["link"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("conflict") || stderr.contains("--force"));
    assert!(!is_symlink(&bashrc), "no symlink should have been created");
}

#[test]
fn link_with_force_backs_up_conflict() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");

    let bashrc = h.write_home_file(".bashrc", "repo-content\n");
    assert_ok(&h.run(&["add", bashrc.to_str().unwrap()]), "add");
    fs::remove_file(&bashrc).expect("remove symlink");
    fs::write(&bashrc, "pre-existing\n").expect("pre-existing");

    assert_ok(&h.run(&["link", "--force"]), "link --force");

    assert!(is_symlink(&bashrc), "symlink should exist after --force");
    let bak = {
        let mut p = bashrc.clone().into_os_string();
        p.push(".bak");
        PathBuf::from(p)
    };
    assert!(bak.is_file(), "backup should exist at .bak");
    assert_eq!(fs::read_to_string(&bak).unwrap(), "pre-existing\n");
}

#[test]
fn status_reports_missing_and_linked() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");

    let bashrc = h.write_home_file(".bashrc", "x\n");
    assert_ok(&h.run(&["add", bashrc.to_str().unwrap()]), "add");

    let out = h.run(&["status"]);
    assert_ok(&out, "status (linked)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("linked"), "status should show linked");

    fs::remove_file(&bashrc).unwrap();
    let out = h.run(&["status"]);
    assert_ok(&out, "status (missing)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("missing"), "status should show missing");
}

#[test]
fn dry_run_link_makes_no_changes() {
    if !can_symlink() {
        eprintln!("skipping: symlinks unavailable");
        return;
    }
    let h = Harness::new();
    assert_ok(&h.run(&["init"]), "init");
    let bashrc = h.write_home_file(".bashrc", "x\n");
    assert_ok(&h.run(&["add", bashrc.to_str().unwrap()]), "add");
    fs::remove_file(&bashrc).unwrap();

    assert_ok(&h.run(&["link", "--dry-run"]), "link --dry-run");
    assert!(!bashrc.exists(), "dry-run should not create the symlink");
}
