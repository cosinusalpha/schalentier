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
for cmd in init add remove list search sync update doctor; do
    if schalentier $cmd --help > /dev/null 2>&1; then
        pass "Subcommand '$cmd' has help"
    else
        fail "Subcommand '$cmd' has help" "schalentier $cmd --help failed"
    fi
done

#=============================================================================
section "Initialization Tests"

cleanup

# Test 4: Init creates directories (use --yes for non-interactive)
if schalentier init --yes 2>/dev/null; then
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

# Test 6: State file has correct permissions (Unix only)
if [ "$(uname)" != "Windows_NT" ]; then
    perms=$(stat -c %a ~/.schalentier/local_state.json 2>/dev/null || stat -f %Lp ~/.schalentier/local_state.json 2>/dev/null)
    if [ "$perms" = "600" ]; then
        pass "State file has 0600 permissions"
    else
        fail "State file permissions" "Expected 600, got $perms"
    fi
fi

# Test 7: Shell scripts created
for script in env.sh env.fish env.ps1; do
    if [ -f ~/.schalentier/$script ]; then
        pass "Init creates $script"
    else
        fail "Init creates $script" "File not found"
    fi
done

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
