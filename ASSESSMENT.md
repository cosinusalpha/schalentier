# Senior Linux User Assessment

**Question:** Would I use schalentier as my daily driver?

**Short answer:** Not yet — but I'd try it for a narrow slice (CLI tool bootstrap).
Not for dotfiles/secrets end-to-end, not today.

## Where it beats what I use now

- **Multi-provider fallback** (binary → cargo → conda → system) is genuinely
  useful. `mise`/`aqua` don't do this — they pick one ecosystem and stop.
  Real pain point solved.
- **Dotfile merging** (not full-file templating like chezmoi) is a different,
  sometimes better model when I only care about 3 keys in a 200-line JSON I
  don't otherwise manage.
- **Secrets + CI story** is more thought-out than most dotfile managers
  bother with (see prior review: env-var override, `secret export`/`run`,
  encrypted-at-rest, git-syncable).

## Why I wouldn't switch my main setup to it today

1. **Version 0.1.2, unproven.** Chezmoi/yadm have years of edge cases beaten
   out of them — symlink handling, template failures, merge conflicts across
   machines. This is new. I don't bet `~/.ssh/config` and shell env on a tool
   with no track record, however good the code reads.

2. **No reproducibility guarantee.** `[tools] ripgrep = {}` installs "latest
   via whatever provider wins" — that's convenience, not reproducibility.
   Nix/lockfile-based tools (even `mise` with its lockfile) pin exact
   versions by default. Two machines running `schalentier sync --pull` a week
   apart can silently diverge. For a "one file, every machine" pitch, there's
   no lockfile — that's the actual core promise, and it's not fully
   delivered.

3. **Merge-not-replace dotfile model is a double-edged sword.** chezmoi's
   "you own the whole file, it's a template, diff it in git" model is
   auditable — `git diff` shows exactly what changed. Schalentier's
   JSON/TOML/INI deep merge computes the effective file at apply-time from
   two sources (existing file + patch) — harder to reason about drift, and a
   single backup-file-per-target isn't the same safety net as full git
   history of the actual file content.

4. **One binary, many trust domains.** Package installation (runs arbitrary
   installers/binaries), dotfile patching (writes to my configs), and
   secrets (decrypts credentials) all live in one process, one codebase, one
   audit surface. I'd rather compose small, separately-trusted tools (`mise`
   for toolchains, `chezmoi`+`age` for dotfiles+secrets, native pkg manager
   for system tools) than trust one young project across all three at once.

5. **No community/ecosystem yet.** Registry has ~34 packages hardcoded.
   Chezmoi/yadm have thousands of real-world dotfile examples to copy from;
   mise has a plugin ecosystem. Schalentier's `search`/fallback is elegant
   but actual package coverage is thin until it's been battle-tested by more
   than one team.

## What's missing (in priority order)

1. **A lockfile** (`schalentier.lock`) pinning resolved versions per tool,
   written only by `update`. Turns the "sync everywhere" pitch from
   aspirational into actually reproducible — this is the single biggest gap
   relative to the tool's own core promise.
2. **Real multi-machine mileage.** Months of actual daily use across
   different machines/OSes, not just clean-room smoke tests in containers.
3. **`config diff`-before-apply as the default workflow**, not an optional
   command — since the merge model is harder to audit than full-file
   templating, the safety net needs to be load-bearing, not opt-in.
4. **Registry breadth.** 34 packages is a demo, not a catalog. Needs
   community contributions or a much larger seed set before `search`/`add`
   feels reliable for arbitrary tools.
5. **Trust-domain separation** (longer-term, lower priority): consider
   whether secrets handling should be more isolated from the
   installer/dotfile-patcher code paths, given it's the highest-consequence
   feature in the binary.

## Where I'd use it today

Bootstrapping CLI tools for a personal project — its strongest, most
differentiated feature — before trusting it with dotfiles or secrets
end-to-end.
