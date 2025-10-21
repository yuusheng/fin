pub mod core;
pub mod lock;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{env, path::PathBuf};

use core::Fin;

#[derive(Debug, Parser)]
#[clap(name = "fin", version = env!("CARGO_PKG_VERSION"), about = "A plugin manager for Fish")]
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
        plugins: Option<Vec<String>>,

        /// Install plugins from the Fish plugin registry
        #[clap(long, short, default_value_t = false)]
        force: bool,
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
    List {},
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut fin = Fin::new(cli.fin_path)?;

    match cli.command {
        Commands::Install { plugins, force } => fin.install(plugins, force),
        Commands::Remove { plugins } => fin.remove(&plugins),
        Commands::Update { plugins } => fin.update(&plugins),
        Commands::List {} => fin.list(),
    }
}
