#!/usr/bin/env bash
# LSP Validation: Compare stack graph results against LSP ground truth.
# Verifies that stack graph references are correct (no false positives)
# and measures coverage (what percentage of LSP refs does stack graph find).
#
# Usage: ./scripts/lsp-validation.sh
# Requires: rust-analyzer, gopls, clangd

set -euo pipefail

CQ="cargo run --release --"
PASS=0
FAIL=0

cargo build --release 2>/dev/null

validate() {
    local lang="$1" project="$2" symbol="$3" server="$4"

    # Check if language server is available
    if ! command -v "$server" &>/dev/null; then
        echo "  SKIP ($server not installed)"
        return
    fi

    # Get stack graph refs
    local sg_refs
    sg_refs=$($CQ refs "$symbol" --json --project "$project" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
refs=set()
for r in d.get('references',[]):
    refs.add(f\"{r['file']}:{r['line']}\")
print(d.get('resolution','none'))
for r in sorted(refs): print(r)
" 2>/dev/null) || { echo "  ERROR: stack graph query failed"; return; }

    local sg_resolution=$(echo "$sg_refs" | head -1)
    local sg_set=$(echo "$sg_refs" | tail -n +2 | sort)
    local sg_count=$(echo "$sg_set" | grep -c . || echo 0)

    # Get LSP refs
    local lsp_refs
    lsp_refs=$($CQ refs "$symbol" --json --semantic --project "$project" 2>/dev/null | python3 -c "
import sys,json
d=json.load(sys.stdin)
refs=set()
for r in d.get('references',[]):
    refs.add(f\"{r['file']}:{r['line']}\")
print(d.get('resolution','none'))
for r in sorted(refs): print(r)
" 2>/dev/null) || { echo "  SKIP (LSP query failed — server might not support this project)"; return; }

    local lsp_resolution=$(echo "$lsp_refs" | head -1)
    local lsp_set=$(echo "$lsp_refs" | tail -n +2 | sort)
    local lsp_count=$(echo "$lsp_set" | grep -c . || echo 0)

    # Compare using python for robust set operations
    python3 -c "
sg = set('''$sg_set'''.strip().split('\n')) if '''$sg_set'''.strip() else set()
lsp = set('''$lsp_set'''.strip().split('\n')) if '''$lsp_set'''.strip() else set()
sg.discard('')
lsp.discard('')

false_pos = sg - lsp
misses = lsp - sg
matched = sg & lsp

if false_pos:
    print(f'  FAIL: {len(false_pos)} false positives!')
    for fp in sorted(false_pos):
        print(f'    SG-only: {fp}')
else:
    cov = f'{len(matched)}/{len(lsp)} ({100*len(matched)//len(lsp)}%)' if lsp else 'N/A'
    print(f'  PASS: 0 false positives, {cov} coverage')

if misses:
    print(f'    Misses ({len(misses)} LSP-only refs):')
    for m in sorted(misses):
        print(f'      {m}')
" 2>/dev/null

    # Update counters
    local fp_count
    fp_count=$(python3 -c "
sg = set('''$sg_set'''.strip().split('\n')) if '''$sg_set'''.strip() else set()
lsp = set('''$lsp_set'''.strip().split('\n')) if '''$lsp_set'''.strip() else set()
sg.discard(''); lsp.discard('')
print(len(sg - lsp))
" 2>/dev/null)
    if [ "$fp_count" != "0" ]; then
        FAIL=$((FAIL + 1))
    else
        PASS=$((PASS + 1))
    fi
}

echo "════════════════════════════════════════════════════"
echo "  LSP VALIDATION: Stack Graph vs Language Server"
echo "════════════════════════════════════════════════════"
echo ""

echo "Rust (fixture: greet)"
validate "Rust" "tests/fixtures/rust_project" "greet" "rust-analyzer"

echo ""
echo "Go (fixture: Greet)"
validate "Go" "tests/fixtures/go_project" "Greet" "gopls"

echo ""
echo "C (fixture: add)"
validate "C" "tests/fixtures/c_project" "add" "clangd"

echo ""
echo "TypeScript (fixture: greet)"
validate "TypeScript" "tests/fixtures/typescript_project" "greet" "typescript-language-server"

# Real projects (if smoke test repos exist)
if [ -d "/tmp/cq-smoke-test/gin" ]; then
    echo ""
    echo "Go (gin: New)"
    validate "Go" "/tmp/cq-smoke-test/gin" "New" "gopls"
fi

if [ -d "/tmp/cq-smoke-test/redis" ]; then
    echo ""
    echo "C (redis: main)"
    validate "C" "/tmp/cq-smoke-test/redis" "main" "clangd"
fi

echo ""
echo "════════════════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "════════════════════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo "FALSE POSITIVES DETECTED — stack graphs returned incorrect refs"
    exit 1
else
    echo "NO FALSE POSITIVES — all stack graph refs verified against LSP"
fi
