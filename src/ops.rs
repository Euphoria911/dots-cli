use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{Config, Profile, CONFIG_FILENAME};
use crate::paths::{home_dir, home_to_repo, repo_to_home};

/// Describes the state of a tracked file relative to its home-dir target.
#[derive(Debug, PartialEq, Eq)]
pub enum FileState {
    /// The home path is a symlink pointing at the repo file. All good.
    Linked,
    /// The home path doesn't exist at all. `dots link` would create it.
    Missing,
    /// The home path exists but is not a symlink (regular file, dir). `dots link` would back it up.
    Conflict,
    /// The home path is a symlink but points somewhere else.
    Mismatched { target: PathBuf },
}

pub fn init(repo: &Path, profile_name: &str) -> Result<()> {
    fs::create_dir_all(repo).with_context(|| format!("creating repo dir {}", repo.display()))?;
    let config_path = repo.join(CONFIG_FILENAME);
    if config_path.exists() {
        bail!(
            "{} already exists at {}",
            CONFIG_FILENAME,
            config_path.display()
        );
    }
    let cfg = Config::new(profile_name);
    cfg.save(repo)?;
    println!(
        "{} initialized {} at {} (default profile `{}`)",
        "✓".green().bold(),
        CONFIG_FILENAME,
        config_path.display(),
        profile_name
    );
    Ok(())
}

pub fn add(repo: &Path, path: &Path, profile: Option<&str>) -> Result<()> {
    let home = home_dir()?;
    let rel = home_to_repo(path, &home).context("mapping path into repo-relative form")?;

    let mut cfg = Config::load(repo)?;
    let profile_name = cfg.resolve_profile_name(profile).to_string();
    cfg.profiles
        .entry(profile_name.clone())
        .or_insert_with(Profile::default);

    let home_target = repo_to_home(&rel, &home);
    let repo_target = repo.join(&rel);

    if !home_target.exists() && !home_target.is_symlink() {
        bail!("source file does not exist: {}", home_target.display());
    }
    if home_target.is_symlink() {
        bail!(
            "`{}` is already a symlink; refusing to re-add",
            home_target.display()
        );
    }
    if repo_target.exists() {
        bail!("repo file already exists: {}", repo_target.display());
    }

    if let Some(parent) = repo_target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::rename(&home_target, &repo_target).with_context(|| {
        format!(
            "moving {} → {}",
            home_target.display(),
            repo_target.display()
        )
    })?;
    create_symlink(&repo_target, &home_target)?;

    let profile_entry = cfg.profiles.get_mut(&profile_name).expect("just inserted");
    if !profile_entry.files.contains(&rel) {
        profile_entry.files.push(rel.clone());
    }
    cfg.save(repo)?;

    println!(
        "{} tracked `{}` under profile `{}`",
        "✓".green().bold(),
        rel,
        profile_name
    );
    Ok(())
}

pub fn link(repo: &Path, profile: Option<&str>, force: bool, dry_run: bool) -> Result<()> {
    let home = home_dir()?;
    let cfg = Config::load(repo)?;
    let profile_name = cfg.resolve_profile_name(profile).to_string();
    let files = cfg.resolve_files(&profile_name)?;

    if files.is_empty() {
        println!("{} profile `{}` has no files", "·".dimmed(), profile_name);
        return Ok(());
    }

    let mut linked = 0;
    let mut skipped = 0;
    let mut backed_up = 0;
    let mut errors = 0;

    for rel in &files {
        let repo_target = repo.join(rel);
        let home_target = repo_to_home(rel, &home);

        if !repo_target.exists() {
            eprintln!(
                "{} {}: not in repo at {}",
                "✗".red().bold(),
                rel,
                repo_target.display()
            );
            errors += 1;
            continue;
        }

        let state = classify(&repo_target, &home_target);
        match state {
            FileState::Linked => {
                println!("{} {} (already linked)", "·".dimmed(), rel);
                skipped += 1;
            }
            FileState::Missing => {
                if dry_run {
                    println!(
                        "{} would link {} → {}",
                        "→".cyan(),
                        home_target.display(),
                        repo_target.display()
                    );
                } else if let Err(e) = ensure_parent_and_link(&repo_target, &home_target) {
                    eprintln!("{} {}: {}", "✗".red().bold(), rel, e);
                    errors += 1;
                    continue;
                } else {
                    println!("{} linked {}", "✓".green().bold(), rel);
                }
                linked += 1;
            }
            FileState::Conflict | FileState::Mismatched { .. } => {
                if force {
                    if dry_run {
                        println!(
                            "{} would replace {} → {}",
                            "→".cyan(),
                            home_target.display(),
                            repo_target.display()
                        );
                    } else {
                        if let Err(e) = backup_path(&home_target) {
                            eprintln!("{} {}: backup failed: {}", "✗".red().bold(), rel, e);
                            errors += 1;
                            continue;
                        }
                        if let Err(e) = ensure_parent_and_link(&repo_target, &home_target) {
                            eprintln!("{} {}: {}", "✗".red().bold(), rel, e);
                            errors += 1;
                            continue;
                        }
                        println!("{} replaced {} (backup saved)", "✓".green().bold(), rel);
                    }
                    backed_up += 1;
                } else {
                    let detail = match state {
                        FileState::Conflict => "file exists".to_string(),
                        FileState::Mismatched { target } => {
                            format!("symlink points to {}", target.display())
                        }
                        _ => unreachable!(),
                    };
                    eprintln!(
                        "{} {}: conflict ({}). Pass --force to back up and replace.",
                        "!".yellow().bold(),
                        rel,
                        detail
                    );
                    errors += 1;
                }
            }
        }
    }

    println!();
    println!(
        "{} linked, {} skipped, {} backed up, {} issues",
        linked.to_string().green(),
        skipped.to_string().dimmed(),
        backed_up.to_string().yellow(),
        if errors > 0 {
            errors.to_string().red().to_string()
        } else {
            "0".to_string().dimmed().to_string()
        }
    );

    if errors > 0 {
        bail!("{} file(s) had issues", errors);
    }
    Ok(())
}

pub fn unlink(repo: &Path, profile: Option<&str>, dry_run: bool) -> Result<()> {
    let home = home_dir()?;
    let cfg = Config::load(repo)?;
    let profile_name = cfg.resolve_profile_name(profile).to_string();
    let files = cfg.resolve_files(&profile_name)?;

    let mut removed = 0;
    for rel in &files {
        let repo_target = repo.join(rel);
        let home_target = repo_to_home(rel, &home);
        if !home_target.is_symlink() {
            println!("{} {} (not a symlink, skipping)", "·".dimmed(), rel);
            continue;
        }
        let link_dest = fs::read_link(&home_target).ok();
        let points_at_us = link_dest
            .as_ref()
            .map(|t| same_path(t, &repo_target))
            .unwrap_or(false);
        if !points_at_us {
            println!(
                "{} {} (symlink points elsewhere, skipping)",
                "·".dimmed(),
                rel
            );
            continue;
        }
        if dry_run {
            println!("{} would remove {}", "→".cyan(), home_target.display());
            removed += 1;
            continue;
        }
        fs::remove_file(&home_target)
            .with_context(|| format!("removing symlink {}", home_target.display()))?;
        println!("{} unlinked {}", "✓".green().bold(), rel);
        removed += 1;
    }
    println!();
    println!("{} removed", removed.to_string().green());
    Ok(())
}

pub fn status(repo: &Path, profile: Option<&str>) -> Result<()> {
    let home = home_dir()?;
    let cfg = Config::load(repo)?;
    let profile_name = cfg.resolve_profile_name(profile).to_string();
    let files = cfg.resolve_files(&profile_name)?;

    if files.is_empty() {
        println!("{} profile `{}` has no files", "·".dimmed(), profile_name);
        return Ok(());
    }

    println!("{} profile: {}", "⟐".bold(), profile_name.bold());
    println!();

    let mut counts = (0usize, 0usize, 0usize, 0usize, 0usize);
    for rel in &files {
        let repo_target = repo.join(rel);
        let home_target = repo_to_home(rel, &home);
        if !repo_target.exists() {
            println!(
                "  {} {}  {}",
                "missing-repo".red().bold(),
                rel,
                "(not in repo)".dimmed()
            );
            counts.4 += 1;
            continue;
        }
        let state = classify(&repo_target, &home_target);
        match state {
            FileState::Linked => {
                println!("  {} {}", "linked".green().bold(), rel);
                counts.0 += 1;
            }
            FileState::Missing => {
                println!("  {} {}", "missing".yellow().bold(), rel);
                counts.1 += 1;
            }
            FileState::Conflict => {
                println!(
                    "  {} {}  {}",
                    "conflict".red().bold(),
                    rel,
                    "(regular file at target)".dimmed()
                );
                counts.2 += 1;
            }
            FileState::Mismatched { target } => {
                println!(
                    "  {} {}  {}",
                    "mismatched".red().bold(),
                    rel,
                    format!("(→ {})", target.display()).dimmed()
                );
                counts.3 += 1;
            }
        }
    }
    println!();
    println!(
        "{} linked · {} missing · {} conflicts · {} mismatched · {} missing-from-repo",
        counts.0.to_string().green(),
        counts.1.to_string().yellow(),
        counts.2.to_string().red(),
        counts.3.to_string().red(),
        counts.4.to_string().red()
    );
    Ok(())
}

pub fn list(repo: &Path) -> Result<()> {
    let cfg = Config::load(repo)?;
    println!("{} default: {}", "⟐".bold(), cfg.default_profile.bold());
    println!();
    for (name, profile) in &cfg.profiles {
        let marker = if name == &cfg.default_profile {
            "★".yellow()
        } else {
            " ".normal()
        };
        let parent = profile
            .inherits
            .as_deref()
            .map(|p| format!("  (inherits `{}`)", p))
            .unwrap_or_default();
        println!(
            "  {} {}{}  —  {} file(s) direct",
            marker,
            name.bold(),
            parent.dimmed(),
            profile.files.len()
        );
    }
    Ok(())
}

/// Classify what's at `home_target` relative to the expected `repo_target`.
pub fn classify(repo_target: &Path, home_target: &Path) -> FileState {
    if home_target.is_symlink() {
        match fs::read_link(home_target) {
            Ok(dest) => {
                if same_path(&dest, repo_target) {
                    FileState::Linked
                } else {
                    FileState::Mismatched { target: dest }
                }
            }
            Err(_) => FileState::Mismatched {
                target: PathBuf::new(),
            },
        }
    } else if home_target.exists() {
        FileState::Conflict
    } else {
        FileState::Missing
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    // Try canonical form if the paths exist; otherwise fall back to byte-equal.
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn ensure_parent_and_link(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    create_symlink(src, dst)
}

fn backup_path(target: &Path) -> Result<()> {
    let mut bak = target.as_os_str().to_owned();
    bak.push(".bak");
    let bak = PathBuf::from(bak);
    if bak.exists() {
        // Append timestamp to keep history
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut bak2 = target.as_os_str().to_owned();
        bak2.push(format!(".bak.{}", ts));
        let bak2 = PathBuf::from(bak2);
        fs::rename(target, &bak2).with_context(|| format!("backing up to {}", bak2.display()))?;
    } else {
        fs::rename(target, &bak).with_context(|| format!("backing up to {}", bak.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("creating symlink {} → {}", dst.display(), src.display()))
}

#[cfg(windows)]
fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
    let result = if src.is_dir() {
        std::os::windows::fs::symlink_dir(src, dst)
    } else {
        std::os::windows::fs::symlink_file(src, dst)
    };
    result.with_context(|| {
        format!(
            "creating symlink {} → {}.\n  Note: On Windows, symlink creation requires either \
             administrator privileges or Developer Mode enabled \
             (Settings → Privacy & Security → For Developers).",
            dst.display(),
            src.display()
        )
    })
}
