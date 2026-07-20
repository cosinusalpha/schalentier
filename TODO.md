# Schalentier: TODO

**Last Updated:** 2026-01-18

---

## Phase 7: Advanced Features - MOSTLY DONE

**Goal:** Secrets management, templating, and project-local configuration.

**Status summary:**
- 7.1 Secrets management — **DONE** (`src/secrets.rs`: age + keyring, plus `shell`/`run` subcommands beyond spec).
- 7.2 Templating — **DONE** (`src/template.rs` wired into `src/dotfiles.rs`; `[variables]` supported).
- 7.3 Project-local config — **DONE**. Merge (`SchalentierConfig::load_with_project`), project secrets, and `doctor` reporting implemented. `load_with_project` now used by `config apply`/`config diff`, `sync`, `doctor`, `list`, and `update` (project tool overrides — including version pins — apply to those). `add`/`remove` intentionally use plain `load()` because they mutate-and-save global config (merging the project layer would corrupt the saved global file).

### Task 7.1: Secrets Management - DONE

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
schalentier secret export [--shell bash|fish]      # Output for eval/source
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
```

**Auto-export in env.sh (opt-in via config):** — NOT YET IMPLEMENTED. The
`auto_export_secrets` setting below is not present in the `Settings` struct
(`src/config.rs`) and `env.sh` does not emit the snippet. Implement the field + shell hook
before documenting in the README.

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

### Task 7.2: Templating Engine - DONE

- **Priority:** HIGH (enables dynamic configs)
- **Dependencies:** `minijinja` (Jinja2-compatible, pure Rust)
- **Files:** `src/template.rs`, modifications to `src/dotfiles.rs`

**Template Engine:** minijinja (Jinja2 syntax)

**Context Variables:**

```
{{ os }}              → "linux" | "macos"
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

### Task 7.3: Project-Local Configuration - PARTIAL (see status summary above)

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

## Dependencies to Add

```toml
# Add to Cargo.toml for Phase 7

# Secrets Management (Task 7.1)
age = "0.10"              # age encryption (pure Rust)
keyring = "3"             # Cross-platform system keyring

# Templating (Task 7.2)
minijinja = "2"           # Jinja2-compatible template engine
```

---

## Pre-Release Checklist

### UX Improvements

| Feature | Priority | Status |
|---------|----------|--------|
| Self-update mechanism | Low | Future |
| `GITHUB_TOKEN` env var support | Low | Future (rate limits handled gracefully) |
