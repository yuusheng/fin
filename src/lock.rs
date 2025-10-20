use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Plugin {
    pub name: String,
    pub source: String,
    pub commit_hash: String,
    pub branch: Option<String>,
    pub installed_files: Option<Vec<String>>,
    pub checksum: Option<String>,
}

pub trait PluginVecExt {
    fn diff(&self, other: &[Plugin]) -> Vec<Plugin>;
}

impl PluginVecExt for Vec<Plugin> {
    fn diff(&self, other: &[Plugin]) -> Vec<Plugin> {
        self.iter()
            .filter(|p| {
                let res = other
                    .iter()
                    .find(|plugin| plugin.name == p.name && plugin.commit_hash == p.commit_hash);

                res.is_none()
            })
            .cloned()
            .collect()
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
