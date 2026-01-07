# Schalentier 🦐

> *German: Schalentier [ˈʃaːlənˌtiːɐ̯] — "shellfish", literally "shell animal"*

**Your tools. Your configs. Every machine. One file.**

Define your entire environment in a single TOML file — tools to install, configs to patch, shell customizations — then sync it across all your machines with Git.

```toml
# ~/.config/schalentier/schalentier.toml

[tools]
ripgrep = {}
bat = { provider = "cargo" }
python = { provider = "conda", version = "3.12" }

[dotfiles."~/.gitconfig".user]
name = "Ada Lovelace"
email = "ada@example.com"

[dotfiles."~/.config/micro/settings.json"]
colorscheme = "dracula"
tabsize = 4
```

```bash
# New laptop? One command.
$ schalentier sync --remote git@github.com:ada/dotfiles.git --pull

⠋ Pulling from git@github.com:ada/dotfiles.git...
✓ Pull completed
✓ Installed 'ripgrep' v14.1.1 via binary
✓ Installed 'bat' v0.24.0 via cargo
✓ Installed 'python' v3.12 via conda
✓ Applied 2 config patches
✓ Sync complete!
```

No more "works on my machine." No more 47-step setup guides.

---

## Why Another Tool?

| Tool | Installs CLI tools | Syncs configs | Patches files | Git sync |
|------|:------------------:|:-------------:|:-------------:|:--------:|
| **chezmoi** | ❌ | ✅ | ✅ | ✅ |
| **yadm** | ❌ | ✅ | ❌ | ✅ |
| **mise/asdf** | ✅ (runtimes) | ❌ | ❌ | ❌ |
| **aqua** | ✅ | ❌ | ❌ | ❌ |
| **Homebrew** | ✅ | ❌ | ❌ | ❌ |
| **Schalentier** | ✅ | ✅ | ✅ | ✅ |

**chezmoi** and **yadm** are great for dotfiles, but they won't install your tools.
**mise** and **aqua** install tools, but won't manage your configs.
**Schalentier** does both — one file, one binary, done.

---

## Quick Start

### Install

```bash
# One-liner install (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/user/schalentier/main/install.sh | bash

# Or download the binary directly
# Linux (static musl binary - works everywhere)
curl -LO https://github.com/cosinusalpha/schalentier/releases/latest/download/schalentier-linux-x86_64
chmod +x schalentier-linux-x86_64 && sudo mv schalentier-linux-x86_64 /usr/local/bin/schalentier
```

### Initialize

```bash
$ schalentier init

Welcome to schalentier!

? Which package managers should be bootstrapped?
> [x] uv - Fast Python package installer
  [x] Miniforge/Conda - Scientific packages

? Proceed with installation? Yes

✓ Initialization complete!
```

### Add Your First Tool

```bash
# Install ripgrep (tries GitHub releases first, falls back to other providers)
$ schalentier add ripgrep

# Install from a specific provider
$ schalentier add python --provider conda

# Just add to config, don't install yet
$ schalentier add neovim --no-install
```

### Sync Across Machines

```bash
# First machine: push your setup
$ schalentier sync --remote git@github.com:you/dotfiles.git --push

# Second machine: pull and install everything
$ schalentier sync --pull
```

---

## Configuration

Schalentier uses a single TOML file: `~/.config/schalentier/schalentier.toml`

### Full Example

```toml
# =============================================================================
# SCHALENTIER CONFIGURATION
# =============================================================================

[settings]
# Provider priority (first available wins)
provider_priority = ["binary", "cargo", "brew", "conda", "uv", "system"]

# Auto-update tools on sync
auto_update = false

[sync]
# Git remote for syncing
remote = "git@github.com:yourname/dotfiles.git"

# Sync mode: manual, pull, push, bidirectional
mode = "manual"

# =============================================================================
# TOOLS
# =============================================================================
# Each tool can specify:
#   - provider: force a specific provider (optional)
#   - version: version constraint (optional)
#   - options: provider-specific options (optional)

[tools.ripgrep]
# No config = use provider priority, latest version

[tools.fd]
provider = "cargo"  # Force install via cargo

[tools.bat]
version = "0.24.0"  # Pin to specific version

[tools.python]
provider = "conda"
version = "3.12"

[tools.ruff]
provider = "uv"  # Python linter via uv tool

[tools.jq]
provider = "binary"  # GitHub releases

[tools.htop]
provider = "system"  # apt/pacman/dnf

# =============================================================================
# DOTFILES (Config Patching)
# =============================================================================
# Schalentier intelligently merges your settings into existing config files.
# It detects the format from the file extension and preserves existing values.
#
# Supported formats:
#   - JSON (.json) - deep merge
#   - TOML (.toml) - deep merge
#   - YAML (.yaml, .yml) - deep merge
#   - INI (.ini, .gitconfig, .cfg) - section-aware merge
#   - KeyValue (.env, KEY=VALUE style) - key-aware merge
#   - Unknown - replace mode (use _content key)

[dotfiles]

# --- JSON Example: micro editor ---
[dotfiles."~/.config/micro/settings.json"]
colorscheme = "dracula"
tabsize = 4
tabstospaces = true
autoclose = true
# These get merged into the JSON file, preserving any other settings you have

# --- TOML Example: starship prompt ---
[dotfiles."~/.config/starship.toml"]
add_newline = false

[dotfiles."~/.config/starship.toml".character]
success_symbol = "[➜](bold green)"
error_symbol = "[✗](bold red)"

[dotfiles."~/.config/starship.toml".git_branch]
symbol = "🌱 "

# --- INI Example: git config ---
[dotfiles."~/.gitconfig"]
[dotfiles."~/.gitconfig".user]
name = "Your Name"
email = "you@example.com"

[dotfiles."~/.gitconfig".core]
editor = "micro"
autocrlf = "input"

[dotfiles."~/.gitconfig".alias]
co = "checkout"
br = "branch"
st = "status"
lg = "log --oneline --graph"

# --- KeyValue Example: environment ---
[dotfiles."~/.config/schalentier/env.local"]
EDITOR = "micro"
PAGER = "less -R"
MANPAGER = "sh -c 'col -bx | bat -l man -p'"

# --- Unknown Format: use _content for complete replacement ---
[dotfiles."~/.vimrc"]
_content = """
set number
set relativenumber
set tabstop=4
set shiftwidth=4
set expandtab
set autoindent
syntax on
colorscheme desert
"""
```

---

## Commands Reference

### Core Commands

```bash
schalentier init [--yes] [--force]    # Initialize schalentier
schalentier add <tool> [--provider X] # Add and install a tool
schalentier remove <tool>             # Remove a tool
schalentier list [--detailed]         # List managed tools
schalentier search <query>            # Search across all providers
schalentier update [tool] [--dry-run] # Check/apply updates
schalentier doctor [--fix]            # Diagnose issues
```

### Sync Commands

```bash
schalentier sync                      # Bidirectional sync
schalentier sync --push               # Push local changes
schalentier sync --pull               # Pull and install
schalentier sync --prune              # Remove orphaned tools
schalentier sync --remote <url>       # Set/use specific remote
```

### Config Patching

```bash
schalentier config list               # Show configured dotfiles
schalentier config diff               # Preview changes
schalentier config apply              # Apply all patches
schalentier config reset <file>       # Restore from backup
```

### Shell Integration

```bash
schalentier alias 'll=ls -la'         # Create alias
schalentier alias --list              # List aliases
schalentier alias --remove ll         # Remove alias

schalentier snippet add yazi          # Add shell snippet
schalentier snippet list              # List snippets
schalentier snippet remove yazi       # Remove snippet
```

---

## Providers

Schalentier searches multiple sources to install your tools:

| Provider | Source | Best for | Notes |
|----------|--------|----------|-------|
| `binary` | GitHub Releases | Most CLI tools | Fast, no dependencies, auto-detects platform |
| `cargo` | crates.io | Rust tools | Builds from source, needs rustc |
| `brew` | Homebrew/Linuxbrew | macOS packages | Cross-platform, large catalog |
| `conda` | conda-forge | Scientific tools | Python/R/Julia ecosystem |
| `uv` | PyPI | Python CLIs | Fast, isolated Python tools |
| `system` | apt/pacman/dnf | System packages | May need sudo |

### Provider Fallback

If your preferred provider fails, schalentier automatically tries the next one:

```bash
$ schalentier add some-tool --provider cargo
# If cargo fails (not installed, build error, etc.):
ℹ Note: Preferred provider Cargo unavailable, used Binary instead
✓ Installed 'some-tool' v1.0.0 via binary
```

---

## Search Results

Search aggregates results from all providers and clusters by package:

```bash
$ schalentier search ripgrep

Found 3 unique packages:

  ripgrep
    Available from: Binary v14.1.1, Cargo v14.1.1, Conda v14.1.0
    A fast line-oriented search tool

  ripgrep-all
    Available from: Cargo v1.0.0
    ripgrep, but also search in PDFs, E-Books, Office documents...
```

---

## Dotfile Patching Deep Dive

Unlike tools that replace entire files, schalentier **merges** your settings:

### Before (your existing config)

```json
{
  "colorscheme": "default",
  "font_size": 14,
  "custom_setting": "preserved"
}
```

### Your schalentier.toml

```toml
[dotfiles."~/.config/app/settings.json"]
colorscheme = "dracula"
tabsize = 4
```

### After `schalentier config apply`

```json
{
  "colorscheme": "dracula",
  "font_size": 14,
  "custom_setting": "preserved",
  "tabsize": 4
}
```

Your `font_size` and `custom_setting` are preserved. Only the specified keys are updated.

### Backup & Recovery

Before the first modification, schalentier creates a backup:

```
~/.config/app/settings.json.schalentier-backup
```

Restore anytime:

```bash
$ schalentier config reset "~/.config/app/settings.json"
✓ Restored from backup
```

---

## Shell Setup

After init, add to your shell config:

### Bash (`~/.bashrc`)

```bash
if [ -f "$HOME/.schalentier/env.sh" ]; then
    source "$HOME/.schalentier/env.sh"
fi
```

### Zsh (`~/.zshrc`)

```zsh
if [ -f "$HOME/.schalentier/env.sh" ]; then
    source "$HOME/.schalentier/env.sh"
fi
```

### Fish (`~/.config/fish/config.fish`)

```fish
if test -f "$HOME/.schalentier/env.fish"
    source "$HOME/.schalentier/env.fish"
end
```

### PowerShell (`$PROFILE`)

```powershell
if (Test-Path "$HOME\.schalentier\env.ps1") {
    . "$HOME\.schalentier\env.ps1"
}
```

---

## Comparison Table

| Feature | Schalentier | chezmoi | mise | aqua | Homebrew |
|---------|-------------|---------|------|------|----------|
| Install CLI tools | ✅ | ❌ | ✅ | ✅ | ✅ |
| Multi-provider fallback | ✅ | - | ❌ | ❌ | ❌ |
| Config file patching | ✅ | ✅ | ❌ | ❌ | ❌ |
| Git sync | ✅ | ✅ | ❌ | ❌ | ❌ |
| Shell aliases/snippets | ✅ | ❌ | ❌ | ❌ | ❌ |
| Single binary | ✅ | ✅ | ✅ | ✅ | ❌ |
| Windows support | 🚧 | ✅ | ✅ | ✅ | ❌ |
| No runtime deps | ✅ | ✅ | ✅ | ✅ | ❌ (Ruby) |
| Declarative config | ✅ | ✅ | ✅ | ✅ | ❌ |
| Tool adoption | ✅ | - | ❌ | ❌ | ❌ |

---

## Philosophy

1. **One file to rule them all** — Your entire setup in `schalentier.toml`
2. **Graceful degradation** — If one provider fails, try another
3. **Non-destructive** — Merge configs, don't replace them
4. **Adopt, don't fight** — Detect existing tools instead of reinstalling
5. **Static binary** — No Python, no Ruby, no Node. Just works.

---

## Building from Source

```bash
# Clone
git clone https://github.com/cosinusalpha/schalentier.git
cd schalentier

# Build (requires Rust)
cargo build --release

# Static musl build (Linux, fully portable)
./build-musl.sh
```

---

## Contributing

PRs welcome! Please run the tests:

```bash
cargo test                    # Unit tests
./tests/smoke/run.sh          # Integration tests (requires podman/docker)
```

---

## License

MIT

---

<p align="center">
  <i>Stop yak-shaving your dev environment. Start building.</i>
</p>
