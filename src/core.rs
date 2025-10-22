use anyhow::{Context, Result};
use rayon::prelude::*;
use std::{
    collections::HashSet,
    env,
    fs::{self},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;

use crate::lock::{LockFile, Plugin, PluginVecExt};

const PLUGIN_SUBDIRS: &[&str] = &["functions", "conf.d", "completions"];
const FIN_LOCK_FILENAME: &str = "fin-lock.toml";

#[allow(dead_code)]
pub struct Fin {
    fisher_path: PathBuf,
    fish_config_dir: PathBuf,
    fin_lock_file_path: PathBuf,
    lock_file: LockFile,
}

impl Fin {
    /// Initialize a Fisher instance
    pub fn new(fisher_path: Option<PathBuf>) -> Result<Self> {
        // Get Fish configuration directory
        let fish_config_dir = Self::get_fish_config_dir()?;
        let fisher_path = fisher_path.unwrap_or_else(|| fish_config_dir.clone());
        let fin_lock_file_path = fish_config_dir.join(FIN_LOCK_FILENAME);

        // Ensure installation directories exist
        for subdir in PLUGIN_SUBDIRS {
            fs::create_dir_all(fisher_path.join(subdir))?;
        }

        let lock_file = LockFile::load(&fin_lock_file_path).context("fin-lock.toml has broken")?;

        Ok(Self {
            fisher_path,
            fish_config_dir,
            fin_lock_file_path,
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

    /// Install plugins
    pub fn install<T: AsRef<str>>(&mut self, plugins: Option<Vec<T>>, force: bool) -> Result<()> {
        let plugins_to_install = self.get_plugins_to_install(plugins, force)?;

        if plugins_to_install.is_empty() {
            println!("All plugins are already installed");
            return Ok(());
        }

        println!("Installing {} plugins...", plugins_to_install.len());

        let new_plugins: Vec<_> = plugins_to_install
            .into_par_iter()
            .filter_map(|plugin| self.install_plugin(plugin).ok())
            .collect();

        for plugin in &new_plugins {
            println!("Installed: {}", &plugin.name);
        }

        self.lock_file.plugins.extend(new_plugins);
        self.lock_file.save(&self.fin_lock_file_path)?;
        Ok(())
    }

    fn get_plugins_to_install<T: AsRef<str>>(
        &self,
        plugins: Option<Vec<T>>,
        force: bool,
    ) -> Result<Vec<Plugin>> {
        let mut plugins_to_install = if let Some(plugins) = plugins {
            parse_plugin(&plugins)?
        } else {
            self.lock_file.plugins.clone()
        };

        if !force {
            plugins_to_install.diff_mut(&self.lock_file.plugins);
        }

        Ok(plugins_to_install)
    }

    fn install_plugin(&self, mut plugin: Plugin) -> Result<Plugin> {
        self.fetch_plugin(&plugin).and_then(|temp_dir| {
            Self::do_install_plugin_files(&self.fisher_path, temp_dir.path()).map(
                |installed_files| {
                    if !installed_files.is_empty() {
                        plugin.installed_files = Some(
                            installed_files
                                .into_iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect(),
                        );
                    }
                    plugin
                },
            )
        })
    }

    /// Fetch a single plugin
    fn fetch_plugin(&self, plugin: &Plugin) -> Result<TempDir> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        if Path::new(&plugin.name).exists() {
            Self::copy_dir(Path::new(&plugin.name), temp_path)?;
        } else {
            Self::download_repo(&plugin.source, temp_path)?;
        }

        Ok(temp_dir)
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

    fn do_install_plugin_files(fisher_path: &Path, temp_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut installed_files = Vec::new();
        for component in PLUGIN_SUBDIRS {
            let src_dir = temp_dir.join(component);
            if src_dir.exists() {
                let dest_dir = fisher_path.join(component);
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
        Ok(installed_files)
    }

    fn plugins(&self) -> impl Iterator<Item = &str> {
        self.lock_file.plugins.iter().map(|p| p.name.as_str())
    }

    /// Remove plugins
    pub fn remove(&mut self, plugins: &[String]) -> Result<()> {
        let plugins_to_remove: HashSet<_> = plugins.iter().collect();
        let mut removed_count = 0;

        self.lock_file.plugins.retain(|plugin| {
            if plugins_to_remove.contains(&plugin.name) {
                if let Some(files) = &plugin.installed_files {
                    for file in files {
                        if fs::remove_file(file).is_err() {
                            // Ignore errors for files that don't exist
                        }
                    }
                }
                removed_count += 1;
                println!("Removed: {}", &plugin.name);
                false
            } else {
                true
            }
        });

        println!("Removed {} plugins total", removed_count);
        self.lock_file.save(&self.fin_lock_file_path)?;
        Ok(())
    }

    /// Update plugins
    pub fn update(&mut self, plugins: &[String]) -> Result<()> {
        if plugins.is_empty() {
            // Update all installed plugins
            let plugins_to_update = self
                .lock_file
                .plugins
                .iter()
                .map(|p| p.name.to_string())
                .collect::<Vec<String>>();
            println!("Updating {} plugins...", plugins_to_update.len());
            return self.install(Some(plugins_to_update), true);
        }

        let plugins_to_update: Vec<String> = {
            let installed_plugins: HashSet<_> = self.plugins().collect();
            // Update only specified plugins
            plugins
                .iter()
                .filter(|p| installed_plugins.contains(p.as_str()))
                .cloned()
                .collect()
        };

        if plugins_to_update.is_empty() {
            println!("No plugins to update");
            return Ok(());
        }

        println!("Updating {} plugins...", plugins_to_update.len());

        // Update by removing then reinstalling
        self.install(Some(plugins_to_update), true)
    }

    /// List installed plugins
    pub fn list(&self) -> Result<()> {
        for plugin in self.plugins() {
            println!("{}", plugin);
        }

        Ok(())
    }
}

fn parse_plugin<T: AsRef<str>>(plugins: &[T]) -> anyhow::Result<Vec<Plugin>> {
    plugins
        .iter()
        .map(|plugin_str| {
            let mut parts = plugin_str.as_ref().split('@');
            let repo = parts.next().unwrap_or("");
            let ref_name = parts.next().unwrap_or("HEAD");

            let repo_name: &str = repo
                .split('/')
                .next_back()
                .with_context(|| format!("Invalid repository name: {}", repo))?;

            let source: String = format!("https://github.com/{}/archive/{}.tar.gz", repo, ref_name);
            Ok(Plugin {
                name: String::from(repo_name),
                source,
                ..Default::default()
            })
        })
        .collect()
}
