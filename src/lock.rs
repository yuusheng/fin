use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Eq)]
pub struct Plugin {
    pub name: String,
    pub source: String,
    pub commit_hash: String,
    pub branch: Option<String>,
    pub installed_files: Option<Vec<String>>,
    pub checksum: Option<String>,
}

impl PartialEq for Plugin {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.commit_hash == other.commit_hash
    }
}

impl std::hash::Hash for Plugin {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.commit_hash.hash(state);
    }
}

pub trait PluginVecExt {
    fn diff(&self, other: &[Plugin]) -> Vec<Plugin>;
    fn diff_mut(&mut self, other: &[Plugin]);
}

impl PluginVecExt for Vec<Plugin> {
    fn diff(&self, other: &[Plugin]) -> Vec<Plugin> {
        let other_plugins: HashSet<_> = other.iter().collect();
        self.iter()
            .filter(|p| !other_plugins.contains(p))
            .cloned()
            .collect()
    }

    fn diff_mut(&mut self, other: &[Plugin]) {
        let other_plugins: HashSet<_> = other.iter().collect();
        self.retain(|p| !other_plugins.contains(p));
    }
}

impl LockFile {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if let Ok(content) = fs::read_to_string(path) {
            let lock: LockFile = toml::from_str(&content)?;
            return Ok(lock);
        }

        // First install
        // Return a default lock file if the file does not exist
        Ok(LockFile {
            version: String::from("1.0"),
            generated_at: Utc::now(),
            plugins: Vec::new(),
        })
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let toml_str = toml::to_string_pretty(&self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}
