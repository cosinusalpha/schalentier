# Schalentier 🦐

> German: *Schalentier* [ˈʃaːlənˌtiːɐ̯] — “shellfish”, literally “shell animal”

**Your tools and configuration, reproducible from one TOML file.**

Schalentier is a cross-platform CLI for installing development tools, patching
dotfiles, and syncing the result between machines. It can:

- install tools through GitHub releases, Go, Cargo, Homebrew, Conda, uv, or the
  system package manager;
- fall back to another available provider when an installation fails;
- merge JSON, TOML, YAML, INI, and `KEY=value` configuration without replacing
  unrelated settings;
- render dotfiles with machine information, variables, environment values, and
  encrypted secrets;
- sync its configuration through Git or an encrypted GitHub Gist;
- manage shell aliases, initialization snippets, and completions;
- check supported package ecosystems for known vulnerabilities through OSV.dev.

## Install

On Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/cosinusalpha/schalentier/main/install.sh | sh
```

The installer downloads the latest release to `~/.local/bin` and runs
`schalentier init`. Set `SCHALENTIER_NO_INIT=1` to skip initialization, or
`SCHALENTIER_INSTALL_DIR` to choose another destination.

With Rust installed:

```bash
cargo install schalentier
```

To build this checkout instead:

```bash
cargo install --path .
```

## Quick start

Initialize Schalentier and optionally bootstrap isolated copies of uv,
Miniforge, Rust, Node.js, and Go under `~/.schalentier/`:

```bash
schalentier init
```

Add and manage tools:

```bash
schalentier add ripgrep
schalentier add bat --provider cargo
schalentier add python --provider conda
schalentier list --detailed
schalentier update
```

Preview operations with `--dry-run` where supported. Run
`schalentier <command> --help` for every option.

### Installation locations

These are the default locations used by Schalentier itself:

| Installed item | Default location |
| --- | --- |
| Schalentier from `install.sh` | `$HOME/.local/bin/schalentier` |
| Prebuilt tools from the `binary` provider | `$HOME/.schalentier/bin/` |
| Bootstrapped uv | `$HOME/.schalentier/bin/uv` |
| Bootstrapped Miniforge | `$HOME/.schalentier/conda/` |
| Bootstrapped Rust/Cargo | `$HOME/.schalentier/.cargo/` and `$HOME/.schalentier/rustup/` |
| Bootstrapped Node.js | `$HOME/.schalentier/node/` |
| Bootstrapped Go | `$HOME/.schalentier/go/` |

When Schalentier bootstraps Go, Go packages are routed to
`$HOME/.schalentier/bin/`; bootstrapped Cargo packages go to
`$HOME/.schalentier/.cargo/bin/`. Existing system toolchains keep their native
locations: Cargo normally uses `$HOME/.cargo/bin/`, Go follows `GOBIN`, then
`GOPATH/bin`, then `$HOME/go/bin/`, and uv controls its tool directory (normally
`$HOME/.local/bin/`). Homebrew and system-provider packages remain managed by
their respective package managers.

## Configuration

The default configuration is `$HOME/.config/schalentier/schalentier.toml`.
Schalentier resolves its main config in this order:

1. `./schalentier.toml`;
2. `$HOME/.config/schalentier/schalentier.toml`;
3. the legacy `$HOME/schalentier.toml` location.

For `sync`, `update`, `doctor`, `list`, `audit`, and config apply/diff operations,
the nearest `.schalentier/config.toml` found between the working directory and
the home directory is merged over the main config.

```toml
[settings]
provider_priority = ["binary", "go", "cargo", "brew", "conda", "uv", "system"]
auto_update = false
audit_cache_ttl_hours = 24

[tools]
ripgrep = {}
bat = { provider = "cargo", version = "0.24.0" }
python = { provider = "conda", version = "3.12" }

[variables]
email = "ada@example.com"

[dotfiles."~/.gitconfig".user]
_template = true
name = "Ada Lovelace"
email = "{{ var.email }}"

[dotfiles."~/.config/micro/settings.json"]
colorscheme = "dracula"
tabsize = 4

[sync]
remote = "git@github.com:ada/dotfiles.git"
mode = "manual"
```

Each tool accepts an optional `provider`, `version`, and provider-specific
`options` table. A fixed version is skipped by `schalentier update` unless
`--force` is used; omit the version or use `latest` to track updates.

Custom package names and provider mappings can be added under `[aliases]`. See
[`registry/README.md`](registry/README.md) for the provider mapping format.

## Providers

| Provider | Source | Typical use |
| --- | --- | --- |
| `binary` | GitHub Releases | Prebuilt CLI binaries |
| `go` | Go modules | Go command-line tools |
| `cargo` | crates.io | Rust command-line tools |
| `brew` | Homebrew/Linuxbrew | General packages |
| `conda` | conda-forge | Python and scientific packages |
| `uv` | PyPI | Isolated Python CLI tools |
| `system` | apt, pacman, dnf, and others | OS packages; may require sudo |

Only providers available on the current machine are registered. The binary
provider is always available, and Schalentier uses the configured priority when
a tool exists in more than one provider.

## Dotfiles and templates

Use `schalentier config diff` to preview changes and
`schalentier config apply` to apply them. Schalentier deep-merges JSON, TOML,
and YAML, performs section-aware INI merges, and updates key/value files by key.
Unknown formats can be replaced explicitly with `_content`:

```toml
[dotfiles."~/.vimrc"]
_content = """
set number
set expandtab
"""
```

Before changing a file for the first time, Schalentier creates a sibling named
`<filename>.schalentier-backup` (for example,
`~/.vimrc.schalentier-backup`). Restore it with:

```bash
schalentier config reset ~/.vimrc
```

Set `_template = true` on a dotfile entry to use MiniJinja expressions. The
template context contains:

- `os`, `arch`, `hostname`, `username`, and `home`;
- `env.NAME` for environment variables;
- `var.NAME` for values under `[variables]`;
- `secret.NAME` for values in the encrypted secret store.

## Sync

Git sync operates on Schalentier's configuration directory:

```bash
# First machine
schalentier sync --remote git@github.com:you/dotfiles.git --push

# Another machine
schalentier sync --remote git@github.com:you/dotfiles.git --pull
```

With neither `--push` nor `--pull`, sync is bidirectional. A pull installs tools
present in the remote config but absent from local state. Add `--prune` to also
remove managed tools that are no longer configured.

For Gist sync, store a GitHub token and create an encrypted Gist:

```bash
schalentier secret set GITHUB_TOKEN --tags github
schalentier sync --remote gist://new --push
```

Save the returned `gist://<id>` as `[sync].remote`, then use
`schalentier sync --pull` on another machine. Gist payloads are encrypted with
age; the master password is kept in the OS keyring when available, with an
encrypted-file fallback for headless environments. Gists are secret by default;
pass `--public` to create a public one.

## Other commands

| Command | Purpose |
| --- | --- |
| `search <query>` | Search packages across available providers |
| `remove <tool>` | Uninstall a managed tool and remove it from config |
| `doctor [--fix]` | Diagnose the installation and optionally repair issues |
| `audit [package]` | Query OSV.dev for known vulnerabilities |
| `alias` | Create executable shell aliases |
| `snippet` | Add, list, or remove shell initialization snippets |
| `secret` | Store, list, export, edit, or inject encrypted secrets |
| `registry` | Validate, inspect, or update the package registry |
| `completions <shell>` | Generate shell completions |

Audit results are cached for `audit_cache_ttl_hours` (24 hours by default).
`list --security` displays cached results without making a network request.
OSV.dev coverage currently maps Cargo to crates.io, uv to PyPI, and Go to the Go
ecosystem; providers without an OSV ecosystem are skipped.

Secrets may be tagged and exposed only to a command or subshell:

```bash
schalentier secret set DEPLOY_TOKEN --tags deploy
schalentier secret run --tags deploy -- ./deploy.sh
schalentier secret shell --tags deploy
```

## Shell setup

`schalentier init` writes two generated environment files:

- `$HOME/.schalentier/env.sh` for Bash and Zsh;
- `$HOME/.schalentier/env.fish` for Fish.

They add Schalentier's bin directory and any bootstrapped Rust, Node.js, and Go
directories to `PATH`, initialize Schalentier's Miniforge installation, and set
`SCHALENTIER_DATA_DIR`.

During interactive initialization, Schalentier offers to source the appropriate
file from `$HOME/.bashrc`, `$HOME/.zshrc`, or
`$HOME/.config/fish/config.fish`. For a non-interactive setup, use:

```bash
schalentier init --yes --setup-shell
```

To activate the generated environment in the current shell immediately:

```bash
# Bash or Zsh
source "$HOME/.schalentier/env.sh"

# Fish
source "$HOME/.schalentier/env.fish"
```

Only source the file matching your shell. New shells load it automatically once
the integration has been added to the corresponding shell configuration.

Generate completion scripts separately, for example:

```bash
schalentier completions fish > ~/.config/fish/completions/schalentier.fish
```

## Development

```bash
cargo test
./tests/smoke/run.sh # requires Docker or Podman
```

The project is licensed under the MIT License.
