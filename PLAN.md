\# 📋 Schalentier: Development Roadmap



\## 🟢 Phase 1: The Skeleton \& CLI Foundation



\*\*Goal:\*\* A compilable binary that parses arguments and handles errors, even if it does nothing yet.



\* \*\*Task 1.1: Project Initialization\*\*

\* \*\*Action:\*\* Run `cargo new`. Configure `Cargo.toml` with `clap`, `tokio`, `anyhow`, `tracing`. Set up the `x86\_64` and `aarch64` musl build targets.

\* \*\*AC:\*\* `cargo build --target x86\_64-unknown-linux-musl` succeeds without warnings.





\* \*\*Task 1.2: CLI Argument Parsing\*\*

\* \*\*Action:\*\* Define `clap` structs for `init`, `add`, `sync`, `update`, `doctor`.

\* \*\*AC:\*\* Running `schalentier --help` shows the correct menu. Running `schalentier add` without args returns a specific error message.





\* \*\*Task 1.3: Logging \& Error Handling\*\*

\* \*\*Action:\*\* Setup `tracing-subscriber` for debug logs. Implement a global error handler that pretty-prints `anyhow` errors (red text, no stack trace for users).

\* \*\*AC:\*\* `RUST\_LOG=debug schalentier` shows internal logs. A forced panic/error displays a clean "❌ Error: ..." message.







\## 🟡 Phase 2: Configuration \& State



\*\*Goal:\*\* The tool can remember who it is and what it has installed.



\* \*\*Task 2.1: Data Models (Structs)\*\*

\* \*\*Action:\*\* Create `config.rs`. Define `SchalentierConfig` (TOML) and `LocalState` (JSON) structs with `serde`.

\* \*\*AC:\*\* Unit tests pass that serialize a sample struct to string and deserialize it back correctly.





\* \*\*Task 2.2: Local State Management\*\*

\* \*\*Action:\*\* Implement `LocalState::load()` and `save()`. Ensure it creates the directory `~/.schalentier` if missing. Enforce `0o600` permissions on the file.

\* \*\*AC:\*\* Running the tool creates `~/.schalentier/local\_state.json`. Manually editing the file persists data between runs.





\* \*\*Task 2.3: Provider Priority Configuration\*\*

\* \*\*Action:\*\* Add `priority` list to `Settings`. Implement logic to merge user preference (TOML) with hardcoded defaults.

\* \*\*AC:\*\* Configuration file successfully overrides the default provider order.







\## 🟠 Phase 3: The "Brain" (Bootstrap \& Shells)



\*\*Goal:\*\* The tool can set up its own isolated environment.



\* \*\*Task 3.1: Architecture Detection\*\*

\* \*\*Action:\*\* Implement `bootstrap::get\_arch()`.

\* \*\*AC:\*\* Returns `Aarch64` on M1/Graviton and `X86\_64` on Intel/AMD. Returns error on unsupported architectures.





\* \*\*Task 3.2: Miniforge \& Tool Bootstrap\*\*

\* \*\*Action:\*\* Implement `install\_miniforge()`, `install\_rust()`, `install\_uv()`. Use `reqwest` to download the correct arch script. Run installers in batch mode (no user prompt).

\* \*\*AC:\*\* Running `bootstrap()` results in a working `~/.schalentier/conda/bin/conda` and `~/.schalenbin/uv` binary.





\* \*\*Task 3.3: Shell Script Generation\*\*

\* \*\*Action:\*\* Implement generation of `env.sh` (Bash/Zsh) and `env.fish`. Must export PATHs correctly.

\* \*\*AC:\*\* Sourcing the generated file in a fresh terminal allows executing `uv` and `conda`.







\## 🔵 Phase 4: The Provider Engine



\*\*Goal:\*\* The tool can search for and install packages.



\* \*\*Task 4.1: The Installer Trait\*\*

\* \*\*Action:\*\* Define the `Installer` async trait (`search`, `install`, `uninstall`).

\* \*\*AC:\*\* A dummy "MockProvider" can be implemented and called by the main loop.





\* \*\*Task 4.2: System \& Conda Providers\*\*

\* \*\*Action:\*\* Implement `System` (detect `pacman`/`apt`) and `Conda` (wrap `mamba`).

\* \*\*AC:\*\* `System` provider correctly detects the OS package manager. `Conda` provider successfully parses JSON output from `mamba search --json`.





\* \*\*Task 4.3: Binary \& Cargo Providers\*\*

\* \*\*Action:\*\* Implement `Cargo` (wrap binary) and `Binary` (GitHub Releases API).

\* \*\*AC:\*\* `Binary` provider can find "micro" on GitHub and download the correct asset for the OS.





\* \*\*Task 4.4: Search Aggregation (Clustering)\*\*

\* \*\*Action:\*\* Implement the "Split Logic". Fetch results in parallel. Group by name. Calculate Description Jaccard Index.

\* \*\*AC:\*\* Searching for "micro" returns TWO groups (Editor vs Framework) in the debug output.





\* \*\*Task 4.5: Interactive Installation\*\*

\* \*\*Action:\*\* Implement `install()`. Use `std::process::Command` with `Stdio::inherit()`.

\* \*\*AC:\*\* Installing a system package triggers the OS `sudo` password prompt, and input is accepted.







\## 🟣 Phase 5: Logic \& Synchronization



\*\*Goal:\*\* The tool is smart (Adoption, Pruning, Sync).



\* \*\*Task 5.1: Adoption Logic\*\*

\* \*\*Action:\*\* Before install, check `which <tool>`. If found, write to State (`managed: false`).

\* \*\*AC:\*\* `add grep` (which is everywhere) results in "Adopted" status, not an installation attempt.





\* \*\*Task 5.2: Sync Adapters\*\*

\* \*\*Action:\*\* Implement `GitSSH` (git wrapper) and `HttpReadOnly` (reqwest GET).

\* \*\*AC:\*\* Can fetch a raw TOML from a Gist URL. Can `git clone` a repo via SSH.





\* \*\*Task 5.3: Structured Merging\*\*

\* \*\*Action:\*\* Implement `Merger::merge(local, remote)`.

\* \*\*AC:\*\* Unit test: Merging `{a:1}` and `{b:2}` results in `{a:1, b:2}`. Conflict test: Merging `{a:1}` and `{a:2}` returns a `Conflict` enum.





\* \*\*Task 5.4: Pruning (Garbage Collection)\*\*

\* \*\*Action:\*\* Compare State vs Config. Call `uninstall`.

\* \*\*AC:\*\* Removing a tool from TOML and running logic results in the `uninstall()` method being called.







\## 🔴 Phase 6: Polish \& Advanced Features



\*\*Goal:\*\* UX refinements and deployment.



\* \*\*Task 6.1: Dotfile Patcher\*\*

\* \*\*Action:\*\* Implement "Block Replacer" (Regex) and "JSON Merger" (Serde).

\* \*\*AC:\*\* `sync` successfully inserts a block of text into a dummy `.bashrc` file without deleting existing content.





\* \*\*Task 6.2: Secrets Management\*\*

\* \*\*Action:\*\* Implement `keyring` access. Add fallback to `local\_state` if keyring fails (simulate by running in a headless Docker container).

\* \*\*AC:\*\* `schalentier` can retrieve a Gist token saved in a previous session.





\* \*\*Task 6.3: UI Polish\*\*

\* \*\*Action:\*\* Add `indicatif` spinners to Search/Sync. Use `inquire` for the Provider Selection menu and Conflict Resolution menu.

\* \*\*AC:\*\* The app feels responsive. Long operations show a spinner.





\* \*\*Task 6.4: The `install.sh` Script\*\*

\* \*\*Action:\*\* Write the shell script to detect OS, fetch binary from GitHub, and install to `~/.local/bin`.

\* \*\*AC:\*\* Running the script in a fresh CI environment successfully installs the binary.







---



\### ⏱️ Estimated Timeline (for 1 Senior Developer)



\* \*\*Week 1:\*\* Phases 1 \& 2 (Skeleton, Config, State).

\* \*\*Week 2:\*\* Phase 3 (Bootstrap, Architecture, Shells).

\* \*\*Week 3:\*\* Phase 4 (Providers, Search, Install).

\* \*\*Week 4:\*\* Phase 5 (Sync, Merge, Prune).

\* \*\*Week 5:\*\* Phase 6 (Patching, Secrets, Polish).

