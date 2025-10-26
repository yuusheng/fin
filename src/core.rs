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
    fin_path: PathBuf,
    fish_config_dir: PathBuf,
    fin_lock_file_path: PathBuf,
    lock_file: LockFile,
}

impl Fin {
    /// Initialize a Fin instance
    pub fn new(fin_path: Option<PathBuf>) -> Result<Self> {
        // Get Fish configuration directory
        let fish_config_dir = Self::get_fish_config_dir()?;
        let fin_path = fin_path.unwrap_or_else(|| fish_config_dir.clone());
        let fin_lock_file_path = fish_config_dir.join(FIN_LOCK_FILENAME);

        // Ensure installation directories exist
        for subdir in PLUGIN_SUBDIRS {
            fs::create_dir_all(fin_path.join(subdir))?;
        }

        let lock_file = LockFile::load(&fin_lock_file_path).context("fin-lock.toml has broken")?;

        Ok(Self {
            fin_path,
            fish_config_dir,
            fin_lock_file_path,
            lock_file,
        })
    }

    /// Install plugins
    pub fn install(&mut self, plugins: Option<Vec<String>>, force: bool) -> Result<()> {
        let plugins_to_install = self.get_plugins_to_install(plugins, force);

        if plugins_to_install.is_empty() {
            println!("All plugins are already installed");
            return Ok(());
        }

        println!("Installing {} plugins...", plugins_to_install.len());

        let installed_plugins: Vec<_> = plugins_to_install
            .into_par_iter()
            .filter_map(|plugin| self.install_plugin(plugin).ok())
            .collect();

        for plugin in &installed_plugins {
            println!("Installed: {}", &plugin.name);
        }

        self.lock_file.plugins.extend(installed_plugins);
        self.lock_file.save(&self.fin_lock_file_path)?;
        Ok(())
    }

    /// Remove plugins
    pub fn remove(&mut self, plugins: &[String]) -> Result<()> {
        let plugins_to_remove: HashSet<_> = plugins.iter().collect();
        let mut removed_count = 0;

        self.lock_file.plugins.retain(|plugin| {
            if !plugins_to_remove.contains(&plugin.name) {
                return true;
            }

            if let Some(files) = &plugin.installed_files {
                for file in files {
                    let plugin_path = &self.fish_config_dir.join(file);
                    // Ignore error for now
                    let _ = fs::remove_file(plugin_path).map_err(|_| {
                        println!("File not found: {}", file);
                    });
                }
            }
            removed_count += 1;
            println!("Removed: {}", &plugin.name);
            false
        });

        println!("Removed {removed_count} plugins total");
        self.lock_file.save(&self.fin_lock_file_path)?;
        Ok(())
    }

    /// Update plugins
    pub fn update(&mut self, plugins: &[String]) -> Result<()> {
        let installed_plugins: std::collections::HashSet<String> =
            self.plugins().map(|p| p.to_string()).collect();

        let plugins_to_update: Vec<String> = if plugins.is_empty() {
            installed_plugins.into_iter().collect()
        } else {
            plugins
                .iter()
                .filter(|&p| installed_plugins.contains(p))
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
            println!("{plugin}");
        }

        Ok(())
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

    fn get_plugins_to_install(&self, plugins: Option<Vec<String>>, force: bool) -> Vec<Plugin> {
        let mut plugins_to_install = if let Some(plugins) = plugins {
            plugins.iter().map(|p| Plugin::from(p.as_str())).collect()
        } else {
            self.lock_file.plugins.clone()
        };

        if !force {
            plugins_to_install.diff_mut(&self.lock_file.plugins);
        }

        plugins_to_install
    }

    fn install_plugin(&self, mut plugin: Plugin) -> Result<Plugin> {
        let temp_dir = self.fetch_plugin(&plugin)?;
        let installed_files = self.do_install_plugin_files(temp_dir.path())?;

        if !installed_files.is_empty() {
            plugin.installed_files = Some(
                installed_files
                    .into_iter()
                    .map(|p| {
                        p.strip_prefix(&self.fish_config_dir)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect::<std::collections::HashSet<String>>(),
            );
        }

        Ok(plugin)
    }

    /// Fetch a single plugin
    fn fetch_plugin(&self, plugin: &Plugin) -> Result<TempDir> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        if Path::new(&plugin.name).exists() {
            Self::copy_dir(Path::new(&plugin.name), temp_path)?;
        } else {
            download_repo(&plugin.source, temp_path)?;
        }

        Ok(temp_dir)
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

    fn do_install_plugin_files(&self, temp_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut installed_files = Vec::new();
        for component in PLUGIN_SUBDIRS {
            let src_dir = temp_dir.join(component);
            if src_dir.exists() {
                let dest_dir = self.fin_path.join(component);
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
}

fn download_repo(url: &str, dest: &Path) -> Result<()> {
    println!("Downloading: {url}");
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
