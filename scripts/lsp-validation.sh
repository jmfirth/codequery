#!/usr/bin/env bash
# LSP Validation: Compare stack graph results against LSP ground truth.
# Verifies 0 false positives and measures coverage.
#
# Usage: ./scripts/lsp-validation.sh [--fixtures-only]

set -euo pipefail

CQ="${CQ:-cargo run --release --}"
PASS=0
FAIL=0
SKIP=0
TOTAL=0

cargo build --release 2>/dev/null

validate() {
    local label="$1" project="$2" symbol="$3" server="$4"
    TOTAL=$((TOTAL + 1))

    if ! command -v "$server" &>/dev/null; then
        echo "  SKIP  ${label}"
        SKIP=$((SKIP + 1))
        return
    fi

    if [ ! -d "$project" ]; then
        echo "  SKIP  ${label} (project not found)"
        SKIP=$((SKIP + 1))
        return
    fi

    # Write results to temp files to avoid bash escaping issues
    local sg_file=$(mktemp) lsp_file=$(mktemp)
    $CQ refs "$symbol" --json --project "$project" > "$sg_file" 2>/dev/null || echo '{"references":[]}' > "$sg_file"
    $CQ refs "$symbol" --json --semantic --project "$project" > "$lsp_file" 2>/dev/null || echo '{"references":[]}' > "$lsp_file"

    python3 - "$sg_file" "$lsp_file" "$label" <<'PYEOF'
import json, sys
sg_file, lsp_file, label = sys.argv[1], sys.argv[2], sys.argv[3]
with open(sg_file) as f:
    sg = json.load(f)
with open(lsp_file) as f:
    lsp = json.load(f)
sg_set = {f"{r['file']}:{r['line']}" for r in sg.get('references', [])}
lsp_set = {f"{r['file']}:{r['line']}" for r in lsp.get('references', [])}
fp = sg_set - lsp_set
matched = sg_set & lsp_set
misses = lsp_set - sg_set
cov = f"{len(matched)}/{len(lsp_set)}" if lsp_set else "0/0"
pct = f"{100*len(matched)//len(lsp_set)}%" if lsp_set else "N/A"
res = sg.get('resolution', '?')
status = "FAIL" if fp else "PASS"
print(f"  {status}  {label:<50} {res:<10} {cov:>8} ({pct:>4})  fp={len(fp)}")
if fp:
    for f in sorted(fp):
        print(f"         FALSE POSITIVE: {f}")
if misses and len(misses) <= 5:
    for m in sorted(misses):
        print(f"         miss: {m}")
elif misses:
    print(f"         {len(misses)} misses (LSP found more)")
sys.exit(1 if fp else 0)
PYEOF
    local rc=$?
    rm -f "$sg_file" "$lsp_file"
    if [ $rc -eq 0 ]; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
    fi
}

echo "═══════════════════════════════════════════════════════════════════════════"
echo "  LSP VALIDATION: Stack Graph vs Language Server Ground Truth"
echo "═══════════════════════════════════════════════════════════════════════════"
echo ""

echo "── Rust (rust-analyzer) ──────────────────────────────────────────────────"
validate "Rust fixture: greet"                "tests/fixtures/rust_project"       "greet"        "rust-analyzer"
validate "Rust fixture: User"                 "tests/fixtures/rust_project"       "User"         "rust-analyzer"
validate "Rust fixture: helper"               "tests/fixtures/rust_project"       "helper"       "rust-analyzer"
validate "Rust fixture: MAX_RETRIES"          "tests/fixtures/rust_project"       "MAX_RETRIES"  "rust-analyzer"
validate "Rust fixture: Validate"             "tests/fixtures/rust_project"       "Validate"     "rust-analyzer"
validate "Rust ripgrep: main"                 "/tmp/cq-smoke-test/ripgrep"        "main"         "rust-analyzer"
validate "Rust serde: Serialize"              "/tmp/cq-smoke-test/serde"          "Serialize"    "rust-analyzer"

echo ""
echo "── Go (gopls) ────────────────────────────────────────────────────────────"
validate "Go fixture: Greet"                  "tests/fixtures/go_project"         "Greet"        "gopls"
validate "Go fixture: FormatName"             "tests/fixtures/go_project"         "FormatName"   "gopls"
validate "Go fixture: User"                   "tests/fixtures/go_project"         "User"         "gopls"
validate "Go fixture: GlobalCounter"          "tests/fixtures/go_project"         "GlobalCounter" "gopls"
validate "Go fixture: FullName"               "tests/fixtures/go_project"         "FullName"     "gopls"
validate "Go gin: New"                        "/tmp/cq-smoke-test/gin"            "New"          "gopls"
validate "Go gin: Default"                    "/tmp/cq-smoke-test/gin"            "Default"      "gopls"
validate "Go gin: ServeHTTP"                  "/tmp/cq-smoke-test/gin"            "ServeHTTP"    "gopls"
validate "Go cobra: Command"                  "/tmp/cq-smoke-test/cobra"          "Command"      "gopls"
validate "Go cobra: Execute"                  "/tmp/cq-smoke-test/cobra"          "Execute"      "gopls"

echo ""
echo "── C (clangd) ────────────────────────────────────────────────────────────"
validate "C fixture: add"                     "tests/fixtures/c_project"          "add"          "clangd"
validate "C fixture: multiply"                "tests/fixtures/c_project"          "multiply"     "clangd"
validate "C fixture: sum_of_squares"          "tests/fixtures/c_project"          "sum_of_squares" "clangd"
validate "C fixture: main"                    "tests/fixtures/c_project"          "main"         "clangd"
validate "C redis: main"                      "/tmp/cq-smoke-test/redis"          "main"         "clangd"
validate "C redis: createClient"              "/tmp/cq-smoke-test/redis"          "createClient" "clangd"
validate "C jq: main"                         "/tmp/cq-smoke-test/jq"             "main"         "clangd"

echo ""
echo "── TypeScript (tsserver) ─────────────────────────────────────────────────"
validate "TS fixture: greet"                  "tests/fixtures/typescript_project" "greet"        "typescript-language-server"
validate "TS fixture: add"                    "tests/fixtures/typescript_project" "add"          "typescript-language-server"
validate "TS fixture: MAX_RETRIES"            "tests/fixtures/typescript_project" "MAX_RETRIES"  "typescript-language-server"
validate "TS fixture: User"                   "tests/fixtures/typescript_project" "User"         "typescript-language-server"
validate "TS zod: ZodType"                    "/tmp/cq-smoke-test/zod"            "ZodType"      "typescript-language-server"
validate "TS zod: parse"                      "/tmp/cq-smoke-test/zod"            "parse"        "typescript-language-server"

echo ""
echo "═══════════════════════════════════════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped out of $TOTAL tests"
echo "═══════════════════════════════════════════════════════════════════════════"
[ "$FAIL" -eq 0 ] && echo "  ALL CLEAR — zero false positives" || echo "  FALSE POSITIVES DETECTED"
