use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// A tiny, fast dotfile manager with per-machine profiles.
#[derive(Parser, Debug)]
#[command(name = "dots", version, about, long_about = None)]
pub struct Cli {
    /// Path to the dotfiles repository. Defaults to $DOTS_REPO or $HOME/dotfiles.
    #[arg(long, global = true, env = "DOTS_REPO")]
    pub repo: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a new dots config in the current (or --repo) directory.
    Init {
        /// Name of the initial profile.
        #[arg(long, default_value = "default")]
        profile: String,
    },

    /// Move a file into the dotfiles repo and symlink it back into place.
    Add {
        /// The dotfile to track (e.g. ~/.bashrc).
        path: PathBuf,

        /// Profile to add it to. Defaults to the config's default_profile.
        #[arg(long)]
        profile: Option<String>,
    },

    /// Create symlinks from the home directory to the dotfiles repo.
    Link {
        /// Profile to install. Defaults to the config's default_profile.
        #[arg(long)]
        profile: Option<String>,

        /// Overwrite existing files/symlinks instead of backing them up.
        #[arg(long)]
        force: bool,

        /// Show what would happen without making changes.
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove symlinks created by `dots link`.
    Unlink {
        /// Profile to remove. Defaults to the config's default_profile.
        #[arg(long)]
        profile: Option<String>,

        /// Show what would happen without making changes.
        #[arg(long)]
        dry_run: bool,
    },

    /// Show the state of each tracked file: linked, missing, mismatched, or conflict.
    Status {
        /// Profile to inspect. Defaults to the config's default_profile.
        #[arg(long)]
        profile: Option<String>,
    },

    /// List all profiles defined in the config.
    List,
}
