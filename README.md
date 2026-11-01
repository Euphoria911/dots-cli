# dots

A tiny, fast dotfile manager with **per-machine profiles**. One binary. One config file. Symlinks on, symlinks off.

```
$ dots add ~/.bashrc
✓ tracked `bashrc` under profile `default`

$ dots list
⟐ default: default
  ★ default                 — 1 file(s) direct
    laptop (inherits `default`)  — 2 file(s) direct
    server                   — 0 file(s) direct

$ dots status
⟐ profile: default
  linked  bashrc
1 linked · 0 missing · 0 conflicts · 0 mismatched · 0 missing-from-repo
```

## Why

Most dotfile managers either lean on git submodule gymnastics or push you into a bespoke shell framework. `dots` is just: your home directory, a repo, and symlinks — with a `dots.toml` describing which files belong to which machine.

- **Zero magic.** The repo mirrors your home directory layout, minus the leading `.` on each path.
- **Per-machine profiles with inheritance.** `laptop` can inherit `default` and add its own files.
- **Idempotent.** Running `dots link` twice is safe. Already-linked files are skipped.
- **Safe by default.** Pre-existing files are backed up to `.bak` (only with `--force`); otherwise `dots` refuses and tells you why.
- **One static binary.** Rust. No runtime dependencies.

## Install

```bash
# From source
cargo install --git https://github.com/Euphoria911/dots-cli

# Or clone and build
git clone https://github.com/Euphoria911/dots-cli
cd dots-cli && cargo install --path .
```

Prebuilt binaries for Linux / macOS / Windows ship with each GitHub release.

## Quickstart

```bash
# 1. Create a dotfiles repo
mkdir ~/dotfiles && cd ~/dotfiles
dots init

# 2. Start tracking files
dots add ~/.bashrc
dots add ~/.config/git/config

# 3. On a new machine
git clone <your-dotfiles-repo> ~/dotfiles
dots link
```

## Commands

| Command | What it does |
| --- | --- |
| `dots init` | Create a new `dots.toml` with a default profile. |
| `dots add <path>` | Move the file into the repo, symlink it back, track it in the config. |
| `dots link` | Install symlinks for the current profile. `--dry-run`, `--force`. |
| `dots unlink` | Remove symlinks created by `dots link` (repo files are untouched). |
| `dots status` | Per-file state: `linked`, `missing`, `conflict`, `mismatched`. |
| `dots list` | Show all profiles and their file counts. |

All commands accept `--profile <name>` to target a specific profile, and all commands accept `--repo <path>` (or the `DOTS_REPO` env var, or auto-discovery if you're `cd`'d inside a repo).

### Environment variables

| Variable | Purpose |
| --- | --- |
| `DOTS_REPO` | Path to the dotfiles repo. Same as `--repo`. |
| `DOTS_HOME` | Override the home directory. Rarely useful outside tests and unusual setups where the OS-reported home differs from where you keep dotfiles. |

## Profiles

Profiles live in `dots.toml`. Each profile has a `files` list, and optionally `inherits` another profile.

```toml
default_profile = "laptop"

[profiles.default]
files = ["bashrc", "vimrc"]

[profiles.laptop]
inherits = "default"
files = ["config/alacritty/alacritty.toml", "ssh/config"]

[profiles.server]
files = ["bashrc", "profile"]
```

Running `dots link` on the laptop installs `bashrc`, `vimrc`, `alacritty.toml`, and `ssh/config`. On the server, only `bashrc` and `profile`.

## Path mapping

The repo mirrors your home directory **with the leading dot stripped from the first component**:

| Home path | Stored in repo as |
| --- | --- |
| `~/.bashrc` | `bashrc` |
| `~/.vimrc` | `vimrc` |
| `~/.config/git/config` | `config/git/config` |
| `~/.ssh/config` | `ssh/config` |

Only the first component is stripped. `~/.config/git/config` stays nested exactly like that in the repo.

## Caveats

- **Windows** requires either administrator privileges or Developer Mode (Settings → Privacy & Security → For Developers) to create symlinks. The error message tells you so.
- **`dots add`** moves the file and then symlinks it — the file physically relocates. Commit the repo immediately after.
- Only the first path component is treated as the "dot" prefix. Nested files like `~/.config/foo/bar` work; truly-exotic layouts outside `$HOME` do not.

## Development

```bash
cargo check
cargo test                    # unit + integration
cargo build --release         # ships target/release/dots
cargo clippy --all-targets    # lints
```

## License

MIT
