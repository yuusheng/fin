pub mod core;
pub mod lock;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{env, path::PathBuf};

use core::Fin;
use lock::LockFile;

#[derive(Debug, Parser)]
#[clap(name = "fisher", version = env!("CARGO_PKG_VERSION"), about = "A plugin manager for Fish")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// Plugin installation path (default: Fish config directory)
    #[clap(long)]
    fin_path: Option<PathBuf>,
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

fn main() -> Result<()> {
    let lock_file = LockFile::load("./fin.lock").unwrap();
    lock_file.save("./fin-locl.toml").unwrap();

    let cli = Cli::parse();
    let mut fin = Fin::new(cli.fin_path)?;

    match cli.command {
        Commands::Install { plugins } => fin.install(&plugins),
        Commands::Remove { plugins } => fin.remove(&plugins),
        Commands::Update { plugins } => fin.update(&plugins),
        Commands::List { pattern } => fin.list(pattern.as_deref()),
    }
}
