use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::process;

mod cli;
mod config;
mod ops;
mod paths;

use cli::{Cli, Command};

fn resolve_repo(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    if let Ok(env_repo) = std::env::var("DOTS_REPO") {
        return Ok(PathBuf::from(env_repo));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(found) = config::find_repo(&cwd) {
            return Ok(found);
        }
    }
    let home = paths::home_dir()?;
    Ok(home.join("dotfiles"))
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let repo = resolve_repo(args.repo).context("resolving dotfiles repo path")?;

    match args.command {
        Command::Init { profile } => ops::init(&repo, &profile),
        Command::Add { path, profile } => ops::add(&repo, &path, profile.as_deref()),
        Command::Link {
            profile,
            force,
            dry_run,
        } => ops::link(&repo, profile.as_deref(), force, dry_run),
        Command::Unlink { profile, dry_run } => ops::unlink(&repo, profile.as_deref(), dry_run),
        Command::Status { profile } => ops::status(&repo, profile.as_deref()),
        Command::List => ops::list(&repo),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("dots: error: {:#}", e);
        process::exit(1);
    }
}
