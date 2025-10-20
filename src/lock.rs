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

// 2. 为 Vec<Plugin> 实现这个 trait
impl PluginVecExt for Vec<Plugin> {
    // 实现 diff 方法（示例逻辑：找出 self 中存在但 other 中不存在的 Plugin）
    fn diff(&self, other: &[Plugin]) -> Vec<Plugin> {
        self.iter()
            .filter(|p| {
                let res = other
                    .iter()
                    .find(|plugin| plugin.name == p.name && plugin.commit_hash == p.commit_hash);

                res.is_none()
            }) // 需要 Plugin 实现 PartialEq
            .cloned() // 从 &Plugin 转换为 Plugin（需要 Plugin 实现 Clone，或手动 map 克隆）
            .collect()
    }
}

impl LockFile {
    pub fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
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

    pub fn save(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str = toml::to_string_pretty(&self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}
