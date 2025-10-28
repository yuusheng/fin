# Fin 🐟

> A fast, lightweight plugin manager for [Fish shell](https://fishshell.com/), inspired by [Fisher](https://github.com/jorgebucaran/fisher). Written in Rust for speed and reliability, with git-shareable lock files.

## Features

- **Fast & Parallel**: Built in Rust with parallel plugin installation using Rayon
- **Git-Friendly**: Track your plugin configuration with `fin-lock.toml`
- **Fisher-Compatible**: Works with Fish plugins just like Fisher
- **Simple**: Minimal commands, maximum functionality
- **Automatic Lock File**: Generates and maintains a lock file to track installed plugins

## Installation

### Prerequisites

- [Fish shell](https://fishshell.com/)
- `curl` and `tar` (for downloading plugins)

### Homebrew

```bash
brew install yuusheng/tap/fin
```

### Curl

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/yuusheng/fin/releases/download/v0.0.1/fin-installer.sh | sh
```

## Usage

### Install Plugins

Install plugins from GitHub repositories:

```bash
# Install a plugin from GitHub
fin install jethrokuan/z

# Install multiple plugins
fin install jorgebucaran/nvm.fish ilancosman/tide@v6

# Install from lock file (fin-lock.toml)
fin install

# Force reinstall (useful for updates)
fin install jethrokuan/z --force
```

### Remove Plugins

```bash
# Remove one or more plugins
fin remove jorgebucaran/fisher ilancosman/tide
```

### Update Plugins

```bash
# Update all installed plugins
fin update

# Update specific plugins
fin update jorgebucaran/nvm.fish ilancosman/tide
```

### List Plugins

```bash
# List all installed plugins
fin list
```

## Plugin Format

Fin supports plugins using GitHub repository syntax:

- `owner/repo` - Installs from the latest commit on the default branch
- `owner/repo@branch` - Installs from a specific branch
- `owner/repo@tag` - Installs from a specific tag

## Lock File

Fin automatically generates and maintains a `fin-lock.toml` file in your Fish configuration directory (typically `~/.config/fish/`). This file tracks:

- Plugin names and sources
- Installed files
- Installation timestamp
- Plugin metadata (commit hash, branch, checksum)

Example `fin-lock.toml`:

```toml
version = "1.0"
generated_at = "2025-10-20T12:34:56Z"

[[plugins]]
name = "z"
source = "https://github.com/jethrokuan/z/archive/HEAD.tar.gz"
installed_files = [
    "functions/__z_complete.fish",
    "functions/__z_clean.fish",
    "functions/__z_add.fish",
    "conf.d/z.fish",
    "functions/__z.fish",
]

[[plugins]]
name = "ilancosman/tide"
source = "https://github.com/ilancosman/tide/archive/v6.tar.gz"
installed_files = [
    "functions/tide.fish",
    "conf.d/tide.fish",
    "completions/tide.fish",
]
```

### Benefits of Lock Files

- **Version Control**: Commit `fin-lock.toml` to share your exact plugin setup across machines
- **Reproducibility**: Reinstall the exact same plugin versions on different systems
- **Team Collaboration**: Share consistent Fish shell configurations with your team

## How It Works

Fin manages Fish shell plugins by:

1. Downloading plugins from GitHub as tar.gz archives
2. Extracting plugin files to temporary directories
3. Copying files from standard plugin directories (`functions/`, `conf.d/`, `completions/`) to your Fish config directory
4. Tracking installed files in `fin-lock.toml` for easy removal and updates

## Configuration

### Custom Installation Path

By default, Fin installs plugins to your Fish configuration directory (usually `~/.config/fish/`). You can specify a custom path:

```bash
fin --fin-path /custom/path install jorgebucaran/fisher
```

### Environment Variables

- `__fish_config_dir`: Override the Fish configuration directory location

## License

This project is open source. See the LICENSE file for details.

## Acknowledgements

- [Fisher](https://github.com/jorgebucaran/fisher) - The original Fish plugin manager that inspired this project

## Related Projects

- [Fisher](https://github.com/jorgebucaran/fisher) - The original Fish plugin manager
- [Oh My Fish](https://github.com/oh-my-fish/oh-my-fish) - Another Fish shell framework
