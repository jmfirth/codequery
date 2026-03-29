#!/usr/bin/env bash
# Smoke test: run cq against real open-source projects to find crashes,
# TSG warnings, and resolution quality issues.
#
# Usage: ./scripts/smoke-test.sh [language]
# If no language specified, tests all languages.

set -euo pipefail

CQ="cargo run --release --"
TMPDIR="${SMOKE_TEST_DIR:-/tmp/cq-smoke-test}"
RESULTS=""
FAILURES=0
WARNINGS=0

mkdir -p "$TMPDIR"

# Build release binary first
echo "Building release binary..."
cargo build --release 2>/dev/null

smoke_test() {
    local lang="$1" repo="$2" dir_name="$3" test_file="$4" test_symbol="$5"
    local project_dir="$TMPDIR/$dir_name"

    echo ""
    echo "=== $lang: $dir_name ==="

    # Clone if not already present
    if [ ! -d "$project_dir" ]; then
        echo "  Cloning $repo (shallow)..."
        git clone --depth 1 --quiet "$repo" "$project_dir" 2>/dev/null || {
            echo "  SKIP: clone failed"
            RESULTS+="| $lang | $dir_name | SKIP | clone failed |\n"
            return
        }
    fi

    # Test 1: outline a representative file
    local outline_file="$project_dir/$test_file"
    if [ -f "$outline_file" ]; then
        local outline_out
        outline_out=$($CQ outline "$outline_file" 2>&1) || true
        local outline_symbols=$(echo "$outline_out" | grep -c "@@" || echo "0")
        echo "  outline $test_file: $outline_symbols symbols"
        if echo "$outline_out" | grep -qi "panic\|thread.*panicked\|SIGSEGV\|segfault"; then
            echo "  FAIL: outline crashed"
            FAILURES=$((FAILURES + 1))
            RESULTS+="| $lang | $dir_name | FAIL | outline crashed |\n"
            return
        fi
    else
        echo "  WARN: $test_file not found, skipping outline"
    fi

    # Test 2: refs for a common symbol
    local refs_json refs_stderr
    refs_stderr=$($CQ refs "$test_symbol" --json --project "$project_dir" 2>&1 1>/tmp/cq-smoke-refs.json) || true
    refs_json=$(cat /tmp/cq-smoke-refs.json 2>/dev/null || echo "{}")

    if echo "$refs_stderr" | grep -qi "panic"; then
        echo "  FAIL: refs panicked"
        echo "  stderr: $(echo "$refs_stderr" | head -3)"
        FAILURES=$((FAILURES + 1))
        RESULTS+="| $lang | $dir_name | FAIL | refs panicked |\n"
        return
    fi

    local resolution total
    resolution=$(echo "$refs_json" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('resolution','none'))" 2>/dev/null || echo "parse_error")
    total=$(echo "$refs_json" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('total',0))" 2>/dev/null || echo "0")
    echo "  refs '$test_symbol': resolution=$resolution, total=$total"

    # Test 3: symbols scan (exercises parser on ALL files)
    local symbols_json symbols_stderr
    symbols_stderr=$($CQ symbols --json --project "$project_dir" --limit 5 2>&1 1>/tmp/cq-smoke-symbols.json) || true
    symbols_json=$(cat /tmp/cq-smoke-symbols.json 2>/dev/null || echo "{}")

    if echo "$symbols_stderr" | grep -qi "panic"; then
        echo "  FAIL: symbols panicked"
        FAILURES=$((FAILURES + 1))
        RESULTS+="| $lang | $dir_name | FAIL | symbols panicked |\n"
        return
    fi

    local sym_total
    sym_total=$(echo "$symbols_json" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('total',0))" 2>/dev/null || echo "0")
    echo "  symbols scan: $sym_total total symbols"

    # Test 4: tree on the test file (parser stress test)
    if [ -f "$outline_file" ]; then
        local tree_out
        tree_out=$($CQ tree --project "$project_dir" 2>&1 | head -5) || true
        local tree_files=$(echo "$tree_out" | grep -c "/" || echo "0")
        echo "  tree: $tree_files files"
    fi

    # Report
    if [ "$resolution" = "resolved" ]; then
        echo "  PASS (resolved, $total refs, $sym_total symbols)"
        RESULTS+="| $lang | $dir_name | PASS | resolved, $total refs, $sym_total symbols |\n"
    elif [ "$resolution" = "syntactic" ] && [ "$total" != "0" ]; then
        echo "  OK (syntactic fallback, $total refs, $sym_total symbols)"
        RESULTS+="| $lang | $dir_name | OK | syntactic, $total refs, $sym_total symbols |\n"
    elif [ "$total" = "0" ]; then
        echo "  WARN: 0 refs for '$test_symbol' ($sym_total symbols)"
        WARNINGS=$((WARNINGS + 1))
        RESULTS+="| $lang | $dir_name | WARN | 0 refs, $sym_total symbols |\n"
    else
        echo "  OK ($resolution, $total refs)"
        RESULTS+="| $lang | $dir_name | OK | $resolution, $total refs |\n"
    fi
}

# ─── Rust ───────────────────────────────────────────────────
run_rust() {
    smoke_test "Rust" "https://github.com/BurntSushi/ripgrep" "ripgrep" \
        "crates/core/main.rs" "run"
    smoke_test "Rust" "https://github.com/serde-rs/serde" "serde" \
        "serde/src/lib.rs" "Serialize"
}

# ─── Go ─────────────────────────────────────────────────────
run_go() {
    smoke_test "Go" "https://github.com/gin-gonic/gin" "gin" \
        "gin.go" "New"
    smoke_test "Go" "https://github.com/spf13/cobra" "cobra" \
        "command.go" "Execute"
}

# ─── C ──────────────────────────────────────────────────────
run_c() {
    smoke_test "C" "https://github.com/redis/redis" "redis" \
        "src/server.c" "serverLog"
    smoke_test "C" "https://github.com/jqlang/jq" "jq" \
        "src/main.c" "main"
}

# ─── C++ ────────────────────────────────────────────────────
run_cpp() {
    smoke_test "C++" "https://github.com/nlohmann/json" "nlohmann-json" \
        "include/nlohmann/json.hpp" "parse"
    smoke_test "C++" "https://github.com/fmtlib/fmt" "fmt" \
        "include/fmt/core.h" "format"
}

# ─── Python ─────────────────────────────────────────────────
run_python() {
    smoke_test "Python" "https://github.com/pallets/flask" "flask" \
        "src/flask/app.py" "Flask"
    smoke_test "Python" "https://github.com/psf/requests" "requests" \
        "src/requests/api.py" "get"
}

# ─── TypeScript ─────────────────────────────────────────────
run_typescript() {
    smoke_test "TypeScript" "https://github.com/colinhacks/zod" "zod" \
        "src/types.ts" "ZodType"
}

# ─── JavaScript ─────────────────────────────────────────────
run_javascript() {
    smoke_test "JavaScript" "https://github.com/expressjs/express" "express" \
        "lib/express.js" "createApplication"
}

# ─── Java ───────────────────────────────────────────────────
run_java() {
    smoke_test "Java" "https://github.com/google/gson" "gson" \
        "gson/src/main/java/com/google/gson/Gson.java" "fromJson"
}

# ─── Ruby ───────────────────────────────────────────────────
run_ruby() {
    smoke_test "Ruby" "https://github.com/sinatra/sinatra" "sinatra" \
        "lib/sinatra/base.rb" "get"
    smoke_test "Ruby" "https://github.com/rack/rack" "rack" \
        "lib/rack/request.rb" "Request"
}

# ─── C# ─────────────────────────────────────────────────────
run_csharp() {
    smoke_test "C#" "https://github.com/JamesNK/Newtonsoft.Json" "newtonsoft-json" \
        "Src/Newtonsoft.Json/JsonConvert.cs" "SerializeObject"
}

# ─── Main ───────────────────────────────────────────────────
target="${1:-all}"

case "$target" in
    rust)       run_rust ;;
    go)         run_go ;;
    c)          run_c ;;
    cpp|c++)    run_cpp ;;
    python)     run_python ;;
    typescript) run_typescript ;;
    javascript) run_javascript ;;
    java)       run_java ;;
    ruby)       run_ruby ;;
    csharp|c#)  run_csharp ;;
    all)
        run_rust; run_go; run_c; run_cpp
        run_python; run_typescript; run_javascript; run_java
        run_ruby; run_csharp
        ;;
    *) echo "Unknown language: $target"; exit 1 ;;
esac

echo ""
echo "════════════════════════════════════════════════════"
echo "  SMOKE TEST RESULTS"
echo "════════════════════════════════════════════════════"
echo ""
echo "| Language | Project | Status | Details |"
echo "|----------|---------|--------|---------|"
echo -e "$RESULTS"
echo ""
echo "Failures: $FAILURES  Warnings: $WARNINGS"

if [ "$FAILURES" -gt 0 ]; then
    echo "SOME TESTS FAILED"
    exit 1
else
    echo "ALL TESTS PASSED"
fi
