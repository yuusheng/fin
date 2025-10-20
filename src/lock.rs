use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub source: String,
    pub commit_hash: String,
    pub branch: Option<String>, // 可选字段
    pub installed_files: Vec<String>,
    pub checksum: Option<String>, // 可选字段
}

impl LockFile {
    pub fn load(path: &str) -> Result<LockFile, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let lock: LockFile = toml::from_str(&content)?;
        Ok(lock)
    }

    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str = toml::to_string_pretty(&self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}
