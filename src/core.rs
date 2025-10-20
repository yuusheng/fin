use anyhow::{Context, Result};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;

use crate::lock::{LockFile, Plugin, PluginVecExt};

pub struct Fin {
    fisher_path: PathBuf,
    fish_config_dir: PathBuf,
    fin_lock_file_path: PathBuf,
    installed_plugins: HashSet<String>,
    plugin_files: HashMap<String, Vec<PathBuf>>, // Plugin file mappings
    lock_file: LockFile,
}

const FIN_LOCK_FILENAME: &str = "fin-lock.toml";
impl Fin {
    /// Initialize a Fisher instance
    pub fn new(fisher_path: Option<PathBuf>) -> Result<Self> {
        // Get Fish configuration directory
        let fish_config_dir = Self::get_fish_config_dir()?;
        let fisher_path = fisher_path.unwrap_or_else(|| fish_config_dir.clone());
        let fin_lock_file_path = fish_config_dir.join(FIN_LOCK_FILENAME);

        // Ensure installation directories exist
        for subdir in ["functions", "conf.d", "completions"] {
            fs::create_dir_all(fisher_path.join(subdir))?;
        }

        let lock_file = LockFile::load(&fin_lock_file_path).context("fin-lock.toml has broken")?;
        // Load installed plugins
        let installed_plugins = Self::load_installed_plugins(&lock_file.plugins)?;

        Ok(Self {
            fisher_path,
            fish_config_dir,
            fin_lock_file_path,
            installed_plugins,
            plugin_files: HashMap::new(),
            lock_file,
        })
    }

    /// Get Fish configuration directory
    fn get_fish_config_dir() -> Result<PathBuf> {
        // Prefer environment variable, fallback to default path
        if let Ok(path) = env::var("__fish_config_dir") {
            Ok(PathBuf::from(path))
        } else {
            dirs::home_dir()
                .map(|p| p.join(".config/fish"))
                .context("Failed to get user home directory")
        }
    }

    /// Load installed plugins from file
    fn load_installed_plugins(lock_file_plugins: &[Plugin]) -> Result<HashSet<String>> {
        let mut plugins = HashSet::new();
        for plugin in lock_file_plugins {
            plugins.insert(plugin.name.to_string());
        }
        Ok(plugins)
    }

    /// Install plugins
    pub fn install(&mut self, plugins: &[String]) -> Result<()> {
        let plugins = parse_plugin(plugins)?;
        let plugins_to_install = plugins.diff(&self.lock_file.plugins);

        if plugins_to_install.is_empty() {
            println!("All plugins are already installed");
            return Ok(());
        }

        println!("Installing {} plugins...", plugins_to_install.len());

        let results: Vec<Result<(&str, TempDir)>> = plugins_to_install
            .par_iter()
            .map(|plugin| self.fetch_plugin(plugin))
            .collect();

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok((plugin, temp_dir)) => {
                    self.install_plugin_files(plugin, temp_dir.path())?;
                    self.installed_plugins.insert(plugin.to_string());
                    let installed_plugin = plugins.get(i).expect("Plugin missing index");
                    self.lock_file.plugins.push(installed_plugin.clone());
                    println!("Installed: {}", plugin);
                }
                Err(e) => eprintln!("Failed to install a plugin: {}", e),
            }
        }

        // self.save_plugins()?;
        self.lock_file.save(&self.fin_lock_file_path)?;
        Ok(())
    }

    /// Fetch a single plugin
    fn fetch_plugin<'a>(&self, plugin: &'a Plugin) -> Result<(&'a str, TempDir)> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        if Path::new(&plugin.name).exists() {
            Self::copy_dir(Path::new(&plugin.name), temp_path)?;
        } else {
            Self::download_repo(&plugin.source, temp_path)?;
        }

        Ok((&plugin.name, temp_dir))
    }

    /// Download and extract repository
    fn download_repo(url: &str, dest: &Path) -> Result<()> {
        println!("Downloading: {}", url);
        let curl = Command::new("curl")
            .arg("-sL")
            .arg(url)
            .stdout(Stdio::piped())
            .spawn()
            .context("Failed to spawn curl")?;

        let tar_status = Command::new("tar")
            .arg("-xz")
            .arg("-C")
            .arg(dest.as_os_str())
            .arg("--strip-components=1")
            .stdin(curl.stdout.context("Failed to get curl stdout")?)
            .status()
            .context("Failed to run tar")?;

        if !tar_status.success() {
            return Err(anyhow::anyhow!("tar command failed"));
        }

        Ok(())
    }

    /// Copy directory recursively
    fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if src_path.is_dir() {
                Self::copy_dir(&src_path, &dest_path)?;
            } else {
                fs::copy(&src_path, &dest_path)?;
            }
        }
        Ok(())
    }

    /// Install plugin files to target directory
    fn install_plugin_files(&mut self, plugin: &str, temp_dir: &Path) -> Result<()> {
        let mut installed_files = Vec::new();
        // Process component directories in the plugin
        // Process component directories in the plugin
        for component in ["functions", "conf.d", "completions"] {
            let src_dir = temp_dir.join(component);
            if src_dir.exists() {
                let dest_dir = self.fisher_path.join(component);
                for entry in fs::read_dir(src_dir)? {
                    let entry = entry?;
                    let src_path = entry.path();
                    let file_name = src_path.file_name().context("Invalid file name")?;
                    let dest_path = dest_dir.join(file_name);

                    fs::copy(&src_path, &dest_path)?;
                    installed_files.push(dest_path);
                }
            }
        }

        self.plugin_files
            .insert(plugin.to_string(), installed_files);
        Ok(())
    }

    /// Remove plugins
    pub fn remove(&mut self, plugins: &[String]) -> Result<()> {
        let mut removed_count = 0;

        for plugin in plugins {
            if self.installed_plugins.contains(plugin) {
                // Remove plugin files
                if let Some(files) = self.plugin_files.get(plugin) {
                    for file in files {
                        if file.exists() {
                            fs::remove_file(file)?;
                        }
                    }
                }

                self.installed_plugins.remove(plugin);
                self.plugin_files.remove(plugin);
                removed_count += 1;
                println!("Removed: {}", plugin);
            } else {
                eprintln!("Plugin not installed: {}", plugin);
            }
        }

        // self.save_plugins()?;
        println!("Removed {} plugins total", removed_count);
        Ok(())
    }

    /// Update plugins
    pub fn update(&mut self, plugins: &[String]) -> Result<()> {
        let plugins_to_update = if plugins.is_empty() {
            // Update all installed plugins
            self.installed_plugins.iter().cloned().collect::<Vec<_>>()
        } else {
            // Update only specified plugins
            plugins
                .iter()
                .filter(|p| self.installed_plugins.contains(p.as_str()))
                .cloned()
                .collect()
        };

        if plugins_to_update.is_empty() {
            println!("No plugins to update");
            return Ok(());
        }

        println!("Updating {} plugins...", plugins_to_update.len());

        // Update by removing then reinstalling
        self.remove(&plugins_to_update)?;
        self.install(&plugins_to_update)?;
        Ok(())
    }

    /// List installed plugins
    pub fn list(&self, pattern: Option<&str>) -> Result<()> {
        let mut plugins: Vec<&String> = self.installed_plugins.iter().collect();
        plugins.sort();

        if let Some(pattern) = pattern {
            let re = regex::Regex::new(pattern)?;
            for plugin in plugins.into_iter().filter(|p| re.is_match(p)) {
                println!("{}", plugin);
            }
        } else {
            for plugin in plugins {
                println!("{}", plugin);
            }
        }
        Ok(())
    }
}

fn parse_plugin(plugins: &[String]) -> anyhow::Result<Vec<Plugin>> {
    let mut parsed_plugins: Vec<Plugin> = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let parts: Vec<&str> = plugin.split('@').collect();
        let repo = parts[0];
        let ref_name = if parts.len() > 1 { parts[1] } else { "HEAD" };
        let repo_name = repo
            .split('/')
            .last()
            .context(format!("Repo Name invalid: {}", repo))?;

        let source = format!("https://github.com/{}/archive/{}.tar.gz", repo, ref_name);
        let plugin = Plugin {
            name: String::from(repo_name),
            source,
            ..Default::default()
        };
        parsed_plugins.push(plugin);
    }

    Ok(parsed_plugins)
}
