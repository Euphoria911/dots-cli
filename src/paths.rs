use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

/// Returns the user's home directory, or an error if it can't be found.
///
/// Honors `DOTS_HOME` as an escape hatch (useful for tests and unusual setups
/// where the OS-reported home differs from where the user wants dotfiles).
pub fn home_dir() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("DOTS_HOME") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    dirs::home_dir().context("could not determine home directory")
}

/// Convert a path like `~/.config/git/config` or an absolute path under the home dir
/// into the repo-relative form `config/git/config` (leading dot stripped).
///
/// Rules:
///   * The input must resolve to something under $HOME.
///   * The first component after $HOME must start with a leading `.` — that's what makes
///     it a dotfile. We strip the leading dot in the stored form.
pub fn home_to_repo(input: &Path, home: &Path) -> Result<String> {
    let expanded = expand_tilde(input, home);
    let abs = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .context("getting current directory")?
            .join(expanded)
    };

    let abs = normalize(&abs);
    let home = normalize(home);

    let rel = abs.strip_prefix(&home).with_context(|| {
        format!(
            "path `{}` is not under home directory `{}`",
            abs.display(),
            home.display()
        )
    })?;

    let mut components = rel.components();
    let first = components
        .next()
        .context("empty path after stripping home directory")?;
    let first = match first {
        Component::Normal(s) => s.to_string_lossy().into_owned(),
        _ => bail!("unexpected path component after home: {:?}", first),
    };

    if !first.starts_with('.') {
        bail!("`{}` is not a dotfile (no leading `.`)", first);
    }
    let stripped_first = first.trim_start_matches('.').to_string();
    if stripped_first.is_empty() {
        bail!("`{}` has no name after stripping leading dots", first);
    }

    let mut out = PathBuf::from(stripped_first);
    for c in components {
        match c {
            Component::Normal(s) => out.push(s),
            _ => bail!("unsupported path component: {:?}", c),
        }
    }

    // Normalize to forward-slash form for cross-platform storage in dots.toml.
    Ok(out
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

/// Convert a repo-relative form like `config/git/config` back to the absolute
/// home path `$HOME/.config/git/config`.
pub fn repo_to_home(rel: &str, home: &Path) -> PathBuf {
    let mut parts = rel.split('/').filter(|p| !p.is_empty());
    let first = match parts.next() {
        Some(p) => format!(".{}", p),
        None => return home.to_path_buf(),
    };
    let mut out = home.to_path_buf();
    out.push(first);
    for p in parts {
        out.push(p);
    }
    out
}

fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        home.join(rest)
    } else if s == "~" {
        home.to_path_buf()
    } else {
        path.to_path_buf()
    }
}

/// Resolve `.` and `..` components without touching the filesystem.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn home() -> PathBuf {
        PathBuf::from("/home/u")
    }

    #[cfg(windows)]
    fn home() -> PathBuf {
        PathBuf::from(r"C:\Users\u")
    }

    #[test]
    fn home_to_repo_strips_leading_dot() {
        let h = home();
        let input = h.join(".bashrc");
        assert_eq!(home_to_repo(&input, &h).unwrap(), "bashrc");
    }

    #[test]
    fn home_to_repo_handles_nested_paths() {
        let h = home();
        let input = h.join(".config").join("git").join("config");
        assert_eq!(home_to_repo(&input, &h).unwrap(), "config/git/config");
    }

    #[test]
    fn home_to_repo_rejects_non_dotfile() {
        let h = home();
        let input = h.join("Documents");
        assert!(home_to_repo(&input, &h).is_err());
    }

    #[test]
    fn home_to_repo_rejects_outside_home() {
        let h = home();
        #[cfg(unix)]
        let outside = PathBuf::from("/etc/hosts");
        #[cfg(windows)]
        let outside = PathBuf::from(r"C:\Windows\System32");
        assert!(home_to_repo(&outside, &h).is_err());
    }

    #[test]
    fn repo_to_home_roundtrips() {
        let h = home();
        let abs = repo_to_home("config/git/config", &h);
        assert_eq!(home_to_repo(&abs, &h).unwrap(), "config/git/config");
    }

    #[test]
    fn repo_to_home_single_component() {
        let h = home();
        let abs = repo_to_home("bashrc", &h);
        assert_eq!(abs, h.join(".bashrc"));
    }
}
