use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::{self, BufRead, BufWriter, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

const FISHER_VERSION: &str = "4.4.5";
const FISH_PLUGINS_FILENAME: &str = "fish_plugins";

#[derive(Debug, Parser)]
#[clap(name = "fisher", version = FISHER_VERSION, about = "A plugin manager for Fish")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// Plugin installation path (default: Fish config directory)
    #[clap(long)]
    fisher_path: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Install plugins
    Install {
        /// Plugins to install (repository URLs or local paths)
        plugins: Vec<String>,
    },

    /// Remove installed plugins
    Remove {
        /// Plugins to remove
        plugins: Vec<String>,
    },

    /// Update installed plugins
    Update {
        /// Plugins to update (leave empty to update all)
        plugins: Vec<String>,
    },

    /// List installed plugins
    List {
        /// Filter plugins by regex pattern
        pattern: Option<String>,
    },
}

struct Fisher {
    fisher_path: PathBuf,
    fish_config_dir: PathBuf,
    fish_plugins_path: PathBuf,
    installed_plugins: HashSet<String>,
    plugin_files: HashMap<String, Vec<PathBuf>>, // Plugin file mappings
}

impl Fisher {
    /// Initialize a Fisher instance
    fn new(fisher_path: Option<PathBuf>) -> Result<Self> {
        // Get Fish configuration directory
        let fish_config_dir = Self::get_fish_config_dir()?;
        let fisher_path = fisher_path.unwrap_or_else(|| fish_config_dir.clone());
        let fish_plugins_path = fish_config_dir.join(FISH_PLUGINS_FILENAME);

        // Ensure installation directories exist
        fs::create_dir_all(&fisher_path)?;
        for subdir in ["functions", "conf.d", "completions", "themes"] {
            fs::create_dir_all(fisher_path.join(subdir))?;
        }

        // Load installed plugins
        let installed_plugins = Self::load_installed_plugins(&fish_plugins_path)?;

        Ok(Self {
            fisher_path,
            fish_config_dir,
            fish_plugins_path,
            installed_plugins,
            plugin_files: HashMap::new(),
        })
    }

    /// Get Fish configuration directory
    fn get_fish_config_dir() -> Result<PathBuf> {
        // Prefer environment variable, fallback to default path
        if let Ok(path) = env::var("__fish_config_dir") {
            Ok(PathBuf::from(path))
        } else {
            Ok(dirs::home_dir()
                .context("Failed to get user home directory")?
                .join(".config/fish"))
        }
    }

    /// Load installed plugins from file
    fn load_installed_plugins(plugins_path: &Path) -> Result<HashSet<String>> {
        let mut plugins = HashSet::new();
        if plugins_path.exists() {
            let file = File::open(plugins_path)?;
            for line in io::BufReader::new(file).lines() {
                let line = line?;
                let plugin = line.trim();
                if !plugin.is_empty() {
                    plugins.insert(plugin.to_string());
                }
            }
        }
        Ok(plugins)
    }

    /// Save plugin list to file
    fn save_plugins(&self) -> Result<()> {
        let file = File::create(&self.fish_plugins_path)?;
        let mut writer = BufWriter::new(file);
        for plugin in &self.installed_plugins {
            writeln!(writer, "{}", plugin)?;
        }
        Ok(())
    }

    /// Install plugins
    fn install(&mut self, plugins: &[String]) -> Result<()> {
        let plugins_to_install: Vec<String> = plugins
            .iter()
            .filter(|p| !self.installed_plugins.contains(p.as_str()))
            .cloned()
            .collect();

        if plugins_to_install.is_empty() {
            println!("All plugins are already installed");
            return Ok(());
        }

        println!("Installing {} plugins...", plugins_to_install.len());
        // Download plugins in parallel
        let rt = Runtime::new()?;
        let fetched_plugins = rt.block_on(self.fetch_plugins(&plugins_to_install))?;

        // Install downloaded plugins
        for (plugin, temp_dir) in fetched_plugins {
            self.install_plugin_files(&plugin, &temp_dir)?;
            self.installed_plugins.insert(plugin.clone());
            println!("Installed: {}", plugin);
        }

        self.save_plugins()?;
        Ok(())
    }

    /// Fetch plugins (supports local paths and remote repositories)
    async fn fetch_plugins(&self, plugins: &[String]) -> Result<Vec<(String, TempDir)>> {
        let mut results = Vec::new();
        for plugin in plugins {
            let temp_dir = TempDir::new()?;
            let temp_path = temp_dir.path().to_path_buf();

            if Path::new(plugin).exists() {
                // Local plugin - copy directly
                Self::copy_local_plugin(plugin, &temp_path)?;
                results.push((plugin.clone(), temp_dir));
            } else {
                // Remote plugin - download from Git repository
                if let Ok(url) = Self::parse_repo_url(plugin) {
                    if Self::download_repo(&url, &temp_path).await? {
                        results.push((plugin.clone(), temp_dir));
                    } else {
                        eprintln!("Failed to download plugin: {}", plugin);
                    }
                } else {
                    eprintln!("Invalid plugin format: {}", plugin);
                }
            }
        }
        Ok(results)
    }

    /// Parse repository URL (supports GitHub)
    fn parse_repo_url(plugin: &str) -> Result<String> {
        let parts: Vec<&str> = plugin.split('@').collect();
        let repo = parts[0];
        let ref_name = if parts.len() > 1 { parts[1] } else { "HEAD" };

        // GitHub repository
        if repo.contains('/') {
            Ok(format!(
                "https://github.com/{}/archive/{}.tar.gz",
                repo, ref_name
            ))
        } else {
            Err(anyhow::anyhow!("Invalid repository format: {}", plugin))
        }
    }

    /// Download and extract repository
    async fn download_repo(url: &str, dest: &Path) -> Result<bool> {
        println!("Downloading: {}", url);
        // Use curl to download and extract
        let status = Command::new("curl")
            .arg("-sL")
            .arg(url)
            .arg("-o")
            .arg("-")
            .pipe(
                Command::new("tar")
                    .arg("-xz")
                    .arg("-C")
                    .arg(dest)
                    .arg("--strip-components=1"),
            )
            .status()
            .context("Failed to download repository")?;

        Ok(status.success())
    }

    /// Copy local plugin
    fn copy_local_plugin(source: &str, dest: &Path) -> Result<()> {
        let source_path = Path::new(source);
        for entry in fs::read_dir(source_path)? {
            let entry = entry?;
            let path = entry.path();
            let dest_path = dest.join(path.file_name().context("Invalid file name")?);

            if path.is_dir() {
                fs::create_dir_all(&dest_path)?;
                Self::copy_dir(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path)?;
            }
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
        for component in ["functions", "conf.d", "completions", "themes"] {
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
    fn remove(&mut self, plugins: &[String]) -> Result<()> {
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

        self.save_plugins()?;
        println!("Removed {} plugins total", removed_count);
        Ok(())
    }

    /// Update plugins
    fn update(&mut self, plugins: &[String]) -> Result<()> {
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
    fn list(&self, pattern: Option<&str>) -> Result<()> {
        let plugins: Vec<&String> = self.installed_plugins.iter().collect();
        if let Some(pattern) = pattern {
            let re = Regex::new(pattern)?;
            let filtered: Vec<&String> = plugins
                .iter()
                .filter(|p| re.is_match(p.as_str()))
                .cloned()
                .collect();
            for plugin in filtered {
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut fisher = Fisher::new(cli.fisher_path)?;

    match cli.command {
        Commands::Install { plugins } => fisher.install(&plugins),
        Commands::Remove { plugins } => fisher.remove(&plugins),
        Commands::Update { plugins } => fisher.update(&plugins),
        Commands::List { pattern } => fisher.list(pattern.as_deref()),
    }
}
