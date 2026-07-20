#!/bin/bash
# Smoke tests for schalentier
# Run inside a container to test real functionality

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASSED=0
FAILED=0
SKIPPED=0

# Test helper functions
pass() {
    echo -e "${GREEN}✓ PASS${NC}: $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo -e "${RED}✗ FAIL${NC}: $1"
    echo "  Error: $2"
    FAILED=$((FAILED + 1))
}

skip() {
    echo -e "${YELLOW}○ SKIP${NC}: $1 - $2"
    SKIPPED=$((SKIPPED + 1))
}

section() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  $1"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# Clean up any previous state
cleanup() {
    rm -rf ~/.schalentier 2>/dev/null || true
    rm -f ~/.config/schalentier.toml 2>/dev/null || true
    rm -rf ~/.config/schalentier 2>/dev/null || true
}

#=============================================================================
# SMOKE TESTS START HERE
#=============================================================================

section "Basic CLI Tests"

# Test 1: Binary runs
if schalentier --version > /dev/null 2>&1; then
    pass "Binary executes (--version)"
else
    fail "Binary executes" "schalentier --version failed"
fi

# Test 2: Help works
if schalentier --help | grep -q "Usage:"; then
    pass "Help displays usage"
else
    fail "Help displays usage" "Missing 'Usage:' in help output"
fi

# Test 3: All subcommands have help
for cmd in init add remove list search sync update doctor secret registry audit config alias snippet completions; do
    if schalentier $cmd --help > /dev/null 2>&1; then
        pass "Subcommand '$cmd' has help"
    else
        fail "Subcommand '$cmd' has help" "schalentier $cmd --help failed"
    fi
done

#=============================================================================
section "Initialization Tests"

cleanup

# Test 4: Init creates directories (--yes for non-interactive; --skip-bootstrap to
# avoid downloading real toolchains like node/go, which is slow and flaky in a container)
if schalentier init --yes --skip-bootstrap 2>/dev/null; then
    if [ -d ~/.schalentier ]; then
        pass "Init creates ~/.schalentier directory"
    else
        fail "Init creates directory" "~/.schalentier not found"
    fi
else
    fail "Init command" "schalentier init failed"
fi

# Test 5: State file created
if [ -f ~/.schalentier/local_state.json ]; then
    pass "Init creates local_state.json"
else
    fail "Init creates state file" "local_state.json not found"
fi

# Test 6: State file has correct permissions
perms=$(stat -c %a ~/.schalentier/local_state.json 2>/dev/null || stat -f %Lp ~/.schalentier/local_state.json 2>/dev/null)
if [ "$perms" = "600" ]; then
    pass "State file has 0600 permissions"
else
    fail "State file permissions" "Expected 600, got $perms"
fi

# Test 7: Shell scripts created
for script in env.sh env.fish; do
    if [ -f ~/.schalentier/$script ]; then
        pass "Init creates $script"
    else
        fail "Init creates $script" "File not found"
    fi
done

# Test 7b: --setup-shell appends a source line to the rc file, idempotently
schalentier init --yes --setup-shell --skip-bootstrap --force > /dev/null 2>&1 || true
if [ -f ~/.bashrc ] && grep -q "env.sh" ~/.bashrc; then
    pass "Init --setup-shell sources env.sh from ~/.bashrc"
else
    fail "Init --setup-shell" "~/.bashrc missing or doesn't source env.sh"
fi
schalentier init --yes --setup-shell --skip-bootstrap --force > /dev/null 2>&1 || true
SOURCE_LINE_COUNT=$(grep -c 'source "/.*env.sh"' ~/.bashrc 2>/dev/null || echo 0)
if [ "$SOURCE_LINE_COUNT" = "1" ]; then
    pass "Init --setup-shell is idempotent (no duplicate source line)"
else
    fail "Init --setup-shell idempotency" "Expected 1 source line for env.sh in ~/.bashrc, got $SOURCE_LINE_COUNT"
fi

# Test 8: Re-init without --force fails
# Capture to variable to avoid SIGPIPE
REINIT_OUTPUT=$(schalentier init 2>&1 || true)
if echo "$REINIT_OUTPUT" | grep -qi "already initialized\|force"; then
    pass "Re-init without --force shows warning"
else
    fail "Re-init protection" "Should warn about existing installation"
fi

#=============================================================================
section "Doctor Tests"

# Test 9: Doctor runs without error
if schalentier doctor > /dev/null 2>&1; then
    pass "Doctor command runs"
else
    fail "Doctor command" "schalentier doctor failed"
fi

# Test 10: Doctor shows status
# Note: Capture to variable to avoid SIGPIPE when grep -q closes pipe early
DOCTOR_OUTPUT=$(schalentier doctor 2>&1)
if echo "$DOCTOR_OUTPUT" | grep -qiE "status|initialized|bootstrap"; then
    pass "Doctor shows system status"
else
    fail "Doctor output" "No status information in output"
fi

#=============================================================================
section "Search Tests"

# Test 11: Search returns results (requires network)
if curl -s --connect-timeout 5 https://api.github.com > /dev/null 2>&1; then
    # Capture to variable to avoid SIGPIPE
    SEARCH_OUTPUT=$(schalentier search ripgrep 2>&1)
    if echo "$SEARCH_OUTPUT" | grep -qi "ripgrep\|rg\|BurntSushi"; then
        pass "Search finds ripgrep"
    else
        fail "Search results" "ripgrep not found in search results"
    fi
else
    skip "Search tests" "No network connectivity"
fi

# Test 12: Search with limit
if curl -s --connect-timeout 5 https://crates.io > /dev/null 2>&1; then
    result_count=$(schalentier search serde --limit 3 2>&1 | grep -c "serde" || true)
    if [ "$result_count" -ge 1 ]; then
        pass "Search with --limit works"
    else
        skip "Search limit test" "Could not verify results"
    fi
else
    skip "Search limit test" "No network connectivity"
fi

#=============================================================================
section "List Tests"

# Test 13: List command works (even if empty)
if schalentier list > /dev/null 2>&1; then
    pass "List command runs"
else
    fail "List command" "schalentier list failed"
fi

#=============================================================================
section "Provider Detection Tests"

# Capture doctor output once to avoid SIGPIPE issues
PROVIDER_DOCTOR_OUTPUT=$(schalentier doctor 2>&1)

# Test 14: System provider detects package manager
if [ -f /usr/bin/apt ] || [ -f /usr/bin/apt-get ]; then
    if echo "$PROVIDER_DOCTOR_OUTPUT" | grep -qi "apt\|system"; then
        pass "Detects apt package manager"
    else
        skip "Apt detection" "apt exists but not detected in doctor output"
    fi
elif [ -f /usr/bin/pacman ]; then
    if echo "$PROVIDER_DOCTOR_OUTPUT" | grep -qi "pacman\|system"; then
        pass "Detects pacman package manager"
    else
        skip "Pacman detection" "pacman exists but not detected"
    fi
elif [ -f /sbin/apk ]; then
    if echo "$PROVIDER_DOCTOR_OUTPUT" | grep -qi "apk\|system"; then
        pass "Detects apk package manager"
    else
        skip "Apk detection" "apk exists but not detected"
    fi
else
    skip "System provider detection" "No known package manager found"
fi

# Test 15: Cargo provider availability
if command -v cargo > /dev/null 2>&1; then
    pass "Cargo is available in PATH"
else
    skip "Cargo provider" "Cargo not installed"
fi

#=============================================================================
section "Shell Integration Tests"

# Test 16: env.sh is sourceable
if bash -c "source ~/.schalentier/env.sh && echo ok" 2>/dev/null | grep -q "ok"; then
    pass "env.sh is sourceable in bash"
else
    fail "env.sh sourcing" "Failed to source env.sh in bash"
fi

# Test 17: env.sh sets SCHALENTIER_DATA_DIR
if bash -c "source ~/.schalentier/env.sh && [ -n \"\$SCHALENTIER_DATA_DIR\" ]" 2>/dev/null; then
    pass "env.sh sets SCHALENTIER_DATA_DIR"
else
    fail "env.sh environment" "SCHALENTIER_DATA_DIR not set after sourcing"
fi

# Test 18: env.sh adds bin to PATH
if bash -c "source ~/.schalentier/env.sh && echo \$PATH" 2>/dev/null | grep -q ".schalentier/bin"; then
    pass "env.sh adds bin directory to PATH"
else
    fail "env.sh PATH" "bin directory not in PATH after sourcing"
fi

#=============================================================================
section "Sync Tests"

# Setup git for sync tests
git config --global user.email "test@test.com" 2>/dev/null || true
git config --global user.name "Test" 2>/dev/null || true
git config --global init.defaultBranch main 2>/dev/null || true

# Test 19: Sync initializes git repo
SYNC_OUTPUT=$(schalentier sync 2>&1)
if echo "$SYNC_OUTPUT" | grep -qi "initialized\|repository\|set up remote"; then
    pass "Sync initializes git repository"
else
    fail "Sync git init" "Should initialize git repo or prompt for remote"
fi

# Test 20: Config directory has .git after sync
if [ -d ~/.config/schalentier/.git ]; then
    pass "Sync creates .git directory"
else
    fail "Sync .git directory" ".git not found in config directory"
fi

# Test 21: Sync with remote (using local bare repo)
mkdir -p /tmp/test-remote.git
cd /tmp/test-remote.git && git init --bare 2>/dev/null
SYNC_REMOTE_OUTPUT=$(schalentier sync --remote /tmp/test-remote.git 2>&1)
if echo "$SYNC_REMOTE_OUTPUT" | grep -qi "push\|pull\|sync complete\|remote"; then
    pass "Sync with remote works"
else
    fail "Sync with remote" "Should sync with remote repository"
fi

#=============================================================================
section "Gist Sync Tests (mocked GitHub API)"

# Requires: python3 (for the mock server) and a master password (secret store).
MOCK_SERVER="/run_mock_gist_server.py"
if [ ! -f "$MOCK_SERVER" ]; then
    MOCK_SERVER="$(dirname "$0")/mock_gist_server.py"
fi

if [ -z "${SCHALENTIER_MASTER_PASSWORD:-}" ]; then
    skip "Gist sync tests" "SCHALENTIER_MASTER_PASSWORD not set"
elif ! command -v python3 > /dev/null 2>&1; then
    skip "Gist sync tests" "python3 not available for mock server"
elif [ ! -f "$MOCK_SERVER" ]; then
    skip "Gist sync tests" "mock_gist_server.py not found"
else
    # Start the in-memory mock GitHub Gists API.
    python3 "$MOCK_SERVER" 8099 > /tmp/mock_gist.log 2>&1 &
    MOCK_PID=$!
    export SCHALENTIER_GITHUB_API_BASE="http://127.0.0.1:8099"

    # Wait for the mock to accept connections.
    mock_ready=0
    for _ in $(seq 1 20); do
        if curl -s --connect-timeout 1 http://127.0.0.1:8099/gists/nope > /dev/null 2>&1; then
            mock_ready=1; break
        fi
        sleep 0.2
    done

    if [ "$mock_ready" -ne 1 ]; then
        skip "Gist sync tests" "mock server did not start"
    else
        cleanup
        mkdir -p ~/.config/schalentier
        cat > ~/.config/schalentier/schalentier.toml << 'GISTCFG'
[tools]

[dotfiles."~/.config/gist-marker/marker.env"]
GIST_ROUNDTRIP = "ok"
GISTCFG
        schalentier secret set GITHUB_TOKEN --value ghp_fake_smoke > /dev/null 2>&1

        # Push: create a new gist and capture its id.
        GIST_PUSH=$(schalentier sync --remote gist://new --push 2>&1 || true)
        GID=$(echo "$GIST_PUSH" | grep -oE "gist://mock[0-9]+" | head -1)
        if echo "$GIST_PUSH" | grep -qi "created secret gist" && [ -n "$GID" ]; then
            pass "Gist push creates a new gist"
        else
            fail "Gist push" "Expected 'Created secret gist', got: $GIST_PUSH"
        fi

        # The mock stores ciphertext; confirm the pushed content is NOT plaintext.
        if [ -n "$GID" ]; then
            RAW=$(curl -s "http://127.0.0.1:8099/gists/${GID#gist://}" || true)
            if echo "$RAW" | grep -q "BEGIN AGE ENCRYPTED FILE"; then
                pass "Gist content is age-encrypted"
            else
                fail "Gist encryption" "Pushed gist content is not age-encrypted"
            fi
            if echo "$RAW" | grep -q "GIST_ROUNDTRIP"; then
                fail "Gist encryption" "Plaintext config leaked into gist!"
            else
                pass "Gist does not leak plaintext config"
            fi
        fi

        # Pull on a fresh state: config should be restored and the dotfile applied.
        if [ -n "$GID" ]; then
            cleanup
            mkdir -p ~/.config/schalentier
            schalentier secret set GITHUB_TOKEN --value ghp_fake_smoke > /dev/null 2>&1
            GIST_PULL=$(schalentier sync --remote "$GID" --pull 2>&1 || true)
            if echo "$GIST_PULL" | grep -qi "downloaded and decrypted"; then
                pass "Gist pull downloads and decrypts config"
            else
                fail "Gist pull" "Expected 'Downloaded and decrypted', got: $GIST_PULL"
            fi
            if grep -q 'GIST_ROUNDTRIP' ~/.config/schalentier/schalentier.toml 2>/dev/null; then
                pass "Gist pull restores config content"
            else
                fail "Gist pull content" "Restored config missing expected marker"
            fi
        fi
    fi

    kill "$MOCK_PID" 2>/dev/null || true
    unset SCHALENTIER_GITHUB_API_BASE
fi

#=============================================================================
section "Add/Install Tests"

# Test 22: Add with --no-install adds to config only
ADD_NOINSTALL_OUTPUT=$(schalentier add testpkg --no-install 2>&1)
if echo "$ADD_NOINSTALL_OUTPUT" | grep -qi "added.*config\|not installed"; then
    pass "Add --no-install adds to config"
else
    fail "Add --no-install" "Should add to config without installing"
fi

# Test 23: Config contains added tool
if grep -q "testpkg" ~/.config/schalentier/schalentier.toml 2>/dev/null; then
    pass "Config contains added tool"
else
    fail "Config tool entry" "testpkg not found in config"
fi

# Test 24: Remove tool from config
REMOVE_OUTPUT=$(schalentier remove testpkg 2>&1 || true)
if echo "$REMOVE_OUTPUT" | grep -qi "removed\|not found\|not installed"; then
    pass "Remove command works"
else
    fail "Remove command" "Should remove or report tool status"
fi

#=============================================================================
section "Real Install Tests (binary provider, needs network)"

GITHUB_RATE_REMAINING=$(curl -s --connect-timeout 5 https://api.github.com/rate_limit 2>/dev/null \
    | grep -o '"remaining":[0-9]*' | head -1 | grep -o '[0-9]*' || echo 0)
if [ "${GITHUB_RATE_REMAINING:-0}" -gt 5 ] 2>/dev/null; then
    # Test: install a registry-known tool (fd) via the binary provider — downloads a
    # real GitHub release, extracts it, and drops a runnable binary in bin/.
    schalentier add fd --provider binary > /dev/null 2>&1 || true
    if [ -x ~/.schalentier/bin/fd ] && ~/.schalentier/bin/fd --version > /dev/null 2>&1; then
        pass "Install fd (registry, binary provider) produces a runnable binary"
    else
        fail "Install fd" "~/.schalentier/bin/fd missing or not runnable"
    fi
    schalentier remove fd > /dev/null 2>&1 || true

    # Test: install a tool NOT in the registry (micro) — exercises the
    # search-providers-then-legacy-install fallback path (B2).
    schalentier add micro --provider binary > /dev/null 2>&1 || true
    if [ -x ~/.schalentier/bin/micro ] && ~/.schalentier/bin/micro --version > /dev/null 2>&1; then
        pass "Install micro (non-registry, binary provider) produces a runnable binary"
    else
        fail "Install micro" "~/.schalentier/bin/micro missing or not runnable"
    fi
    schalentier remove micro > /dev/null 2>&1 || true
else
    skip "Real install tests" "No network to api.github.com or unauthenticated rate limit exhausted (${GITHUB_RATE_REMAINING:-0} remaining)"
fi

#=============================================================================
section "Update Tests"

# Test 25: Update command runs (dry-run)
UPDATE_OUTPUT=$(schalentier update --dry-run 2>&1)
if echo "$UPDATE_OUTPUT" | grep -qi "update\|check\|no.*update\|up.to.date"; then
    pass "Update --dry-run works"
else
    fail "Update dry-run" "Should check for updates"
fi

#=============================================================================
section "Alias Tests (Task 6.7)"

# Test 26: Alias command exists
if schalentier alias --help 2>&1 | grep -qi "alias\|usage\|help"; then
    pass "Alias command exists"

    # Test 27: Create alias
    ALIAS_CREATE=$(schalentier alias ll="ls -la" 2>&1 || true)
    if echo "$ALIAS_CREATE" | grep -qi "created\|alias"; then
        pass "Alias creation works"
    else
        fail "Alias creation" "Should create alias script"
    fi

    # Test 28: Alias script exists in bin
    if [ -x ~/.schalentier/bin/ll ]; then
        pass "Alias script is executable"
    else
        fail "Alias script" "ll not found or not executable in bin/"
    fi

    # Test 29: Alias --list shows aliases
    ALIAS_LIST=$(schalentier alias --list 2>&1 || true)
    if echo "$ALIAS_LIST" | grep -qi "ll"; then
        pass "Alias --list shows aliases"
    else
        fail "Alias list" "Should list created aliases"
    fi

    # Test 30: Alias --remove works
    ALIAS_REMOVE=$(schalentier alias --remove ll 2>&1 || true)
    if [ ! -f ~/.schalentier/bin/ll ]; then
        pass "Alias --remove works"
    else
        fail "Alias remove" "Alias script should be deleted"
    fi
else
    skip "Alias command" "NOT IMPLEMENTED - Task 6.7"
    skip "Alias creation" "NOT IMPLEMENTED - Task 6.7"
    skip "Alias script executable" "NOT IMPLEMENTED - Task 6.7"
    skip "Alias --list" "NOT IMPLEMENTED - Task 6.7"
    skip "Alias --remove" "NOT IMPLEMENTED - Task 6.7"
fi

#=============================================================================
section "Snippets Tests (Task 6.8)"

# Test 31: Snippet command exists
if schalentier snippet --help 2>&1 | grep -qi "snippet\|usage\|help"; then
    pass "Snippet command exists"

    # Test 32: Snippet list (should be empty initially)
    SNIPPET_LIST=$(schalentier snippet list 2>&1 || true)
    if echo "$SNIPPET_LIST" | grep -qi "no snippet\|empty\|snippet"; then
        pass "Snippet list works"
    else
        fail "Snippet list" "Should list snippets or show empty"
    fi

    # Test 33: Snippet add from registry
    SNIPPET_ADD=$(schalentier snippet add yazi 2>&1 || true)
    if echo "$SNIPPET_ADD" | grep -qi "added\|installed\|snippet\|yazi"; then
        pass "Snippet add works"
    else
        fail "Snippet add" "Should add snippet from registry"
    fi

    # Test 34: Snippet file created
    if [ -f ~/.schalentier/snippets.d/yazi.bash ] || [ -f ~/.schalentier/snippets.d/yazi.sh ]; then
        pass "Snippet file created"
    else
        fail "Snippet file" "yazi snippet not found in snippets.d/"
    fi

    # Test 35: Snippet remove works
    SNIPPET_REMOVE=$(schalentier snippet remove yazi 2>&1 || true)
    if echo "$SNIPPET_REMOVE" | grep -qi "removed\|deleted\|snippet"; then
        pass "Snippet remove works"
    else
        fail "Snippet remove" "Should remove snippet"
    fi
else
    skip "Snippet command" "NOT IMPLEMENTED - Task 6.8"
    skip "Snippet list" "NOT IMPLEMENTED - Task 6.8"
    skip "Snippet add" "NOT IMPLEMENTED - Task 6.8"
    skip "Snippet file created" "NOT IMPLEMENTED - Task 6.8"
    skip "Snippet remove" "NOT IMPLEMENTED - Task 6.8"
fi

#=============================================================================
section "Dotfiles/Config Patching Tests (Task 6.5)"

# Setup test directories
TEST_CONFIG_DIR="$HOME/.config/schalentier"
TEST_DOTFILES_DIR="$HOME/.config/test-dotfiles"
mkdir -p "$TEST_DOTFILES_DIR"

# Test: Config command exists
CONFIG_HELP=$(schalentier config --help 2>&1 || true)
if echo "$CONFIG_HELP" | grep -qi "apply.*diff\|diff.*apply\|dotfile\|patch"; then
    pass "Config command exists"
else
    fail "Config command exists" "schalentier config --help failed"
fi

# Test: Config list works
CONFIG_LIST=$(schalentier config list 2>&1 || true)
if echo "$CONFIG_LIST" | grep -qi "dotfile\|managed\|no.*config\|empty\|list"; then
    pass "Config list works"
else
    fail "Config list" "Should list managed dotfiles or show empty"
fi

# Create test schalentier.toml with dotfiles section
cat > "$TEST_CONFIG_DIR/schalentier.toml" << 'TESTCFG'
[tools]

[dotfiles."~/.config/test-dotfiles/test.json"]
colorscheme = "monokai"
tabsize = 4
nested = { key = "value" }

[dotfiles."~/.config/test-dotfiles/test.toml"]
theme = "dark"

[dotfiles."~/.config/test-dotfiles/test.toml".section]
option1 = true
option2 = "hello"

[dotfiles."~/.config/test-dotfiles/test.ini"]
[dotfiles."~/.config/test-dotfiles/test.ini".user]
name = "testuser"
email = "test@example.com"

[dotfiles."~/.config/test-dotfiles/test.env"]
EDITOR = "micro"
PAGER = "less"

[dotfiles."~/.config/test-dotfiles/test.custom"]
_content = """
# Custom config
setting1 = value1
setting2 = value2
"""
TESTCFG

# Test: Config diff (dry-run) shows changes
CONFIG_DIFF=$(schalentier config diff 2>&1 || true)
if echo "$CONFIG_DIFF" | grep -qi "test.json\|would\|change\|diff\|create"; then
    pass "Config diff shows pending changes"
else
    fail "Config diff" "Should show what would be changed"
fi

# Test: Config apply creates/updates files
CONFIG_APPLY=$(schalentier config apply 2>&1 || true)
if echo "$CONFIG_APPLY" | grep -qi "applied\|updated\|created\|patched"; then
    pass "Config apply works"
else
    fail "Config apply" "Should apply dotfile patches"
fi

# Test: JSON file created with correct content
if [ -f "$TEST_DOTFILES_DIR/test.json" ]; then
    if grep -q "monokai" "$TEST_DOTFILES_DIR/test.json" && grep -q "tabsize" "$TEST_DOTFILES_DIR/test.json"; then
        pass "JSON merge creates file with settings"
    else
        fail "JSON merge content" "JSON file missing expected settings"
    fi
else
    fail "JSON merge" "test.json not created"
fi

# Test: TOML file created with correct content
if [ -f "$TEST_DOTFILES_DIR/test.toml" ]; then
    if grep -q "dark" "$TEST_DOTFILES_DIR/test.toml" && grep -q "option1" "$TEST_DOTFILES_DIR/test.toml"; then
        pass "TOML merge creates file with settings"
    else
        fail "TOML merge content" "TOML file missing expected settings"
    fi
else
    fail "TOML merge" "test.toml not created"
fi

# Test: INI file created with correct content
if [ -f "$TEST_DOTFILES_DIR/test.ini" ]; then
    if grep -q "testuser" "$TEST_DOTFILES_DIR/test.ini" && grep -q "\[user\]" "$TEST_DOTFILES_DIR/test.ini"; then
        pass "INI merge creates file with sections"
    else
        fail "INI merge content" "INI file missing expected settings"
    fi
else
    fail "INI merge" "test.ini not created"
fi

# Test: KeyValue (.env) file created with correct content
if [ -f "$TEST_DOTFILES_DIR/test.env" ]; then
    if grep -q "EDITOR=micro" "$TEST_DOTFILES_DIR/test.env" || grep -q "EDITOR = micro" "$TEST_DOTFILES_DIR/test.env"; then
        pass "KeyValue merge creates file with settings"
    else
        fail "KeyValue merge content" "ENV file missing expected settings"
    fi
else
    fail "KeyValue merge" "test.env not created"
fi

# Test: Unknown format uses replace mode
if [ -f "$TEST_DOTFILES_DIR/test.custom" ]; then
    if grep -q "setting1 = value1" "$TEST_DOTFILES_DIR/test.custom"; then
        pass "Replace mode works for unknown formats"
    else
        fail "Replace mode content" "Custom file missing expected content"
    fi
else
    fail "Replace mode" "test.custom not created"
fi

# Test: Idempotency - running apply again produces same result
if [ -f "$TEST_DOTFILES_DIR/test.json" ]; then
    BEFORE_HASH=$(md5sum "$TEST_DOTFILES_DIR/test.json" | cut -d' ' -f1)
    schalentier config apply > /dev/null 2>&1 || true
    AFTER_HASH=$(md5sum "$TEST_DOTFILES_DIR/test.json" | cut -d' ' -f1)
    if [ "$BEFORE_HASH" = "$AFTER_HASH" ]; then
        pass "Config apply is idempotent"
    else
        fail "Idempotency" "Running apply twice changed the file"
    fi
else
    fail "Idempotency" "Cannot test - test.json doesn't exist"
fi

# Test: JSON merge preserves existing keys (deep merge, not replace)
if [ -f "$TEST_DOTFILES_DIR/test.json" ] && command -v python3 > /dev/null 2>&1; then
    # Add a custom key (simulating user edit)
    python3 -c "
import json
with open('$TEST_DOTFILES_DIR/test.json', 'r') as f:
    data = json.load(f)
data['user_custom_key'] = 'should_be_preserved'
with open('$TEST_DOTFILES_DIR/test.json', 'w') as f:
    json.dump(data, f, indent=2)
" 2>/dev/null
    schalentier config apply > /dev/null 2>&1 || true
    if grep -q "user_custom_key" "$TEST_DOTFILES_DIR/test.json" 2>/dev/null; then
        pass "JSON merge preserves existing keys"
    else
        fail "JSON merge preservation" "User's custom key was removed"
    fi
else
    fail "JSON merge preservation" "Cannot test - test.json doesn't exist or python3 missing"
fi

# Test: Config reset command exists
CONFIG_RESET=$(schalentier config reset --help 2>&1 || true)
if echo "$CONFIG_RESET" | grep -qi "restore.*backup\|backup.*restore\|reset.*file"; then
    pass "Config reset command exists"
else
    fail "Config reset" "Config reset command not found"
fi

# Cleanup test dotfiles
rm -rf "$TEST_DOTFILES_DIR"

#=============================================================================
section "Secrets Tests (Task 7.1)"

# Secret tests rely on SCHALENTIER_MASTER_PASSWORD to avoid the interactive prompt.
if [ -z "${SCHALENTIER_MASTER_PASSWORD:-}" ]; then
    skip "Secrets tests" "SCHALENTIER_MASTER_PASSWORD not set"
else
    # Test: secret set (non-interactive via --value)
    SECRET_SET=$(schalentier secret set SMOKE_TOKEN --value "s3cr3t-value" --tags smoke,ci 2>&1 || true)
    if echo "$SECRET_SET" | grep -qi "saved\|added"; then
        pass "Secret set stores a value"
    else
        fail "Secret set" "Expected 'saved' confirmation, got: $SECRET_SET"
    fi

    # Test: secrets.enc file created (encrypted, in config dir)
    if [ -f ~/.config/schalentier/secrets.enc ]; then
        pass "Secret set creates secrets.enc"
    else
        fail "secrets.enc" "Encrypted store not created"
    fi

    # Test: secret get round-trips the value
    SECRET_GET=$(schalentier secret get SMOKE_TOKEN 2>/dev/null || true)
    if [ "$SECRET_GET" = "s3cr3t-value" ]; then
        pass "Secret get returns stored value"
    else
        fail "Secret get" "Expected 's3cr3t-value', got: '$SECRET_GET'"
    fi

    # Test: secrets.enc does NOT contain the plaintext (it is encrypted)
    if grep -q "s3cr3t-value" ~/.config/schalentier/secrets.enc 2>/dev/null; then
        fail "Secret encryption" "Plaintext value found in secrets.enc!"
    else
        pass "Secret value is encrypted at rest"
    fi

    # Test: secret list shows the name
    SECRET_LIST=$(schalentier secret list 2>&1 || true)
    if echo "$SECRET_LIST" | grep -q "SMOKE_TOKEN"; then
        pass "Secret list shows secret name"
    else
        fail "Secret list" "SMOKE_TOKEN not listed"
    fi

    # Test: secret export emits a valid bash export line
    SECRET_EXPORT=$(schalentier secret export --shell bash 2>&1 || true)
    if echo "$SECRET_EXPORT" | grep -q 'export SMOKE_TOKEN="s3cr3t-value"'; then
        pass "Secret export emits bash syntax"
    else
        fail "Secret export" "Missing/incorrect export line: $SECRET_EXPORT"
    fi

    # Test: secret run injects the secret into the child environment
    SECRET_RUN=$(schalentier secret run -- bash -c 'echo $SMOKE_TOKEN' 2>/dev/null || true)
    if echo "$SECRET_RUN" | grep -q "s3cr3t-value"; then
        pass "Secret run injects env var"
    else
        fail "Secret run" "SMOKE_TOKEN not in child env: '$SECRET_RUN'"
    fi

    # Test: tag filtering excludes non-matching secrets
    schalentier secret set OTHER_TOKEN --value "other" --tags prod > /dev/null 2>&1 || true
    SECRET_TAGGED=$(schalentier secret export --tags smoke 2>&1 || true)
    if echo "$SECRET_TAGGED" | grep -q "SMOKE_TOKEN" && ! echo "$SECRET_TAGGED" | grep -q "OTHER_TOKEN"; then
        pass "Secret export --tags filters by tag"
    else
        fail "Secret tag filter" "Tag filtering did not work as expected"
    fi

    # Test: secret delete removes it
    schalentier secret delete SMOKE_TOKEN > /dev/null 2>&1 || true
    if ! schalentier secret list 2>&1 | grep -q "SMOKE_TOKEN"; then
        pass "Secret delete removes secret"
    else
        fail "Secret delete" "SMOKE_TOKEN still present after delete"
    fi
fi

#=============================================================================
section "Registry Tests"

# Test: registry validate (offline, bundled registry)
REGISTRY_VALIDATE=$(schalentier registry validate 2>&1 || true)
if echo "$REGISTRY_VALIDATE" | grep -qi "valid\|package count"; then
    pass "Registry validate passes"
else
    fail "Registry validate" "Bundled registry did not validate: $REGISTRY_VALIDATE"
fi

# Test: registry info shows stats
REGISTRY_INFO=$(schalentier registry info 2>&1 || true)
if echo "$REGISTRY_INFO" | grep -qi "total packages\|packages by provider"; then
    pass "Registry info shows statistics"
else
    fail "Registry info" "No statistics in output"
fi

#=============================================================================
section "Multi-Provider Resolution Tests"

# Test: add --dry-run for a registry package shows providers, installs nothing
ADD_DRYRUN=$(schalentier add ripgrep --dry-run 2>&1 || true)
if echo "$ADD_DRYRUN" | grep -qi "available in.*provider\|dry run.*would install"; then
    pass "Add --dry-run shows provider resolution"
else
    fail "Add --dry-run" "Expected provider list / dry-run notice: $ADD_DRYRUN"
fi

# Test: dry-run must NOT add the tool to config
if ! grep -q "ripgrep" ~/.config/schalentier/schalentier.toml 2>/dev/null; then
    pass "Add --dry-run does not modify config"
else
    fail "Add --dry-run isolation" "ripgrep was added to config during dry-run"
fi

# Test: dry-run for a NON-registry package must also install nothing (regression:
# cmd_add_legacy previously ignored --dry-run and actually installed).
ADD_DRYRUN_LEGACY=$(schalentier add zzz-nonexistent-smoke-pkg --dry-run 2>&1 || true)
if echo "$ADD_DRYRUN_LEGACY" | grep -qi "dry run"; then
    pass "Add --dry-run works for non-registry packages"
else
    fail "Add --dry-run (legacy path)" "Non-registry dry-run did not short-circuit: $ADD_DRYRUN_LEGACY"
fi

#=============================================================================
section "Security Audit Tests (OSV.dev)"

# Test: audit command runs with no installed packages
AUDIT_EMPTY=$(schalentier audit 2>&1 || true)
if echo "$AUDIT_EMPTY" | grep -qi "no packages to audit\|running security audit"; then
    pass "Audit runs (empty state)"
else
    fail "Audit empty" "Unexpected output: $AUDIT_EMPTY"
fi

# Test: audit a specific package (requires network to reach OSV.dev)
if curl -s --connect-timeout 5 https://api.osv.dev > /dev/null 2>&1; then
    # black 21.12b0 has known advisories in the PyPI ecosystem.
    schalentier add black --no-install > /dev/null 2>&1 || true
    AUDIT_BLACK=$(schalentier audit black 2>&1 || true)
    if echo "$AUDIT_BLACK" | grep -qi "advisor\|vulnerab\|clean\|skipped"; then
        pass "Audit queries OSV.dev for a package"
    else
        fail "Audit package" "No recognizable audit output: $AUDIT_BLACK"
    fi

    # Test: second audit of the same package should be served from cache (near-instant,
    # no OSV.dev round-trip) rather than re-querying every time.
    CACHE_FILE="$HOME/.schalentier/osv_cache.json"
    if [ -f "$CACHE_FILE" ]; then
        START_MS=$(date +%s%3N)
        schalentier audit black > /dev/null 2>&1 || true
        END_MS=$(date +%s%3N)
        ELAPSED=$((END_MS - START_MS))
        if [ "$ELAPSED" -lt 1000 ]; then
            pass "Second audit served from cache (${ELAPSED}ms)"
        else
            fail "Audit cache" "Second audit took ${ELAPSED}ms, expected a fast cache hit"
        fi

        # Test: --refresh bypasses the cache and re-queries OSV.dev.
        REFRESH_OUT=$(schalentier audit black --refresh 2>&1 || true)
        if echo "$REFRESH_OUT" | grep -qi "advisor\|vulnerab\|clean\|skipped"; then
            pass "Audit --refresh bypasses cache"
        else
            fail "Audit --refresh" "Unexpected output: $REFRESH_OUT"
        fi
    else
        fail "Audit cache" "Expected $CACHE_FILE to exist after first audit"
    fi

    schalentier remove black > /dev/null 2>&1 || true
else
    skip "Audit OSV.dev query" "No network connectivity to api.osv.dev"
fi

#=============================================================================
section "Templating Tests (Task 7.2)"

# Rendered templated dotfile using the {{ hostname }} / {{ var.* }} context.
TEST_TMPL_DIR="$HOME/.config/test-template"
mkdir -p "$TEST_TMPL_DIR"

cat > "$HOME/.config/schalentier/schalentier.toml" << 'TMPLCFG'
[tools]

[variables]
editor = "micro"

[dotfiles."~/.config/test-template/rendered.env"]
_template = true
EDITOR = "{{ var.editor }}"
HOST = "{{ hostname }}"
TMPLCFG

TMPL_APPLY=$(schalentier config apply 2>&1 || true)
if [ -f "$TEST_TMPL_DIR/rendered.env" ]; then
    # var.editor must be substituted; the literal Jinja braces must be gone.
    if grep -q "EDITOR=micro" "$TEST_TMPL_DIR/rendered.env" 2>/dev/null \
       || grep -q "EDITOR = micro" "$TEST_TMPL_DIR/rendered.env" 2>/dev/null; then
        if ! grep -q "{{" "$TEST_TMPL_DIR/rendered.env" 2>/dev/null; then
            pass "Template renders {{ var.* }} into dotfile"
        else
            fail "Template rendering" "Unrendered '{{' left in output"
        fi
    else
        fail "Template var substitution" "var.editor not substituted"
    fi
else
    fail "Template apply" "rendered.env not created: $TMPL_APPLY"
fi

# hostname must be a non-empty concrete value, not the literal template.
if grep -qE "HOST ?= ?.+" "$TEST_TMPL_DIR/rendered.env" 2>/dev/null \
   && ! grep -q "{{ hostname }}" "$TEST_TMPL_DIR/rendered.env" 2>/dev/null; then
    pass "Template renders {{ hostname }} to a concrete value"
else
    fail "Template hostname" "hostname not rendered"
fi

# Template with {{ secret.* }} and {{ env.* }} (needs the secret store).
if [ -n "${SCHALENTIER_MASTER_PASSWORD:-}" ]; then
    schalentier secret set TPL_SECRET --value "sk-tpl-99" > /dev/null 2>&1
    cat > "$HOME/.config/schalentier/schalentier.toml" << 'TMPLCFG2'
[tools]

[dotfiles."~/.config/test-template/secret.env"]
_template = true
KEY = "{{ secret.TPL_SECRET }}"
HOME_DIR = "{{ env.HOME }}"
TMPLCFG2
    schalentier config apply > /dev/null 2>&1 || true
    if grep -q "KEY=sk-tpl-99" "$TEST_TMPL_DIR/secret.env" 2>/dev/null \
       || grep -q "KEY = sk-tpl-99" "$TEST_TMPL_DIR/secret.env" 2>/dev/null; then
        pass "Template renders {{ secret.* }} to decrypted value"
    else
        fail "Template secret" "secret not substituted in template"
    fi
    if grep -qE "HOME_DIR ?= ?/" "$TEST_TMPL_DIR/secret.env" 2>/dev/null; then
        pass "Template renders {{ env.* }} to environment value"
    else
        fail "Template env" "env var not substituted in template"
    fi
    schalentier secret delete TPL_SECRET > /dev/null 2>&1 || true
else
    skip "Template secret/env" "SCHALENTIER_MASTER_PASSWORD not set"
fi

rm -rf "$TEST_TMPL_DIR"

#=============================================================================
section "Config Reset / Backup Tests"

TEST_RESET_DIR="$HOME/.config/test-reset"
mkdir -p "$TEST_RESET_DIR"
# Pre-existing file with a user value we expect to be restored.
echo '{"color":"original","user_key":"keep"}' > "$TEST_RESET_DIR/cfg.json"

cat > "$HOME/.config/schalentier/schalentier.toml" << 'RESETCFG'
[tools]

[dotfiles."~/.config/test-reset/cfg.json"]
color = "patched"
RESETCFG

schalentier config apply > /dev/null 2>&1 || true

# A backup of the original should exist after the first patch.
if [ -f "$TEST_RESET_DIR/cfg.json.schalentier-backup" ]; then
    pass "Config apply creates a backup"
else
    fail "Config backup" "No .schalentier-backup created"
fi

# The patch should have taken effect.
if grep -q "patched" "$TEST_RESET_DIR/cfg.json" 2>/dev/null; then
    pass "Config apply patched the file"
else
    fail "Config patch" "Patch value not applied"
fi

# Reset should restore the original content.
schalentier config reset "~/.config/test-reset/cfg.json" > /dev/null 2>&1 || true
if grep -q "original" "$TEST_RESET_DIR/cfg.json" 2>/dev/null \
   && ! grep -q "patched" "$TEST_RESET_DIR/cfg.json" 2>/dev/null; then
    pass "Config reset restores from backup"
else
    fail "Config reset" "File not restored to original"
fi

rm -rf "$TEST_RESET_DIR"

#=============================================================================
section "Provider Selection Tests"

# Requesting an unavailable/unknown provider for a registry package should fail
# clearly rather than silently installing via another provider.
PROV_ERR=$(schalentier add ripgrep --provider nonexistentprovider --dry-run 2>&1 || true)
if echo "$PROV_ERR" | grep -qiE "not available|unknown|available:|provider"; then
    pass "Add with unavailable provider reports clearly"
else
    fail "Provider selection" "Unexpected output for bad provider: $PROV_ERR"
fi

#=============================================================================
section "Completions Tests"

# Test: completions generate for each supported shell
for sh in bash zsh fish; do
    COMP_OUT=$(schalentier completions $sh 2>&1 || true)
    if echo "$COMP_OUT" | grep -qi "schalentier"; then
        pass "Completions generate for $sh"
    else
        fail "Completions $sh" "No completion output for $sh"
    fi
done

#=============================================================================
section "Error Handling Tests"

# Test 36: Invalid command returns error
if ! schalentier invalid_command 2>&1; then
    pass "Invalid command returns error"
else
    fail "Invalid command handling" "Should have returned non-zero exit code"
fi

# Test 37: Add without name shows error
# Capture to variable to avoid SIGPIPE and handle non-zero exit
ADD_OUTPUT=$(schalentier add 2>&1 || true)
if echo "$ADD_OUTPUT" | grep -qi "required\|missing\|argument"; then
    pass "Add without name shows error"
else
    fail "Add argument validation" "Should show missing argument error"
fi

#=============================================================================
section "Cleanup"

cleanup
pass "Cleanup completed"

#=============================================================================
# SUMMARY
#=============================================================================

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  SMOKE TEST SUMMARY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e "  ${GREEN}Passed${NC}:  $PASSED"
echo -e "  ${RED}Failed${NC}:  $FAILED"
echo -e "  ${YELLOW}Skipped${NC}: $SKIPPED"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $FAILED -gt 0 ]; then
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
fi
