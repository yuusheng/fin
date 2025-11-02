use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use std::{collections::HashSet, fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Default, Clone, Eq)]
pub struct Plugin {
    pub name: String,
    pub source: String,
    pub commit_hash: Option<String>,
    pub branch: Option<String>,
    #[serde(serialize_with = "serialize_option_hashset_sorted")]
    pub installed_files: Option<HashSet<String>>,
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
    }
}

pub trait PluginVecExt {
    fn diff_mut(&mut self, other: &HashSet<Plugin>);
}

impl PluginVecExt for HashSet<Plugin> {
    fn diff_mut(&mut self, other: &HashSet<Plugin>) {
        let other_plugins: HashSet<_> = other.iter().collect();
        self.retain(|p| !other_plugins.contains(p));
    }
}

impl From<&str> for Plugin {
    fn from(s: &str) -> Self {
        let mut parts = s.split('@');
        let repo = parts.next().unwrap_or("");
        let ref_name = parts.next().unwrap_or("HEAD");

        let source: String = format!("https://github.com/{repo}/archive/{ref_name}.tar.gz");

        Self {
            name: String::from(repo),
            source,
            ..Default::default()
        }
    }
}

fn serialize_option_hashset_sorted<S>(
    opt_hashset: &Option<HashSet<String>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match opt_hashset {
        Some(hashset) => {
            let mut vec: Vec<&String> = hashset.iter().collect();
            vec.sort();
            Some(vec).serialize(serializer)
        }
        None => None::<Vec<String>>.serialize(serializer),
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockFile {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub plugins: HashSet<Plugin>,
}

impl LockFile {
    pub fn load(path: &PathBuf) -> Self {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(lock) = toml::from_str(&content) {
                return lock;
            }
            println!("Lock file has been broken, rebuilding lock file.")
        }

        // First install
        // Return a default lock file if the file does not exist
        LockFile {
            version: String::from("1.0"),
            generated_at: Utc::now(),
            plugins: HashSet::new(),
        }
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let toml_str = toml::to_string_pretty(&self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use crate::lock::LockFile;

    fn create_valid_lockfile_content() -> (LockFile, String) {
        let lock_file = LockFile {
            version: String::from("1.0"),
            generated_at: chrono::Utc::now(), // Note: actual generated_at will differ slightly
            plugins: std::collections::HashSet::new(),
        };

        let toml_content = toml::to_string(&lock_file).unwrap();

        (lock_file, toml_content)
    }

    #[test]
    fn lock_file_load() {
        let lock_file = LockFile::load(&PathBuf::new());
        assert_eq!(lock_file.version, "1.0");
        assert!(lock_file.plugins.is_empty());
    }

    #[test]
    fn test_load_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_lock.toml");

        let (_, valid_content) = create_valid_lockfile_content();
        fs::write(&file_path, &valid_content).unwrap();

        let loaded_lock = LockFile::load(&file_path);

        assert_eq!(loaded_lock.version, "1.0");
        assert!(loaded_lock.plugins.is_empty());
        assert!(loaded_lock.generated_at > chrono::Utc::now() - chrono::Duration::seconds(5));
    }

    #[test]
    fn test_invalid_toml_returns_default() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("broken_lock.toml");

        fs::write(&file_path, "not a valid toml\nkey = {").unwrap();

        let loaded_lock = LockFile::load(&file_path);

        assert_eq!(loaded_lock.version, "1.0");
        assert!(loaded_lock.plugins.is_empty());
    }
}
