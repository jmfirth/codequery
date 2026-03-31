#!/usr/bin/env bash
#
# validate-languages.sh — End-to-end validation of cq against real open-source projects.
#
# Clones a representative project for each language, runs a suite of cq commands,
# and reports pass/fail per language. Repos are cached between runs.
#
# Usage:
#   ./scripts/validate-languages.sh              # validate all languages
#   ./scripts/validate-languages.sh rust python   # validate specific languages
#   CQ_BIN=./target/release/cq ./scripts/validate-languages.sh  # use release binary
#
# Environment:
#   CQ_BIN          Path to cq binary (default: ./target/debug/cq)
#   CQ_TEST_CACHE   Directory for cloned repos (default: /tmp/cq-test-repos)
#   CQ_VERBOSE      Set to 1 for full command output

set -euo pipefail

CQ="${CQ_BIN:-./target/debug/cq}"
CACHE_DIR="${CQ_TEST_CACHE:-/tmp/cq-test-repos}"
MANIFEST="$(dirname "$0")/test-repos.json"
VERBOSE="${CQ_VERBOSE:-0}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Counters
TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0
ERRORS=()

# Check prerequisites
if [ ! -f "$CQ" ]; then
    echo "Error: cq binary not found at $CQ"
    echo "Build with: cargo build --features test-all-langs"
    exit 1
fi

if [ ! -f "$MANIFEST" ]; then
    echo "Error: manifest not found at $MANIFEST"
    exit 1
fi

# Ensure cache directory exists
mkdir -p "$CACHE_DIR"

# Parse manifest — requires python3 for JSON parsing
get_languages() {
    python3 -c "
import json, sys
with open('$MANIFEST') as f:
    data = json.load(f)
for lang in sorted(data['repos'].keys()):
    print(lang)
"
}

get_field() {
    local lang="$1" field="$2"
    python3 -c "
import json
with open('$MANIFEST') as f:
    data = json.load(f)
entry = data['repos'].get('$lang', {})
print(entry.get('$field', ''))
"
}

# Clone a repo if not already cached
clone_repo() {
    local lang="$1" repo="$2"
    local repo_dir="$CACHE_DIR/$lang"

    if [ -d "$repo_dir/.git" ]; then
        return 0
    fi

    echo -ne "  cloning ${repo}... "
    if git clone --depth 1 --quiet "https://github.com/${repo}.git" "$repo_dir" 2>/dev/null; then
        echo "ok"
        return 0
    else
        echo "FAILED"
        return 1
    fi
}

# Run a single cq command and check for success
run_check() {
    local label="$1"
    shift
    local output
    local exit_code

    output=$("$@" 2>&1) || true
    exit_code=${PIPESTATUS[0]:-$?}

    # Exit code 0 = success, 1 = no results (acceptable for some commands)
    if [ "$exit_code" -eq 0 ] || [ "$exit_code" -eq 1 ]; then
        if [ "$VERBOSE" = "1" ]; then
            echo -e "    ${GREEN}✓${NC} $label"
        fi
        return 0
    else
        if [ "$VERBOSE" = "1" ]; then
            echo -e "    ${RED}✗${NC} $label (exit $exit_code)"
            echo "$output" | head -3 | sed 's/^/      /'
        fi
        return 1
    fi
}

# Validate a single language
validate_language() {
    local lang="$1"
    local repo file symbol search_pattern
    repo=$(get_field "$lang" "repo")
    file=$(get_field "$lang" "file")
    symbol=$(get_field "$lang" "symbol")
    search_pattern=$(get_field "$lang" "search_pattern")

    if [ -z "$repo" ]; then
        echo -e "  ${YELLOW}SKIP${NC} $lang — no repo configured"
        SKIPPED=$((SKIPPED + 1))
        return 0
    fi

    TOTAL=$((TOTAL + 1))

    # Clone
    if ! clone_repo "$lang" "$repo"; then
        echo -e "  ${RED}FAIL${NC} $lang — clone failed"
        FAILED=$((FAILED + 1))
        ERRORS+=("$lang: clone failed")
        return 0
    fi

    local repo_dir="$CACHE_DIR/$lang"
    local pass=0
    local fail=0

    # Test suite — each command that makes sense
    # 1. outline (if file specified)
    if [ -n "$file" ] && [ -f "$repo_dir/$file" ]; then
        if run_check "outline $file" "$CQ" outline "$repo_dir/$file" --project "$repo_dir"; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # 2. def (if symbol specified)
    if [ -n "$symbol" ]; then
        if run_check "def $symbol" "$CQ" def "$symbol" --project "$repo_dir"; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # 3. body (if symbol specified)
    if [ -n "$symbol" ]; then
        if run_check "body $symbol" "$CQ" body "$symbol" --project "$repo_dir"; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # 4. symbols (project-wide scan)
    if run_check "symbols" "$CQ" symbols --project "$repo_dir" --limit 10; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
    fi

    # 5. tree (project structure)
    if run_check "tree" "$CQ" tree --project "$repo_dir" --depth 0; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
    fi

    # 6. search (if pattern specified)
    if [ -n "$search_pattern" ]; then
        if run_check "search" "$CQ" search "$search_pattern" --project "$repo_dir" --limit 5; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # 7. diagnostics (if file specified)
    if [ -n "$file" ] && [ -f "$repo_dir/$file" ]; then
        if run_check "diagnostics $file" "$CQ" diagnostics "$repo_dir/$file" --project "$repo_dir"; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # Report
    local total_checks=$((pass + fail))
    if [ "$fail" -eq 0 ]; then
        echo -e "  ${GREEN}PASS${NC} $lang — ${pass}/${total_checks} commands"
        PASSED=$((PASSED + 1))
    else
        echo -e "  ${RED}FAIL${NC} $lang — ${pass}/${total_checks} commands (${fail} failed)"
        FAILED=$((FAILED + 1))
        ERRORS+=("$lang: ${fail}/${total_checks} commands failed")
    fi
}

# Main
echo -e "${BOLD}cq language validation${NC}"
echo -e "Binary: $CQ"
echo -e "Cache:  $CACHE_DIR"
echo ""

# Get language list
if [ $# -gt 0 ]; then
    LANGS=("$@")
else
    LANGS=()
    while IFS= read -r lang; do
        LANGS+=("$lang")
    done < <(get_languages)
fi

echo -e "${BOLD}Validating ${#LANGS[@]} languages...${NC}"
echo ""

for lang in "${LANGS[@]}"; do
    validate_language "$lang"
done

echo ""
echo -e "${BOLD}Results${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Skipped:${NC} $SKIPPED"
echo -e "  Total:   $TOTAL"

if [ ${#ERRORS[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}Failures:${NC}"
    for err in "${ERRORS[@]}"; do
        echo "  - $err"
    done
fi

echo ""
if [ "$FAILED" -eq 0 ]; then
    echo -e "${GREEN}${BOLD}All languages passed.${NC}"
    exit 0
else
    echo -e "${RED}${BOLD}${FAILED} language(s) failed.${NC}"
    exit 1
fi
