# Schalentier: Windows Development Tasks

This document outlines what can be fully implemented and tested on Windows.

**Last Updated:** 2024-12-23

---

## Phase 1: The Skeleton & CLI Foundation (100% on Windows) - COMPLETED

### Task 1.1: Project Initialization - DONE
- **Action:** Run `cargo new`, configure `Cargo.toml` with dependencies
- **Status:** Completed
- **Files:** `Cargo.toml`, `.cargo/config.toml`
- **Dependencies:** clap, tokio, anyhow, thiserror, tracing, tracing-subscriber, serde, serde_json, toml, reqwest, dirs, async-trait, futures-util, urlencoding

### Task 1.2: CLI Argument Parsing - DONE
- **Action:** Define `clap` structs for `init`, `add`, `sync`, `update`, `doctor`
- **Status:** Completed
- **Files:** `src/cli.rs`
- **Commands:** init, add, sync, update, doctor, remove, list, search
- **AC:** `schalentier --help` shows correct menu, `schalentier add` without args returns error

### Task 1.3: Logging & Error Handling - DONE
- **Action:** Setup `tracing-subscriber`, implement global error handler
- **Status:** Completed
- **Files:** `src/logging.rs`, `src/error.rs`
- **AC:** `RUST_LOG=debug schalentier` shows logs, errors display cleanly with colors

---

## Phase 2: Configuration & State (95% on Windows) - COMPLETED

### Task 2.1: Data Models (Structs) - DONE
- **Action:** Create `config.rs`, define `SchalentierConfig` and `LocalState` structs
- **Status:** Completed
- **Files:** `src/config.rs`
- **Structs:** SchalentierConfig, LocalState, Settings, ToolEntry, InstalledTool, BootstrapState, SyncConfig
- **AC:** Unit tests pass for serialization/deserialization

### Task 2.2: Local State Management - DONE
- **Action:** Implement `LocalState::load()` and `save()`
- **Status:** Completed
- **Files:** `src/state.rs`
- **Features:** Directory creation, JSON persistence, Unix permissions via `#[cfg(unix)]`
- **AC:** Creates state directory, saves/loads JSON

### Task 2.3: Provider Priority Configuration - DONE
- **Action:** Add `priority` list to Settings, implement merge logic
- **Status:** Completed
- **Files:** `src/config.rs`, `src/state.rs`
- **AC:** Configuration overrides default provider order

---

## Phase 3: The "Brain" (Bootstrap & Shells) (30% on Windows) - STRUCTURE COMPLETE

### Task 3.1: Architecture Detection - DONE
- **Action:** Implement `bootstrap::get_arch()`
- **Status:** Completed
- **Files:** `src/bootstrap.rs`
- **AC:** Returns correct architecture enum (X86_64, Aarch64)

### Task 3.2: Miniforge & Tool Bootstrap - STRUCTURE DONE
- **Action:** Implement download logic and installer invocation structure
- **Status:** Structure complete, execution requires Linux
- **Files:** `src/bootstrap.rs`
- **Features:**
  - Download URLs for Miniforge, uv (all platforms)
  - `download_file()` async function
  - `Bootstrap` orchestrator with `run()` method
- **Remaining:** Actual installer execution (Unix shell scripts)

### Task 3.3: Shell Script Generation - DONE
- **Action:** Implement template generation for `env.sh`, `env.fish`, `env.ps1`
- **Status:** Completed
- **Files:** `src/shell.rs`
- **Features:**
  - `generate_bash_env()`, `generate_fish_env()`, `generate_powershell_env()`
  - `write_env_scripts()` - writes all three scripts
  - `shell_init_snippet()` - generates user instructions
- **AC:** Generates correct file content for all shells

---

## Phase 4: The Provider Engine (60% on Windows) - PARTIAL

### Task 4.1: The Installer Trait - DONE
- **Action:** Define `Installer` async trait
- **Status:** Completed
- **Files:** `src/provider/mod.rs`
- **Methods:** `search()`, `install()`, `uninstall()`, `is_installed()`, `installed_version()`
- **AC:** MockProvider implemented and tested

### Task 4.2: System & Conda Providers - NOT STARTED
- **Action:** Implement `System` and `Conda` provider structs
- **Status:** Not started (requires Linux for testing)
- **Notes:** Could add Windows support (winget/chocolatey) as extension

### Task 4.3: Binary Provider (GitHub Releases) - DONE
- **Action:** Implement `Binary` provider using GitHub Releases API
- **Status:** Completed
- **Files:** `src/provider/binary.rs`
- **Features:**
  - GitHub repository search
  - Latest release fetching
  - Platform-specific asset matching (prefers musl for Linux)
  - Asset download
- **AC:** Can search GitHub releases, download assets

### Task 4.3b: Cargo Provider - NOT STARTED
- **Action:** Implement `Cargo` provider
- **Status:** Not started

### Task 4.4: Search Aggregation (Clustering) - NOT STARTED
- **Action:** Implement parallel search, grouping, Jaccard Index calculation
- **Status:** Not started
- **Notes:** Pure Rust logic, can be done on Windows

### Task 4.5: Interactive Installation - PARTIAL
- **Action:** Implement `install()` with `Command` and `Stdio::inherit()`
- **Status:** Download works, extraction not implemented
- **Remaining:** Archive extraction (.tar.gz, .zip), `sudo` prompts (Linux)

---

## Phase 5: Logic & Synchronization (70% on Windows) - NOT STARTED

### Task 5.1: Adoption Logic - NOT STARTED
- **Action:** Check if tool exists before installing
- **Status:** Not started

### Task 5.2: Sync Adapters - NOT STARTED
- **Action:** Implement `GitSSH` and `HttpReadOnly` adapters
- **Status:** Not started

### Task 5.3: Structured Merging - NOT STARTED
- **Action:** Implement `Merger::merge(local, remote)`
- **Status:** Not started
- **Notes:** Pure Rust logic, can be done on Windows

### Task 5.4: Pruning (Garbage Collection) - NOT STARTED
- **Action:** Compare State vs Config, call uninstall
- **Status:** Not started

---

## Phase 6: Polish & Advanced Features (50% on Windows) - NOT STARTED

### Task 6.1: Dotfile Patcher - NOT STARTED
### Task 6.2: Secrets Management - NOT STARTED
### Task 6.3: UI Polish - NOT STARTED
### Task 6.4: The `install.sh` Script - NOT STARTED (Linux only)

---

## Implementation Summary

| Phase | Coverage | Status | Notes |
|-------|----------|--------|-------|
| Phase 1 | 100% | DONE | CLI, logging, errors |
| Phase 2 | 100% | DONE | Config, state, serialization |
| Phase 3 | 80% | DONE | Structure complete, execution needs Linux |
| Phase 4 | 40% | PARTIAL | Binary provider done, others pending |
| Phase 5 | 0% | NOT STARTED | Sync logic |
| Phase 6 | 0% | NOT STARTED | Polish |

## Files Created

```
src/
├── main.rs          # CLI entry point, command implementations
├── lib.rs           # Module exports
├── cli.rs           # Clap argument parsing
├── config.rs        # Data structures (SchalentierConfig, LocalState)
├── state.rs         # State persistence (load/save)
├── error.rs         # Error types and pretty printing
├── logging.rs       # Tracing setup
├── bootstrap.rs     # Architecture detection, download logic
├── shell.rs         # Shell script generation
└── provider/
    ├── mod.rs       # Installer trait, ProviderRegistry
    ├── binary.rs    # GitHub Releases provider
    └── mock.rs      # Mock provider for testing
```

## Test Results

```
running 50 tests
test result: ok. 50 passed; 0 failed; 0 ignored
```

## Cross-Compilation Setup (Windows)

```powershell
# Install Zig (for cross-compilation linker)
winget install zig.zig

# Or via scoop
scoop install zig

# Install cargo-zigbuild
cargo install cargo-zigbuild

# Add musl targets
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl

# Build for Linux
cargo zigbuild --target x86_64-unknown-linux-musl --release
```
