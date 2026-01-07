# Schalentier: Development Roadmap

**Last Updated:** 2026-01-07 (Milestone 6 complete)

---

## Phase 1: The Skeleton & CLI Foundation - COMPLETE

**Goal:** A compilable binary that parses arguments and handles errors.

### Task 1.1: Project Initialization - DONE

- **Action:** Run `cargo new`. Configure `Cargo.toml` with dependencies.
- **Status:** Complete
- **Files:** `Cargo.toml`, `.cargo/config.toml`
- **Dependencies:** clap, tokio, anyhow, thiserror, tracing, tracing-subscriber, serde, serde_json, toml, reqwest (rustls), dirs, async-trait, futures-util, urlencoding, flate2, tar, zip, which
- **Note:** Pure Rust build - no C dependencies. Uses rustls-rustcrypto for TLS.

### Task 1.2: CLI Argument Parsing - DONE

- **Action:** Define `clap` structs for all commands.
- **Status:** Complete
- **Files:** `src/cli.rs`
- **Commands:** init, add, sync, update, doctor, remove, list, search
- **AC:** `schalentier --help` shows correct menu. `schalentier add` without args returns error.

### Task 1.3: Logging & Error Handling - DONE

- **Action:** Setup `tracing-subscriber`, implement global error handler.
- **Status:** Complete
- **Files:** `src/logging.rs`, `src/error.rs`
- **Features:** RUST_LOG support, verbose flag, ANSI colors, error chain display
- **AC:** `RUST_LOG=debug schalentier` shows logs. Errors display with red text.

---

## Phase 2: Configuration & State - COMPLETE

**Goal:** The tool can remember who it is and what it has installed.

### Task 2.1: Data Models (Structs) - DONE

- **Action:** Create config.rs with all data structures.
- **Status:** Complete
- **Files:** `src/config.rs`
- **Structs:** SchalentierConfig, LocalState, Settings, ToolEntry, InstalledTool, BootstrapState, SyncConfig, Provider enum, ToolStatus enum
- **AC:** Unit tests pass for serialization/deserialization (TOML and JSON).

### Task 2.2: Local State Management - DONE

- **Action:** Implement `LocalState::load()` and `save()`.
- **Status:** Complete
- **Files:** `src/state.rs`
- **Features:** Directory creation (~/.schalentier), JSON persistence, 0o600 permissions on Unix
- **AC:** Creates state directory, saves/loads JSON correctly.

### Task 2.3: Provider Priority Configuration - DONE

- **Action:** Add `priority` list to Settings, implement merge logic.
- **Status:** Complete
- **Files:** `src/config.rs`, `src/state.rs`
- **Features:** default_provider_priority(), merge_settings()
- **AC:** Configuration file successfully overrides default provider order.

---

## Phase 3: The "Brain" (Bootstrap & Shells) - COMPLETE

**Goal:** The tool can set up its own isolated environment.

### Task 3.1: Architecture Detection - DONE

- **Action:** Implement `bootstrap::get_arch()` and `get_os()`.
- **Status:** Complete
- **Files:** `src/bootstrap.rs`
- **Features:** Arch enum (X86_64, Aarch64), Os enum (Linux, MacOS, Windows)
- **AC:** Returns correct values. Errors on unsupported platforms.

### Task 3.2: Miniforge & Tool Bootstrap - DONE

- **Action:** Implement download and installation of Miniforge and uv.
- **Status:** Complete
- **Files:** `src/bootstrap.rs`, `src/archive.rs`
- **Implemented:**
  - `miniforge_url()` - correct URLs for all platforms
  - `uv_url()` - correct URLs for all platforms
  - `download_file()` - async HTTP download
  - `Bootstrap::run()` orchestrator
  - Archive extraction (tar.gz, zip only - pure Rust) via `archive` module
  - `install_uv()` - extracts archive and installs binary
  - `install_miniforge()` - runs installer in batch mode (Linux/macOS) or silent mode (Windows)
- **Note:** bzip2/xz support removed to maintain pure Rust build
- **AC:** `~/.schalentier/conda/bin/conda` and `~/.schalentier/bin/uv` exist and work.

### Task 3.3: Shell Script Generation - DONE

- **Action:** Generate `env.sh`, `env.fish`, `env.ps1`.
- **Status:** Complete
- **Files:** `src/shell.rs`
- **Features:**
  - `generate_bash_env()`, `generate_fish_env()`, `generate_powershell_env()`
  - `write_env_scripts()` - writes all three
  - `shell_init_snippet()` - user instructions
  - `ShellType::detect()` - auto-detect current shell
- **AC:** Generates correct content for all shells. Test verifies files created.

---

## Phase 4: The Provider Engine - 100% COMPLETE

**Goal:** The tool can search for and install packages via multiple providers.

### Task 4.1: The Installer Trait - DONE

- **Action:** Define the `Installer` async trait.
- **Status:** Complete
- **Files:** `src/provider/mod.rs`
- **Methods:** `provider()`, `search()`, `install()`, `uninstall()`, `is_installed()`, `installed_version()`, `is_available()`
- **Supporting types:** SearchResult, InstallResult, ProviderRegistry
- **AC:** MockProvider implemented and works in tests.

### Task 4.2: System Provider - DONE

- **Action:** Implement `System` provider to detect and use apt/pacman/dnf/apk.
- **Status:** Complete
- **Files:** `src/provider/system.rs`
- **Features:**
  - `PackageManager` enum (Apt, Pacman, Dnf, Apk, Zypper)
  - `PackageManager::detect()` - auto-detect from /usr/bin paths
  - Search output parsing for all package managers
  - sudo passthrough with `Stdio::inherit()`
- **AC:** Correctly detects OS package manager. Installs with sudo prompt.

### Task 4.3: Binary Provider (GitHub Releases) - DONE

- **Action:** Implement `Binary` provider for GitHub releases.
- **Status:** Complete
- **Files:** `src/provider/binary.rs`
- **Implemented:**
  - `search_github()` - GitHub API search
  - `get_latest_release()` - fetch release info
  - `find_best_asset()` - platform-aware asset matching (prefers musl, correct arch/os)
  - Archive extraction (tar.gz, zip only - pure Rust)
  - Binary detection with `guess_binary_name()` (ripgrep->rg, fd-find->fd, etc.)
  - Executable permission setting on Unix
- **AC:** Can find and install a binary like "ripgrep" from GitHub.

### Task 4.4: Cargo Provider - DONE

- **Action:** Implement `Cargo` provider wrapping cargo install.
- **Status:** Complete
- **Files:** `src/provider/cargo.rs`
- **Features:**
  - `search_crates()` - crates.io API search
  - `cargo install` with version support
  - `cargo uninstall`
  - `is_installed()` and `installed_version()` checks
- **AC:** Can install crates via cargo.

### Task 4.5: Conda Provider - DONE

- **Action:** Implement `Conda` provider wrapping mamba/conda.
- **Status:** Complete
- **Files:** `src/provider/conda.rs`
- **Features:**
  - `mamba search --json` or `conda search --json` for package search
  - JSON output parsing for package info
  - `conda install -y -c conda-forge` for installation
  - Channel configuration (conda-forge default)
  - Auto-detect mamba/conda in schalentier data dir or system PATH
- **AC:** Can search and install packages via conda.

### Task 4.6: Brew Provider - DONE

- **Action:** Implement `Brew` provider wrapping Homebrew/Linuxbrew.
- **Status:** Complete
- **Files:** `src/provider/brew.rs`
- **Features:**
  - Auto-detect brew in common locations (Linuxbrew, macOS ARM/Intel)
  - `brew search` for package search
  - `brew install` / `brew uninstall`
  - `brew info --json=v2` for version info
  - Version extraction from installed packages
- **AC:** Can search and install packages via brew.

### Task 4.7: UV Provider - DONE

- **Action:** Implement `UV` provider for Python tools via uv.
- **Status:** Complete
- **Files:** `src/provider/uv.rs`
- **Features:**
  - Use schalentier-installed uv or system uv
  - `uv tool install` for CLI tools
  - `uv tool uninstall`
  - PyPI API search for packages
  - `uv tool list` for installed version detection
- **AC:** Can install Python CLI tools via uv.

### Task 4.8: Provider Fallback Logic - DONE

- **Action:** When preferred provider unavailable, try alternatives in priority order.
- **Status:** Complete
- **Files:** `src/provider/mod.rs` (`install_with_fallback` method)
- **Features:**
  - `registry.install_with_fallback()` tries preferred provider first
  - Falls back to other providers in registration (priority) order
  - Logs which provider was actually used
  - Stores actual provider in state (not just requested)
  - Clear warning when fallback is used
- **AC:** Tool installs from alternative provider when preferred unavailable.

### Task 4.9: Search Provider Filter - DONE

- **Action:** Add `--provider` flag to search command.
- **Status:** Complete
- **Files:** `src/cli.rs`, `src/main.rs`
- **Features:**
  - `schalentier search ripgrep --provider binary` - search only binary provider
  - Unknown provider gracefully falls back to all-provider search
  - Warning if specified provider is unavailable
- **AC:** Search can be filtered to single provider.

### Task 4.10: Search Aggregation (Clustering) - DONE

- **Action:** Implement parallel search with result clustering by package name.
- **Status:** Complete
- **Implementation:**
  - Added `ClusteredSearchResult` and `ProviderInfo` structs in `src/provider/mod.rs`
  - Added `search_all_clustered()` method to `ProviderRegistry`
  - Groups results by normalized package name (lowercase)
  - Shows which providers have each package with their versions
  - Updated `cmd_search` to use clustered results for multi-provider searches
- **Example output:**

  ```
  ripgrep
    Available from: Cargo v14.1.1, Binary v14.1.1, Conda v14.1.0
    A fast line-oriented search tool
  ```

- **AC:** Search returns grouped results showing package availability across providers. ✓

---

## Phase 5: Logic & Synchronization - 100% COMPLETE

**Goal:** The tool is smart (Adoption, Pruning, Sync) and syncs bidirectionally with git.

### Task 5.1: Interactive Init - DONE

- **Action:** Prompt user for provider priority during init.
- **Status:** Complete
- **Implemented:**
  - `--yes` flag skips prompts and uses defaults
  - Interactive mode (without `--yes`) uses `inquire` for prompts:
    - Multi-select for which package managers to bootstrap (uv, conda)
    - Confirmation before proceeding
  - Bootstrap struct has `set_install_uv()` and `set_install_conda()` methods
- **AC:** Interactive init prompts user for bootstrap options. ✓
- **AC:** `schalentier init --yes` uses defaults. ✓

### Task 5.2: Adoption Logic - DONE

- **Action:** Before install, check `which <tool>`. If found, adopt instead of install.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_add function, lines 151-188)
- **Implemented:**
  - `which::which(name)` checks if tool exists before installing
  - If exists and not in state, tool is "adopted" instead of installed
  - `get_binary_version()` helper extracts version from `--version` output
  - `detect_provider_from_path()` guesses provider from install path
  - Sets `managed: false` and `status: ToolStatus::Adopted`
- **AC:** `add grep` results in "Adopted" status, not installation. ✓

### Task 5.3: Remove Protection - DONE

- **Action:** Refuse to remove tools not managed by schalentier.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_remove function, lines 839-897)
- **Implemented:**
  - Checks `tool.managed` flag before uninstalling
  - Adopted tools (`managed: false`) cannot be uninstalled
  - Shows warning: "was not installed by schalentier (managed by X)"
  - `--keep-installed` flag allows removing from tracking without uninstall
- **Smoke test:** `tests/smoke/scripts/conflicts.sh`
- **AC:** Cannot remove OS-managed tools. Shows "managed by another tool" message. ✓

### Task 5.4: Bidirectional Git Sync - DONE

- **Action:** Implement full git-based config sync with pull and push.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_sync function)
- **Implemented:**
  - Config directory at `~/.config/schalentier/` (can be a git repo)
  - `sync --remote <url>` - clone remote into config dir
  - `sync` (no args) - bidirectional: pull then push
  - `sync --pull` - pull only
  - `sync --push` - push only
  - Auto git init if no repo exists
  - Auto add remote if specified
  - `git pull --rebase` with fallback to merge
  - Auto commit and push local changes
- **AC:** Full bidirectional sync flow works. ✓

### Task 5.5: Sync Apply Logic - DONE

- **Action:** After pulling config, install missing tools and optionally prune.
- **Status:** Complete
- **Implemented:**
  - After pull, compares pulled config vs local state
  - Installs tools in config but not in state
  - Uses `install_with_fallback` for provider fallback
  - Updates state with actual provider used
- **AC:** Tools from remote config get installed after sync. ✓

### Task 5.6: Pruning (Garbage Collection) - DONE

- **Action:** Compare State vs Config, call uninstall on orphaned tools.
- **Status:** Complete
- **Implemented:**
  - `sync --prune` flag added to CLI
  - After pull, identifies tools in state but not in config
  - Uninstalls orphaned tools via their provider
  - Updates state after pruning
- **AC:** `sync --prune` removes orphaned tools. ✓
- **AC:** `sync --prune` removes tools deleted from config.

---

## Phase 6: Polish & Advanced Features - COMPLETE

**Goal:** UX refinements, enhanced output, and deployment.

### Task 6.1: List Command Enhancement - DONE

- **Action:** Improve list output to show management status.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_list function, lines 771-898)
- **Implemented:**
  - Shows all tools from both config and state (merged view)
  - Ownership status: "Installed by schalentier", "Adopted (external)", "In config (not installed)", "Orphaned"
  - Compact view with symbols: `✓` installed, `~` adopted, `○` pending, `!` orphaned
  - Detailed view (`--detailed`) shows path, version, provider, accessibility
  - Accessibility check via `which` to verify tool is in PATH
  - Provider filter (`--provider`) works with actual installed provider
  - Legend displayed at bottom of compact view
- **Smoke test:** `tests/smoke/scripts/conflicts.sh`
- **AC:** `schalentier list` shows who manages each tool. ✓

### Task 6.2: Doctor Command Enhancement - DONE

- **Action:** Improve doctor output with comprehensive status.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_doctor function, lines 596-836)
- **Implemented:**
  - **Core Status section:** data dir, config dir, initialized, bootstrap (uv/conda)
  - **Available Providers section:** Shows all 6 providers and their availability
  - **Sync Status section:** Git repo status, remote URL, uncommitted changes count
  - **Tool Status section:** Config vs state counts, managed vs adopted breakdown
  - Orphaned tools list (in state but not config)
  - Missing tools list (in config but not state)
  - Inaccessible tools list (installed but not in PATH)
  - **Summary section:** Issues found/fixed counts
  - `--fix` flag can auto-create missing directories and env scripts
- **Smoke test:** `tests/smoke/scripts/doctor.sh`
- **AC:** `schalentier doctor` shows all system status info. ✓

### Task 6.3: Update Command - DONE

- **Action:** Implement tool update functionality.
- **Status:** Complete
- **Files:** `src/main.rs` (cmd_update function, lines 564-724), `src/provider/mod.rs` (latest_version method)
- **Implemented:**
  - `schalentier update` - check and update all managed tools
  - `schalentier update <name>` - update specific tool
  - `schalentier update --dry-run` - show available updates without installing
  - `latest_version()` method on Installer trait queries providers
  - `version_is_newer()` helper compares semver-like versions
  - Skips adopted tools (not managed by schalentier)
  - Updates state with new version after successful update
  - Shows summary of updates available/applied/failed
- **Smoke test:** `tests/smoke/scripts/sync.sh` (includes update test)
- **AC:** Can check for and apply updates. ✓

### Task 6.4: UI Polish - DONE (Spinners)

- **Action:** Add spinners (indicatif) for long-running operations.
- **Status:** Complete
- **Dependencies:** indicatif (enabled in Cargo.toml)
- **Implementation:**
  - Added `create_spinner()` helper function in main.rs
  - Spinners added to: install, search, sync (clone/pull/push)
  - Uses braille spinner pattern with cyan color
  - Spinners auto-tick and clear on completion
- **AC:** Long operations show spinner. ✓

### Task 6.5: Dotfiles/Config Patching System - DONE

- **Action:** Implement intelligent config file patching with format-aware parsing.
- **Status:** Complete
- **Files to create:** `src/dotfiles.rs`, `src/dotfiles/` module
- **Design Philosophy:**
  - Users define only the settings they want to change (not complete files)
  - Schalentier parses the target file, updates/appends settings intelligently
  - Decoupled from tools - `[dotfiles]` is independent section
- **Config Syntax:**

  ```toml
  # Dotfiles section - independent of tools
  [dotfiles]

  # JSON file - auto-detected, deep merged
  [dotfiles."~/.config/micro/settings.json"]
  colorscheme = "monokai"
  tabsize = 4

  # TOML file - deep merged
  [dotfiles."~/.config/starship.toml"]
  [dotfiles."~/.config/starship.toml".character]
  success_symbol = "[➜](bold green)"

  # INI file - section-aware merge
  [dotfiles."~/.gitconfig"]
  [dotfiles."~/.gitconfig".user]
  name = "John Doe"
  email = "john@example.com"
  [dotfiles."~/.gitconfig".core]
  editor = "micro"

  # KeyValue file (.env style)
  [dotfiles."~/.env"]
  EDITOR = "micro"
  PAGER = "less"

  # Unknown format - replace mode (user provides complete content)
  [dotfiles."~/.vimrc"]
  _content = """
  set number
  set tabstop=4
  syntax on
  """

  # Override auto-detection
  [dotfiles."~/.config/custom/config"]
  _format = "toml"
  some_key = "value"
  ```

- **Supported Formats (MVP):**

  | Format | Extension/Detection | Parser | Merge Strategy |
  |--------|---------------------|--------|----------------|
  | JSON | `.json` | serde_json | Deep merge (RFC 7386) |
  | TOML | `.toml` | toml crate | Deep merge |
  | YAML | `.yaml`, `.yml` | serde_yaml | Deep merge |
  | INI | `.ini`, `.gitconfig`, `.conf` | rust-ini | Section-key merge |
  | KeyValue | `.env`, `KEY=VALUE` pattern | Custom (trivial) | Line-based update/append |
  | Unknown | Everything else | None | Replace mode |

- **Future Formats (V1.1):**

  | Format | Tools | Parser Strategy |
  |--------|-------|-----------------|
  | Vim | vim, neovim | `set X=Y` pattern matching |
  | Tmux | tmux | `set -g X Y` pattern matching |
  | SSH | openssh | `Host` block parser |

- **Format Detection:**

  ```rust
  fn detect_format(path: &Path) -> ConfigFormat {
      // By extension
      match extension {
          "json" => Json,
          "toml" => Toml,
          "yaml" | "yml" => Yaml,
          "ini" | "conf" => Ini,
          "env" => KeyValue,
          _ => {}
      }
      // By known filename
      match filename {
          ".gitconfig" => Ini,
          ".env" | ".env.*" => KeyValue,
          "settings.json" => Json,
          "starship.toml" => Toml,
          _ => Unknown  // → replace mode
      }
  }
  ```

- **Merge Behavior:**
  - **Structured formats (JSON/TOML/YAML/INI):** Deep merge - only specified keys are touched
  - **KeyValue:** Find existing key and update, or append new line
  - **Unknown/Replace:** Overwrite entire file with provided content
- **Safety Features:**
  - Backup before first modification: `~/.vimrc.schalentier-backup`
  - Idempotent: Running twice produces same result
  - Dry-run mode: Show diff without applying
- **Commands:**

  ```bash
  schalentier config apply          # Apply all dotfile patches
  schalentier config diff           # Show what would change (dry-run)
  schalentier config reset <file>   # Restore from backup
  schalentier config list           # List managed dotfiles
  ```

- **Integration:**
  - `schalentier sync` applies config patches after pulling
  - `schalentier doctor` checks for config drift
- **Dependencies:** serde_yaml (new), rust-ini or configparser (new)
- **AC:** JSON merge creates/updates file correctly. ✓
- **AC:** TOML merge works with nested structures. ✓
- **AC:** INI merge handles sections correctly. ✓
- **AC:** KeyValue update/append works. ✓
- **AC:** Unknown format uses replace mode. ✓
- **AC:** Backup created before first modification. ✓
- **AC:** `config diff` shows changes without applying. ✓

### Task 6.6: The `install.sh` Script - DONE

- **Action:** Create one-liner installation script.
- **Status:** Complete
- **Files:** `install.sh`
- **Implemented:**
  - Auto-detects OS (Linux, macOS, Windows) and architecture (x86_64, aarch64)
  - Downloads appropriate binary from GitHub releases
  - Supports `SCHALENTIER_VERSION` env var for specific versions
  - Supports `SCHALENTIER_INSTALL_DIR` env var for custom install location
  - Supports `SCHALENTIER_NO_INIT` to skip initialization
  - Automatically runs `schalentier init` after install
  - Shows PATH setup instructions if needed
- **AC:** `curl ... | sh` installs schalentier on fresh system. ✓

### Task 6.7: Alias Command - DONE

- **Action:** Implement `schalentier alias` to create shell script aliases in bin/.
- **Status:** Complete
- **Files modified:** `src/cli.rs`, `src/main.rs`
- **Design:** Non-intrusive approach - creates executable scripts instead of modifying shell configs
- **Commands:**
  - `schalentier alias lt="ls -ltrh"` - Create alias
  - `schalentier alias --list` - List all aliases
  - `schalentier alias --remove lt` - Remove alias
- **Implementation:**

  ```bash
  # ~/.schalentier/bin/lt
  #!/bin/sh
  exec ls -ltrh "$@"
  ```

- **Benefits:**
  - Works across all shells (bash, zsh, fish, powershell)
  - No shell config modification needed
  - Scripts are portable and version-controllable
- **AC:** `schalentier alias ll="ls -la"` creates working `ll` command. ✓

### Task 6.8: Snippets System (Shell Integration) - DONE

- **Action:** Implement snippets for tools requiring shell sourcing (yazi, zoxide, fzf, etc.)
- **Status:** Complete
- **Files modified:** `src/main.rs`, `src/cli.rs`
- **Design:** Hybrid approach with built-in registry + user-defined snippets
- **Directory Structure:**

  ```
  ~/.schalentier/
  ├── bin/                    # Binaries and alias scripts
  ├── snippets.d/             # Source-required snippets
  │   ├── yazi.bash           # yy() directory-change wrapper
  │   ├── zoxide.bash         # eval "$(zoxide init bash)"
  │   └── fzf.bash            # eval "$(fzf --bash)"
  └── env.sh                  # Modified to source snippets.d/*.bash
  ```

- **Commands:**
  - `schalentier snippet add yazi` - Add snippet from built-in registry
  - `schalentier snippet add --file custom.sh` - Add custom snippet
  - `schalentier snippet remove yazi` - Remove snippet
  - `schalentier snippet list` - List installed snippets
- **Built-in Snippet Registry:**

  | Tool | Snippet Purpose |
  |------|-----------------|
  | yazi | `yy()` function for directory change on exit |
  | zoxide | `z` smart cd command |
  | fzf | Ctrl+R/Ctrl+T keybindings |
  | direnv | Directory environment hook |
  | starship | Prompt initialization |
  | atuin | Shell history replacement |

- **Interactive Prompt:** When installing a tool with known snippet, ask user:

  ```
  Tool 'yazi' has a recommended shell snippet (yy function for directory change).
  Install snippet? [Y/n]
  ```

- **Config Support:**

  ```toml
  [tools.yazi]
  provider = "binary"
  snippet = true              # Use built-in snippet

  [tools.custom-tool]
  provider = "cargo"
  [tools.custom-tool.snippet]
  bash = "eval \"$(custom-tool init bash)\""
  fish = "custom-tool init fish | source"
  ```

- **env.sh modification:**

  ```bash
  # Source all shell snippets
  if [ -d "$SCHALENTIER_DATA_DIR/snippets.d" ]; then
      for snippet in "$SCHALENTIER_DATA_DIR/snippets.d"/*.bash; do
          [ -f "$snippet" ] && . "$snippet"
      done
  fi
  ```

- **AC:** Installing yazi prompts for snippet, creates yy() function. ✓
- **AC:** `schalentier snippet list` shows installed snippets. ✓
- **AC:** Custom snippets work from config. ✓

---

## Phase 7: Advanced Features - TODO

**Goal:** Secrets management, templating, and project-local configuration.

### Task 7.1: Secrets Management - TODO

- **Priority:** HIGH (foundation for templating)
- **Dependencies:** `age` (encryption), `keyring` (system keyring)
- **Files:** `src/secrets.rs`

**Architecture:**

```
~/.config/schalentier/
├── schalentier.toml      # Config (synced via git)
├── secrets.enc           # Encrypted secrets (synced via git - safe!)
└── state.json            # Local state (not synced)

System Keyring:
└── schalentier/master-password  # Decryption key (local per machine)
```

**Encryption:**

- Use `age` encryption (modern, simple, audited)
- Master password encrypts/decrypts secrets.enc
- Master password stored in OS keyring after first use:
  - Linux: Secret Service (GNOME Keyring / KWallet)
  - macOS: Keychain
  - Windows: Credential Manager

**Secret File Format (before encryption):**

```json
{
  "GITHUB_TOKEN": "ghp_xxxxxxxxxxxx",
  "AWS_ACCESS_KEY_ID": "AKIAIOSFODNN7EXAMPLE",
  "AWS_SECRET_ACCESS_KEY": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
}
```

**Commands:**

```bash
schalentier secret set <NAME> [--value <VALUE>]   # Interactive if no --value
schalentier secret get <NAME>                      # Print to stdout (no newline)
schalentier secret list                            # List names only
schalentier secret delete <NAME>                   # Remove secret
schalentier secret export [--shell bash|fish|pwsh] # Output for eval/source
schalentier secret edit                            # Decrypt → $EDITOR → re-encrypt
schalentier secret change-password                 # Re-encrypt with new password
```

**UX Flow:**

```bash
# First time on Machine A
$ schalentier secret set GITHUB_TOKEN
Value: ****
Create master password: ****
Confirm master password: ****
✓ Secret 'GITHUB_TOKEN' saved
✓ Master password stored in system keyring

# Subsequent use on Machine A (no password prompt!)
$ schalentier secret set AWS_KEY
Value: ****
✓ Secret 'AWS_KEY' saved

# Sync to Machine B
$ schalentier sync --push   # On Machine A
$ schalentier sync --pull   # On Machine B (pulls secrets.enc)

# First use on Machine B
$ schalentier secret get GITHUB_TOKEN
Master password: ****
✓ Master password stored in system keyring
ghp_xxxxxxxxxxxx

# Subsequent use on Machine B (no password prompt!)
$ schalentier secret list
GITHUB_TOKEN
AWS_KEY
```

**Shell Integration:**

```bash
# Export all secrets as environment variables
$ eval "$(schalentier secret export)"
$ echo $GITHUB_TOKEN
ghp_xxxxxxxxxxxx

# Fish shell
$ schalentier secret export --shell fish | source

# PowerShell
PS> Invoke-Expression (schalentier secret export --shell pwsh)
```

**Auto-export in env.sh (opt-in via config):**

```toml
# schalentier.toml
[settings]
auto_export_secrets = true
```

Generated env.sh:

```bash
# Auto-export secrets
if [ -f "$HOME/.config/schalentier/secrets.enc" ]; then
    eval "$(schalentier secret export 2>/dev/null)" || true
fi
```

- **AC:** Secrets encrypted with age, decryptable with password
- **AC:** Password stored in keyring after first use
- **AC:** `secret export` outputs valid shell syntax
- **AC:** secrets.enc can be synced via git safely

---

### Task 7.2: Templating Engine - TODO

- **Priority:** HIGH (enables dynamic configs)
- **Dependencies:** `minijinja` (Jinja2-compatible, pure Rust)
- **Files:** `src/template.rs`, modifications to `src/dotfiles.rs`

**Template Engine:** minijinja (Jinja2 syntax)

**Context Variables:**

```
{{ os }}              → "linux" | "macos" | "windows"
{{ arch }}            → "x86_64" | "aarch64"
{{ hostname }}        → machine hostname
{{ username }}        → current user
{{ home }}            → home directory path
{{ env.VARNAME }}     → environment variable
{{ secret.NAME }}     → decrypted secret (requires 7.1)
{{ var.NAME }}        → user-defined variable from [variables] section
```

**User-Defined Variables:**

```toml
# schalentier.toml

[variables]
work_email = "ada@company.com"
personal_email = "ada@personal.dev"
default_editor = "nvim"
git_signing_key = "ABC123"

# Can also be nested
[variables.work]
email = "ada@company.com"
name = "Ada Lovelace (Company)"

[variables.personal]
email = "ada@personal.dev"
name = "Ada Lovelace"
```

Access in templates:

```
{{ var.work_email }}           → "ada@company.com"
{{ var.work.email }}           → "ada@company.com" (nested)
{{ var.default_editor }}       → "nvim"
```

**Enabling Templates:**

```toml
[dotfiles."~/.gitconfig".user]
_template = true                    # Enable templating for this dotfile
name = "{{ var.work.name }}"
email = "{% if hostname == 'work-laptop' %}{{ var.work_email }}{% else %}{{ var.personal_email }}{% endif %}"
signingkey = "{{ var.git_signing_key }}"

[dotfiles."~/.config/gh/hosts.yml"."github.com"]
_template = true
oauth_token = "{{ secret.GITHUB_TOKEN }}"

[dotfiles."~/.ssh/config"]
_template = true
_content = """
Host *
    IdentityFile ~/.ssh/{% if os == 'macos' %}id_ed25519{% else %}id_rsa{% endif %}

Host github.com
    HostName github.com
    User git
    IdentityFile ~/.ssh/github_{{ hostname }}
"""
```

**Processing Flow:**

1. Load dotfile entry
2. If `_template = true`:
   a. Build context (os, arch, hostname, username, home, env, secrets)
   b. For each string value, render as Jinja2 template
   c. Apply rendered values to target file
3. If `_template` not set or false:
   a. Apply values directly (current behavior)

**Error Handling:**

```
Error: Template error in ~/.gitconfig
  → Undefined variable 'hotsname' (did you mean 'hostname'?)

Error: Template error in ~/.config/gh/hosts.yml
  → Secret 'GITHUB_TOKEN' not found
    Run: schalentier secret set GITHUB_TOKEN
```

- **AC:** `{{ hostname }}` renders to actual hostname
- **AC:** `{{ secret.NAME }}` renders to decrypted secret
- **AC:** Undefined variables produce helpful error messages
- **AC:** Non-templated dotfiles work unchanged

---

### Task 7.3: Project-Local Configuration - TODO

- **Priority:** MEDIUM (after 7.1 and 7.2)
- **Files:** Modifications to config loading in `src/config.rs`, `src/state.rs`

**Detection:**

- Walk up from `$PWD` looking for `.schalentier/config.toml` or `.schalentier.toml`
- Stop at filesystem root or home directory
- If found, merge with global config (project wins)

**Directory Structure:**

```
~/projects/my-app/
├── .schalentier/
│   ├── config.toml       # Project config (committed to project repo)
│   └── secrets.enc       # Project secrets (add to .gitignore)
├── .gitignore            # Contains: .schalentier/secrets.enc
└── src/
```

**Project Config Example:**

```toml
# ~/projects/legacy-app/.schalentier/config.toml

[tools]
node = { provider = "conda", version = "18" }  # Override global
python = { version = "3.9" }                    # Pin for this project

[dotfiles."~/.npmrc"]
_template = true
registry = "https://npm.company.com"
//npm.company.com/:_authToken = "{{ secret.NPM_TOKEN }}"
```

**Merge Behavior:**

```
Global config:                    Project config:
[tools]                          [tools]
ripgrep = {}                     node = { version = "18" }
node = { version = "22" }

           ↓ merge (project wins) ↓

Effective config:
[tools]
ripgrep = {}                     # From global
node = { version = "18" }        # From project (overridden!)
```

**Project Secrets:**

- Same master password as global (one password to remember)
- Project secrets in `.schalentier/secrets.enc`
- Project secrets override global secrets with same name

```bash
$ cd ~/projects/my-app

$ schalentier secret set DB_PASSWORD      # → .schalentier/secrets.enc
$ schalentier secret set --global API_KEY # → ~/.config/schalentier/secrets.enc

$ schalentier secret list
Project secrets (.schalentier/secrets.enc):
  DB_PASSWORD

Global secrets (~/.config/schalentier/secrets.enc):
  API_KEY
  GITHUB_TOKEN
```

**Commands in Project Context:**

```bash
$ cd ~/projects/my-app
$ schalentier doctor
...
Project config: ~/projects/my-app/.schalentier/
  Tools overridden: node (v18)
  Project secrets: 1
...
```

- **AC:** Project config detected when running from project directory
- **AC:** Project config overrides global config
- **AC:** Project secrets are separate but use same master password
- **AC:** `doctor` shows project context when applicable

---

## Implementation Summary

| Phase | Coverage | Status | Key Gaps |
|-------|----------|--------|----------|
| Phase 1 | 100% | DONE | - |
| Phase 2 | 100% | DONE | - |
| Phase 3 | 100% | DONE | - |
| Phase 4 | 100% | COMPLETE | Search clustering added ✓ |
| Phase 5 | 100% | COMPLETE | Interactive init with inquire ✓ |
| Phase 6 | 100% | COMPLETE | Spinners, aliases, snippets, dotfiles all done ✓ |
| Phase 7 | 0% | TODO | Secrets, Templating, Project-local config |

## Files Structure

```
src/
├── main.rs          # CLI entry point, command implementations
├── lib.rs           # Module exports
├── archive.rs       # Archive extraction (tar.gz, zip - pure Rust)
├── cli.rs           # Clap argument parsing
├── config.rs        # Data structures (SchalentierConfig, LocalState, Provider enum)
├── state.rs         # State persistence (load/save)
├── error.rs         # Error types and pretty printing
├── logging.rs       # Tracing setup
├── bootstrap.rs     # Architecture detection, uv/Miniforge installation
├── shell.rs         # Shell script generation
├── dotfiles.rs      # Dotfile patching (JSON, TOML, YAML, INI, KeyValue)
├── secrets.rs       # Secrets management (age encryption, keyring) [Phase 7]
├── template.rs      # Templating engine (minijinja) [Phase 7]
└── provider/
    ├── mod.rs       # Installer trait, ProviderRegistry, install_with_fallback
    ├── binary.rs    # GitHub Releases provider [DONE]
    ├── cargo.rs     # Cargo/crates.io provider [DONE]
    ├── system.rs    # System package manager provider (apt/pacman/dnf/etc) [DONE]
    ├── conda.rs     # Conda/Mamba provider [DONE]
    ├── brew.rs      # Homebrew/Linuxbrew provider [DONE]
    ├── uv.rs        # UV/PyPI provider [DONE]
    └── mock.rs      # Mock provider for testing
```

## Phase 7 Dependencies

```toml
# To be added to Cargo.toml for Phase 7

# Secrets Management (Task 7.1)
age = "0.10"              # age encryption (pure Rust)
keyring = "3"             # Cross-platform system keyring

# Templating (Task 7.2)
minijinja = "2"           # Jinja2-compatible template engine
```

## Smoke Tests

```
tests/smoke/
├── Dockerfile           # Multi-stage (Debian + Arch)
├── run.sh               # Test runner (builds containers and runs tests)
└── run_smoke_tests.sh   # Test script (runs inside containers)
```

## Test Results

### Unit Tests

```
running 90 tests (86 lib + 4 main)
test result: ok. 90 passed; 0 failed; 0 ignored
```

### Smoke Tests

```
58 tests: Basic CLI, Init, Doctor, Search, List, Provider Detection,
         Shell Integration, Sync, Add/Install, Update, Alias, Snippets,
         Dotfiles/Config Patching, Error Handling, Cleanup
test result: ok. 58 passed; 0 failed; 1 skipped
```

---

## Next Priority Tasks

### Milestone 1: Core Providers - COMPLETE ✓

1. ~~**Task 4.5: Conda Provider** - mamba search/install~~ ✓
2. ~~**Task 4.6: Brew Provider** - brew search/install~~ ✓
3. ~~**Task 4.7: UV Provider** - uv tool install~~ ✓
4. ~~**Task 4.8: Provider Fallback** - try alternatives when preferred unavailable~~ ✓
5. ~~**Task 4.9: Search Provider Filter** - `--provider` flag~~ ✓

### Milestone 2: Sync (makes multi-machine workflow work) - COMPLETE ✓

6. ~~**Task 5.1: Interactive Init** - `--yes` flag + inquire prompts~~ ✓
2. ~~**Task 5.4: Bidirectional Git Sync** - pull/push with git~~ ✓
3. ~~**Task 5.5: Sync Apply Logic** - install after pull~~ ✓
4. ~~**Task 5.6: Pruning** - `sync --prune` removes orphaned tools~~ ✓

### Milestone 3: Safety & Status - COMPLETE ✓

10. ~~**Task 5.2: Adoption Logic** - detect pre-installed tools~~ ✓
2. ~~**Task 5.3: Remove Protection** - refuse to remove OS tools~~ ✓
3. ~~**Task 6.1: List Enhancement** - show managed-by status~~ ✓
4. ~~**Task 6.2: Doctor Enhancement** - comprehensive status output~~ ✓

### Milestone 4: Polish - COMPLETE ✓

14. ~~**Task 6.3: Update Command** - check/apply updates~~ ✓
2. ~~**Task 6.4: UI Polish** - spinners added~~ ✓
3. ~~**Task 6.6: install.sh** - one-liner installation~~ ✓
4. ~~**Task 4.10: Search Clustering** - grouped search results~~ ✓

### Milestone 5: Shell Integration - COMPLETE ✓

17. ~~**Task 6.7: Alias Command** - non-intrusive alias scripts in bin/~~ ✓
2. ~~**Task 6.8: Snippets System** - source-required shell integrations~~ ✓
    - Built-in snippet registry (yazi, zoxide, fzf, direnv, starship, atuin)
    - Custom snippet support via `--file` flag

### Milestone 6: Config Synchronization - COMPLETE ✓

19. ~~**Task 6.5: Dotfiles/Config Patching System** - intelligent config file patching~~ ✓
    - Format-aware parsing (JSON, TOML, YAML, INI, KeyValue)
    - Deep merge for structured formats
    - Replace mode for unknown formats
    - Backup and restore functionality
    - `schalentier config apply/diff/list/reset` commands

### Milestone 7: Advanced Features - TODO

20. **Task 7.1: Secrets Management** - encrypted secrets with keyring integration
    - `age` encryption for secrets.enc (synced via git)
    - System keyring for master password (local per machine)
    - Commands: set, get, list, delete, export, edit, change-password
    - Shell export: `eval "$(schalentier secret export)"`

2. **Task 7.2: Templating Engine** - Jinja2-style templates in dotfiles
    - `minijinja` for template rendering
    - Context: os, arch, hostname, username, home, env.*, secret.*
    - `_template = true` flag enables templating per dotfile

3. **Task 7.3: Project-Local Configuration** - per-project overrides
    - Detect `.schalentier/config.toml` in project directories
    - Merge with global config (project wins)
    - Project-local secrets (same master password)

---

## Pre-Release Checklist

### Bugs to Fix Before v1.0

| Bug | Severity | Status |
|-----|----------|--------|
| **install.sh expects .tar.gz** but release workflow uploads raw binaries | Critical | TODO (fix when releasing) |
| `ripgrep-all` shows `vv0.10.10` - double v prefix in version | Medium | DONE - added `format_version()` helper |
| `--provider` help lists incomplete providers | Minor | DONE - updated help text |

### UX Improvements

| Feature | Priority | Status |
|---------|----------|--------|
| Shell completions (`schalentier completions bash/zsh/fish`) | High | DONE - uses clap_complete |
| `init --skip-bootstrap` | Medium | DONE - skips uv/conda install |
| `add --dry-run` | Medium | DONE - shows what would happen |
| `sync --dry-run` | Medium | DONE - shows sync status |
| Self-update mechanism | Low | Future |
| `GITHUB_TOKEN` env var support | Low | Future (rate limits handled gracefully) |

## CLI Changes Needed

| Command | Current | Needed |
|---------|---------|--------|
| `init` | `--force`, `--yes`, `--skip-bootstrap` | ✓ Complete |
| `add` | `--provider`, `--no-install`, `--dry-run` | ✓ Complete |
| `sync` | `--remote`, `--push`, `--pull`, `--prune`, `--dry-run` | ✓ Complete |
| `search` | `--limit`, `--provider` | ✓ Complete |
| `list` | `--detailed`, `--provider` | ✓ Complete |
| `alias` | `NAME="CMD"`, `--list`, `--remove` | ✓ Complete |
| `snippet` | `add`, `remove`, `list`, `--file` | ✓ Complete |
| `config` | `apply`, `diff`, `reset`, `list` | ✓ Complete |
| `completions` | `bash/zsh/fish/elvish/powershell` | ✓ Complete |

## Provider Status

| Provider | Enum | Implementation | Smoke Test |
|----------|------|----------------|------------|
| Binary | ✓ | ✓ DONE | binary-provider.sh |
| Cargo | ✓ | ✓ DONE | cargo-provider.sh |
| System | ✓ | ✓ DONE | apt-provider.sh, pacman-provider.sh |
| Conda | ✓ | ✓ DONE | conda-provider.sh |
| UV | ✓ | ✓ DONE | uv-provider.sh |
| Brew | ✓ | ✓ DONE | brew-provider.sh |
