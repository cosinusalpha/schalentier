# Schalentier: Linux-Required Tasks

This document outlines tasks that require a Linux environment for implementation or testing.

**Last Updated:** 2024-12-23

---

## Status Overview

| Task | Priority | Status | Depends On |
|------|----------|--------|------------|
| Binary verification | High | READY TO TEST | Cross-compiled binary |
| File permissions | Medium | READY TO TEST | State module done |
| Miniforge install | High | READY TO TEST | Bootstrap download done |
| uv install | High | READY TO TEST | Bootstrap download done |
| Shell script sourcing | High | READY TO TEST | Shell scripts done |
| System provider | High | NOT STARTED | Trait defined |
| Conda provider | Medium | NOT STARTED | Trait defined |
| sudo prompts | Medium | NOT STARTED | - |
| Git SSH adapter | Medium | NOT STARTED | - |
| install.sh script | High | NOT STARTED | - |

---

## Phase 1: Binary Verification - READY TO TEST

### Task 1.1: Verify musl Build
- **Why Linux:** Must verify cross-compiled binary actually runs
- **Action:** Test the binary built on Windows runs on Linux
- **Test Commands:**
  ```bash
  # Copy binary from Windows build
  ./schalentier --help
  ./schalentier doctor
  ./schalentier add ripgrep --no-install
  ```
- **AC:** Binary runs on Linux, all commands work

---

## Phase 2: Configuration & State - READY TO TEST

### Task 2.2: File Permissions Testing
- **Why Linux:** Unix file permissions (`chmod 0o600`) only work on Unix systems
- **Action:** Test that `~/.schalentier/local_state.json` has correct permissions
- **Implementation:** Already done with `#[cfg(unix)]` in `src/state.rs:48-54`
- **Test Commands:**
  ```bash
  ./schalentier init
  ls -la ~/.schalentier/local_state.json
  # Should show: -rw------- (600)
  ```
- **AC:** State file has `0600` permissions

---

## Phase 3: Bootstrap - READY TO TEST

### Task 3.2: Miniforge & Tool Bootstrap
- **Why Linux:** Installers are Unix shell scripts
- **Status:** Download logic complete, execution needs testing
- **Action:**
  1. Test Miniforge installer download
  2. Test installer execution in batch mode
  3. Verify conda works after install
- **Test Commands:**
  ```bash
  ./schalentier init
  # Should download and run Miniforge installer
  ~/.schalentier/conda/bin/conda --version
  ```
- **Implementation needed:** Execute installer in `src/bootstrap.rs:195-210`
  ```rust
  // Linux/macOS execution:
  Command::new("bash")
      .arg(&installer_path)
      .arg("-b")  // batch mode
      .arg("-p")
      .arg(&self.paths.conda_dir)
      .status()?;
  ```
- **AC:** `~/.schalentier/conda/bin/conda` exists and works

### Task 3.2b: uv Bootstrap
- **Status:** Download logic complete, extraction needs implementation
- **Action:** Extract tar.gz and install binary
- **Implementation needed:** Archive extraction in `src/bootstrap.rs:175-190`
- **AC:** `~/.schalentier/bin/uv --version` works

### Task 3.3: Shell Script Sourcing
- **Why Linux:** Need to test `source env.sh` in actual shells
- **Status:** Script generation complete
- **Test Commands:**
  ```bash
  # Bash/Zsh
  source ~/.schalentier/env.sh
  which uv
  which conda

  # Fish
  source ~/.schalentier/env.fish
  which uv
  ```
- **AC:** After sourcing, `which uv` returns schalentier-managed path

---

## Phase 4: Provider Engine - NOT STARTED

### Task 4.2: System Provider
- **Why Linux:** Needs `apt`, `pacman`, `dnf`, etc.
- **Status:** Not started
- **Action:**
  1. Detect package manager (check for apt, pacman, dnf, apk)
  2. Implement search (parse output)
  3. Implement install (with sudo)
- **Files to create:** `src/provider/system.rs`
- **Test Environments:**
  - Ubuntu/Debian: `apt`
  - Arch Linux: `pacman`
  - Fedora/RHEL: `dnf`
  - Alpine: `apk`
- **AC:** `System` provider detects and uses correct package manager

### Task 4.2b: Conda Provider
- **Why Linux:** Needs working conda/mamba installation
- **Status:** Not started
- **Action:**
  1. Wrap `mamba search --json`
  2. Parse JSON output
  3. Implement install into environment
- **Files to create:** `src/provider/conda.rs`
- **AC:** Can search and install packages via mamba

### Task 4.5: Interactive Installation (sudo)
- **Why Linux:** `sudo` prompts are Unix-specific
- **Status:** Not started
- **Action:** Test that `Stdio::inherit()` passes through sudo prompts
- **AC:** Installing a system package prompts for password

---

## Phase 5: Sync & Logic - NOT STARTED

### Task 5.1: Adoption Logic
- **Why Linux:** `which` command
- **Status:** Not started
- **Action:** Detect pre-installed tools
- **Implementation:** Use `which` crate or shell command
- **AC:** `schalentier add grep` results in "Adopted" status

### Task 5.2: Git SSH Adapter
- **Why Linux:** SSH agent integration
- **Status:** Not started
- **Action:** Test SSH key authentication for git clone
- **AC:** Can clone a private repo using SSH keys

---

## Phase 6: Polish - NOT STARTED

### Task 6.1: Dotfile Patcher Integration
- **Why Linux:** Real dotfiles (`.bashrc`, `.zshrc`)
- **Status:** Not started
- **AC:** Block insertion into `.bashrc` doesn't break shell

### Task 6.2: Keyring Fallback
- **Why Linux:** Need headless environment
- **Action:** Test in Docker without D-Bus
- **AC:** Secrets work without keyring

### Task 6.4: Installation Script
- **Why Linux:** Unix shell script
- **Status:** Not started
- **File to create:** `install.sh`
- **Template:**
  ```bash
  #!/bin/bash
  set -euo pipefail

  ARCH=$(uname -m)
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')

  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
  esac

  DOWNLOAD_URL="https://github.com/user/schalentier/releases/latest/download/schalentier-${OS}-${ARCH}"

  mkdir -p ~/.local/bin
  curl -Lo ~/.local/bin/schalentier "$DOWNLOAD_URL"
  chmod +x ~/.local/bin/schalentier

  echo "Installed schalentier to ~/.local/bin/schalentier"
  ```
- **AC:** Script installs binary on fresh system

---

## Testing Approach

### Option 1: WSL2 (Recommended)
```powershell
wsl --install -d Ubuntu
```
Then test directly in WSL with the cross-compiled binary.

### Option 2: Docker
```bash
# Test on Ubuntu
docker run -it --rm -v $(pwd)/target/x86_64-unknown-linux-musl/release:/app ubuntu:22.04 /app/schalentier --help

# Test on Alpine (musl native)
docker run -it --rm -v $(pwd)/target/x86_64-unknown-linux-musl/release:/app alpine:latest /app/schalentier --help
```

### Option 3: GitHub Actions CI
Create `.github/workflows/test.yml`:
```yaml
name: Test
on: [push, pull_request]
jobs:
  test-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test
      - run: cargo build --release
      - run: ./target/release/schalentier --help
      - run: ./target/release/schalentier doctor
```

---

## Priority Order for Linux Testing

### Immediate (verify Windows work)
1. Binary execution verification
2. File permissions test
3. Shell script sourcing

### High Priority (core functionality)
4. Miniforge bootstrap execution
5. uv bootstrap extraction
6. System provider implementation
7. install.sh script

### Medium Priority (full features)
8. Conda provider
9. Adoption logic
10. Git SSH adapter
11. sudo prompts

### Lower Priority (polish)
12. Dotfile patcher
13. Keyring fallback
