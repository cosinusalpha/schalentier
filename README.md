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

| Tool            | Installs CLI tools | Syncs configs | Patches files | Git sync |
|-----------------|:------------------:|:-------------:|:-------------:|:--------:|
| **chezmoi**     |         ❌          |       ✅       |       ✅       |    ✅     |
| **yadm**        |         ❌          |       ✅       |       ❌       |    ✅     |
| **mise/asdf**   |    ✅ (runtimes)    |       ❌       |       ❌       |    ❌     |
| **aqua**        |         ✅          |       ❌       |       ❌       |    ❌     |
| **Homebrew**    |         ✅          |       ❌       |       ❌       |    ❌     |
| **Schalentier** |         ✅          |       ✅       |       ✅       |    ✅     |

**chezmoi** and **yadm** are great for dotfiles, but they won't install your tools.
**mise** and **aqua** install tools, but won't manage your configs.
**Schalentier** does both — one file, one binary, done.

---

## Quick Start

### Install

```bash
# Install via Cargo (requires Rust toolchain)
cargo install schalentier

# One-liner install script (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/cosinusalpha/schalentier/main/install.sh | bash

# Or download the binary directly
# Linux (static musl binary - works everywhere)
curl -LO https://github.com/cosinusalpha/schalentier/releases/latest/download/schalentier-linux-x86_64
chmod +x schalentier-linux-x86_64 && sudo mv schalentier-linux-x86_64 /usr/local/bin/schalentier
```

### Initialize

```bash
$ schalentier init

Welcome to schalentier!

═══════════════════════════════════System Tools Detected

✓ apt (Debian/Ubuntu) (apt 3.2.0 (amd64))
✗ uv
✗ conda
✗ brew
✗ cargo
✗ rust
✗ node
✗ go

? Which package managers should be bootstrapped?
> [x] uv - Fast Python package installer (recommended for Python CLI tools)
  [x] Miniforge/Conda - Scientific packages and isolated environments
  [x] Rust (rustup) - Rust toolchain and cargo package manager
  [x] Node.js - JavaScript runtime and npm package manager
  [x] Go - Go toolchain for building and installing Go CLI tools

? Proceed with installation? Yes

✓ Initialization complete!

? Add schalentier's environment setup to your shell config now? Yes
? Shell config file to update: /home/user/.bashrc
✓ Added schalentier setup to /home/user/.bashrc
```

#### Bootstrapped Tools

Schalentier can bootstrap these development toolchains:

| Tool | Description | Binary Location |
|------|-------------|-----------------|
| **uv** | Fast Python package installer | `~/.schalentier/bin/uv` |
| **Miniforge** | Conda distribution for scientific computing | `~/.schalentier/conda/` |
| **Rust** | Rust toolchain via rustup | `~/.schalentier/.cargo/` |
| **Node.js** | JavaScript runtime (current: v26.5.0) | `~/.schalentier/node/` |
| **Go** | Go programming language (v1.22.5) | `~/.schalentier/go/` |

All bootstrapped tools are managed under `~/.schalentier/` for easy cleanup and isolation.

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

#### GitHub Gist Sync (Encrypted)

Schalentier can sync your configuration via encrypted GitHub Gists - perfect for personal dotfiles without managing a git repository:

```bash
# Store GitHub token in encrypted secrets
$ schalentier secret set GITHUB_TOKEN --tags github
Value: ****
✓ Secret 'GITHUB_TOKEN' saved

# Create new encrypted gist and push config
$ schalentier sync --remote gist://new --push
✓ Created secret gist: gist://abc123def456
✓ Config pushed successfully

# Add the gist ID to your config (edit ~/.config/schalentier/schalentier.toml)
# [sync]
# remote = "gist://abc123def456"

# Pull on another machine
$ schalentier sync --pull
✓ Downloaded encrypted gist
✓ Decrypted with master password
✓ Installed 5 tools
```

**Public vs Secret Gists:**

```bash
# Create a public gist (visible on your profile)
$ schalentier sync --push --remote gist://new --public

# Create a secret gist (default - unlisted, requires URL)
$ schalentier sync --push --remote gist://new --secret

# Or set default in config
[sync]
remote = "gist://abc123def456"
gist_public = false  # true = public, false = secret (default)
```

**Security:** All gist content is encrypted with age encryption using the same master password stored in your OS keyring. GitHub never sees your plaintext configuration - only encrypted ciphertext.

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
provider_priority = ["binary", "go", "cargo", "brew", "conda", "uv", "system"]

# Auto-update tools on sync
auto_update = false

# How long a cached `audit` result (OSV.dev) stays valid before it's re-queried
audit_cache_ttl_hours = 24

[sync]
# Sync remote: git repository or GitHub Gist
# Git repository:
remote = "git@github.com:yourname/dotfiles.git"
# GitHub Gist (encrypted):
# remote = "gist://abc123def456"

# Sync mode: manual, pull, push, bidirectional
mode = "manual"

# For gist:// remotes: create as public (false = secret/private)
gist_public = false

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

[tools.lazygit]
provider = "go"  # Go CLI tool via go install

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

### Templating & Variables

Dotfiles can be rendered as [Jinja2](https://jinja.palletsprojects.com/) templates
(via minijinja). Set `_template = true` on a dotfile entry to enable rendering. Available
context: `{{ os }}`, `{{ arch }}`, `{{ hostname }}`, `{{ username }}`, `{{ home }}`,
`{{ env.VAR }}`, `{{ secret.NAME }}` (from your encrypted secrets), and `{{ var.NAME }}`
(from the `[variables]` section below).

```toml
[variables]
work_email = "ada@company.com"
default_editor = "nvim"

# Nested variables → {{ var.work.email }}
[variables.work]
email = "ada@company.com"
name = "Ada Lovelace (Company)"

[dotfiles."~/.gitconfig".user]
_template = true
name = "{{ var.work.name }}"
email = "{% if hostname == 'work-laptop' %}{{ var.work_email }}{% else %}ada@personal.dev{% endif %}"

[dotfiles."~/.config/gh/hosts.yml"."github.com"]
_template = true
oauth_token = "{{ secret.GITHUB_TOKEN }}"
```

### Package Aliases

Define custom packages or override registry entries with `[aliases]`. Each alias maps
provider names to provider-specific package names.

```toml
[aliases.my-internal-tool]
description = "Company internal tool"

[aliases.my-internal-tool.providers.pnpm]
name = "@mycompany/internal-tool"
```

---

## Commands Reference

### Core Commands

```bash
schalentier init [--yes] [--force] [--skip-bootstrap] [--setup-shell]  # Initialize schalentier
schalentier add <tool> [--provider X] [--dry-run]      # Add and install a tool
schalentier remove <tool> [--keep-installed]           # Remove a tool
schalentier list [--detailed] [--provider X] [--security]  # List managed tools
schalentier search <query> [--limit N] [--provider X]  # Search across all providers
schalentier update [tool] [--dry-run] [--force]        # Check/apply updates
schalentier doctor [--fix]                             # Diagnose issues
schalentier audit [package] [--refresh]                # Security audit (via OSV.dev)
```

> **Security audit:** `audit` (and the pre-install check) queries the
> [OSV.dev](https://osv.dev) vulnerability database, covering the crates.io (cargo),
> PyPI (uv), npm (npm/pnpm/yarn), and Go ecosystems. Tools installed as prebuilt
> binaries or via system/brew have no queryable ecosystem and are skipped. When an
> installed version is known, the audit narrows results to advisories affecting it.
> Results are cached (`~/.schalentier/osv_cache.json`, TTL configurable via
> `audit_cache_ttl_hours`, default 24h) so repeated `audit`/`add` calls don't hammer
> OSV.dev; pass `--refresh` to bypass the cache. `list --security` shows each tool's
> cached status without any network call — run `audit` first to populate it.
>
> **Update pinning:** a tool pinned to a specific version in `schalentier.toml`
> (`[tools.<name>] version = "1.2.3"`) is skipped by `update` unless `--force` is
> passed; `version = "latest"` (or omitting it) means always update.

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

### Secrets (encrypted, age + system keyring)

```bash
schalentier secret set <NAME> [--value V] [--tags a,b] [--global]  # Store a secret
schalentier secret get <NAME>                    # Print value to stdout
schalentier secret list [--tags a,b]             # List secret names
schalentier secret delete <NAME> [--global]      # Remove a secret
schalentier secret export [--shell bash|fish] [--tags a,b]  # Emit export statements
schalentier secret edit                          # Decrypt → $EDITOR → re-encrypt
schalentier secret change-password               # Re-encrypt with new password
schalentier secret shell [--tags a,b]            # Spawn shell with secrets in env
schalentier secret run [--tags a,b] -- <cmd>     # Run command with secrets in env
```

### Registry

```bash
schalentier registry validate         # Check registry format
schalentier registry info             # Show package statistics
schalentier registry update           # Download latest registry from GitHub
```

### Shell Completions

```bash
schalentier completions <bash|zsh|fish|powershell|elvish>  # Generate completion script
```

---

## Providers

Schalentier searches multiple sources to install your tools:

| Provider | Source             | Best for         | Notes                                        |
|----------|--------------------|------------------|----------------------------------------------|
| `binary` | GitHub Releases    | Most CLI tools   | Fast, no dependencies, auto-detects platform |
| `go`     | Go modules         | Go CLI tools     | Fast, static binaries, `go install`          |
| `cargo`  | crates.io          | Rust tools       | Builds from source, needs rustc              |
| `brew`   | Homebrew/Linuxbrew | macOS packages   | Cross-platform, large catalog                |
| `conda`  | conda-forge        | Scientific tools | Python/R/Julia ecosystem                     |
| `uv`     | PyPI               | Python CLIs      | Fast, isolated Python tools                  |
| `system` | apt/pacman/dnf     | System packages  | May need sudo                                |

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

`schalentier init` prompts interactively to add this for you (or use
`--setup-shell` to do it non-interactively, or `--yes` alone to just print the
snippet). To do it manually instead, add to your shell config:

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

---

## Comparison Table

| Feature                 | Schalentier | chezmoi | mise | aqua | Homebrew |
|-------------------------|-------------|---------|------|------|----------|
| Install CLI tools       | ✅           | ❌       | ✅    | ✅    | ✅        |
| Multi-provider fallback | ✅           | -       | ❌    | ❌    | ❌        |
| Config file patching    | ✅           | ✅       | ❌    | ❌    | ❌        |
| Git sync                | ✅           | ✅       | ❌    | ❌    | ❌        |
| Encrypted gist sync     | ✅           | ❌       | ❌    | ❌    | ❌        |
| Shell aliases/snippets  | ✅           | ❌       | ❌    | ❌    | ❌        |
| Single binary           | ✅           | ✅       | ✅    | ✅    | ❌        |
| No runtime deps         | ✅           | ✅       | ✅    | ✅    | ❌ (Ruby) |
| Declarative config      | ✅           | ✅       | ✅    | ✅    | ❌        |
| Tool adoption           | ✅           | -       | ❌    | ❌    | ❌        |

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
