use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILENAME: &str = "dots.toml";

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default = "default_profile_name")]
    pub default_profile: String,

    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Profile {
    #[serde(default)]
    pub inherits: Option<String>,

    #[serde(default)]
    pub files: Vec<String>,
}

fn default_profile_name() -> String {
    "default".to_string()
}

impl Config {
    pub fn new(initial_profile: &str) -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            initial_profile.to_string(),
            Profile {
                inherits: None,
                files: Vec::new(),
            },
        );
        Config {
            default_profile: initial_profile.to_string(),
            profiles,
        }
    }

    pub fn load(repo: &Path) -> Result<Self> {
        let path = repo.join(CONFIG_FILENAME);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        let cfg: Config = toml::from_str(&text)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, repo: &Path) -> Result<()> {
        let path = repo.join(CONFIG_FILENAME);
        let text = toml::to_string_pretty(self).context("serializing config")?;
        fs::write(&path, text).with_context(|| format!("writing config to {}", path.display()))?;
        Ok(())
    }

    /// Returns the fully resolved file list for a profile, with inherited files first
    /// (deduplicated, order preserved from the inheritance chain root-to-leaf).
    pub fn resolve_files(&self, profile_name: &str) -> Result<Vec<String>> {
        let mut chain: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut current = profile_name;

        loop {
            if !seen.insert(current) {
                bail!("profile inheritance cycle detected involving `{}`", current);
            }
            let profile = self
                .profiles
                .get(current)
                .ok_or_else(|| anyhow!("profile `{}` not found in config", current))?;
            chain.push(current);
            match &profile.inherits {
                Some(parent) => current = parent.as_str(),
                None => break,
            }
        }

        // Walk from root (ancestor) down to the requested profile, so overrides and
        // appends from more-specific profiles come last.
        let mut files: Vec<String> = Vec::new();
        let mut seen_files: HashSet<String> = HashSet::new();
        for name in chain.iter().rev() {
            let profile = &self.profiles[*name];
            for f in &profile.files {
                if seen_files.insert(f.clone()) {
                    files.push(f.clone());
                }
            }
        }
        Ok(files)
    }

    pub fn resolve_profile_name<'a>(&'a self, requested: Option<&'a str>) -> &'a str {
        requested.unwrap_or(&self.default_profile)
    }
}

/// Walk up from `start` looking for a dots.toml. Returns the directory containing it.
pub fn find_repo(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(CONFIG_FILENAME).is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_from(toml_str: &str) -> Config {
        toml::from_str(toml_str).expect("parse")
    }

    #[test]
    fn resolves_a_single_profile() {
        let cfg = cfg_from(
            r#"
            default_profile = "default"
            [profiles.default]
            files = ["bashrc", "vimrc"]
        "#,
        );
        let files = cfg.resolve_files("default").unwrap();
        assert_eq!(files, vec!["bashrc", "vimrc"]);
    }

    #[test]
    fn inheritance_prepends_parent_files() {
        let cfg = cfg_from(
            r#"
            default_profile = "laptop"
            [profiles.default]
            files = ["bashrc"]
            [profiles.laptop]
            inherits = "default"
            files = ["ssh/config"]
        "#,
        );
        let files = cfg.resolve_files("laptop").unwrap();
        assert_eq!(files, vec!["bashrc", "ssh/config"]);
    }

    #[test]
    fn inheritance_deduplicates_overlapping_entries() {
        let cfg = cfg_from(
            r#"
            default_profile = "laptop"
            [profiles.base]
            files = ["bashrc", "vimrc"]
            [profiles.laptop]
            inherits = "base"
            files = ["vimrc", "alacritty.toml"]
        "#,
        );
        let files = cfg.resolve_files("laptop").unwrap();
        assert_eq!(files, vec!["bashrc", "vimrc", "alacritty.toml"]);
    }

    #[test]
    fn cycle_detection_errors_cleanly() {
        let cfg = cfg_from(
            r#"
            default_profile = "a"
            [profiles.a]
            inherits = "b"
            [profiles.b]
            inherits = "a"
        "#,
        );
        let err = cfg.resolve_files("a").unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn missing_profile_errors() {
        let cfg = cfg_from(
            r#"
            default_profile = "default"
            [profiles.default]
            files = []
        "#,
        );
        let err = cfg.resolve_files("ghost").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn missing_parent_errors() {
        let cfg = cfg_from(
            r#"
            default_profile = "child"
            [profiles.child]
            inherits = "nonexistent"
        "#,
        );
        let err = cfg.resolve_files("child").unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn round_trips_through_toml() {
        let mut cfg = Config::new("default");
        cfg.profiles.insert(
            "laptop".to_string(),
            Profile {
                inherits: Some("default".to_string()),
                files: vec!["ssh/config".to_string()],
            },
        );
        let text = toml::to_string_pretty(&cfg).unwrap();
        let round: Config = toml::from_str(&text).unwrap();
        assert_eq!(round.default_profile, "default");
        assert_eq!(
            round.profiles["laptop"].inherits.as_deref(),
            Some("default")
        );
        assert_eq!(round.profiles["laptop"].files, vec!["ssh/config"]);
    }
}
