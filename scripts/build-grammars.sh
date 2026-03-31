#!/usr/bin/env bash
#
# Build WASM grammar packages for all installable languages.
#
# Reads languages/registry.json, clones each grammar repo, compiles to WASM,
# and packages into lang-<name>.tar.gz archives in dist/.
#
# Requirements: tree-sitter CLI, node, jq, git
#
# Usage:
#   ./scripts/build-grammars.sh              # build all languages
#   ./scripts/build-grammars.sh bash lua     # build specific languages
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REGISTRY="$PROJECT_ROOT/languages/registry.json"
DIST_DIR="$PROJECT_ROOT/dist"
TSG_DIR="$PROJECT_ROOT/crates/codequery-resolve/tsg"
LANGUAGES_DIR="$PROJECT_ROOT/languages"
WORK_DIR=""

# --- Dependency checks ---

check_dep() {
  if ! command -v "$1" &>/dev/null; then
    echo "error: $1 is required but not found in PATH" >&2
    echo "  install: $2" >&2
    exit 1
  fi
}

check_dep tree-sitter "npm install -g tree-sitter-cli"
check_dep node "https://nodejs.org"
check_dep jq "https://jqlang.github.io/jq/"
check_dep git "https://git-scm.com"

# --- Setup ---

cleanup() {
  if [[ -n "$WORK_DIR" && -d "$WORK_DIR" ]]; then
    rm -rf "$WORK_DIR"
  fi
}
trap cleanup EXIT

WORK_DIR="$(mktemp -d)"
mkdir -p "$DIST_DIR"

# --- Parse registry ---

# Extract languages that have a grammar_repo field
LANG_DATA=$(jq -r '.languages[] | select(.grammar_repo != null) | "\(.name)\t\(.grammar_repo)"' "$REGISTRY")

# If specific languages were requested, filter to those
FILTER_LANGS=("$@")

# --- Build each grammar ---

SUCCESS=0
FAILED=0
SKIPPED=0
FAILED_NAMES=()

build_grammar() {
  local name="$1"
  local repo="$2"

  # Filter check
  if [[ ${#FILTER_LANGS[@]} -gt 0 ]]; then
    local found=false
    for f in "${FILTER_LANGS[@]}"; do
      if [[ "$f" == "$name" ]]; then
        found=true
        break
      fi
    done
    if [[ "$found" == "false" ]]; then
      return 0
    fi
  fi

  echo "==> Building $name (from $repo)"

  local clone_dir="$WORK_DIR/$name"
  local pkg_dir="$WORK_DIR/pkg-$name"

  # Clone the grammar repo (shallow)
  if ! git clone --depth 1 --quiet "https://github.com/$repo.git" "$clone_dir" 2>/dev/null; then
    echo "    FAILED: could not clone https://github.com/$repo.git" >&2
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("$name")
    return 0
  fi

  # Find the grammar directory. Some repos have the grammar in:
  # - root (most common)
  # - src/ subfolder with grammar.js at root
  # - a subfolder named after the language
  # - grammars/<name>/ (multi-grammar repos like xml, markdown, ocaml)
  local grammar_dir="$clone_dir"

  # Multi-grammar repos: check for grammars/<name>/ or <name>/
  # xml -> grammars/xml, markdown -> grammars/markdown, ocaml -> grammars/ocaml
  if [[ -f "$clone_dir/grammars/$name/grammar.js" ]]; then
    grammar_dir="$clone_dir/grammars/$name"
  elif [[ -f "$clone_dir/$name/grammar.js" ]]; then
    grammar_dir="$clone_dir/$name"
  fi

  # Special cases for repos where subfolder name doesn't match cq language name
  if [[ "$name" == "objective-c" && -f "$clone_dir/grammar.js" ]]; then
    grammar_dir="$clone_dir"
  fi

  # For terraform, the HCL repo has the grammar at the root or in dialects/
  if [[ "$name" == "terraform" ]]; then
    if [[ -f "$clone_dir/dialects/terraform/grammar.js" ]]; then
      grammar_dir="$clone_dir/dialects/terraform"
    fi
  fi

  # Verify grammar.js exists
  if [[ ! -f "$grammar_dir/grammar.js" ]]; then
    # Try grammar.json as fallback (some older grammars)
    if [[ ! -f "$grammar_dir/src/grammar.json" ]]; then
      echo "    FAILED: no grammar.js found in $grammar_dir" >&2
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return 0
    fi
  fi

  # Install npm dependencies if package.json exists (some grammars need this)
  if [[ -f "$grammar_dir/package.json" ]]; then
    (cd "$grammar_dir" && npm install --ignore-scripts --silent 2>/dev/null) || true
  fi

  # Generate the parser source if needed
  if [[ -f "$grammar_dir/grammar.js" && ! -f "$grammar_dir/src/parser.c" ]]; then
    (cd "$grammar_dir" && tree-sitter generate 2>/dev/null) || true
  fi

  # Build WASM
  local wasm_file="$grammar_dir/tree-sitter-${name}.wasm"

  # tree-sitter build --wasm outputs to the current directory
  if ! (cd "$grammar_dir" && tree-sitter build --wasm -o "$wasm_file" 2>/dev/null); then
    # Some grammars use different naming; try without specifying output
    if ! (cd "$grammar_dir" && tree-sitter build --wasm 2>/dev/null); then
      echo "    FAILED: tree-sitter build --wasm failed" >&2
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return 0
    fi
    # Find the generated .wasm file
    wasm_file=$(find "$grammar_dir" -maxdepth 1 -name "*.wasm" -type f | head -1)
    if [[ -z "$wasm_file" ]]; then
      echo "    FAILED: no .wasm file produced" >&2
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return 0
    fi
  fi

  if [[ ! -f "$wasm_file" ]]; then
    # Try finding it
    wasm_file=$(find "$grammar_dir" -maxdepth 1 -name "*.wasm" -type f | head -1)
    if [[ -z "$wasm_file" || ! -f "$wasm_file" ]]; then
      echo "    FAILED: .wasm file not found after build" >&2
      FAILED=$((FAILED + 1))
      FAILED_NAMES+=("$name")
      return 0
    fi
  fi

  # Package into lang-<name>.tar.gz
  # Contents: grammar.wasm, extract.toml, lsp.toml (optional), stack-graphs.tsg (optional)
  mkdir -p "$pkg_dir"

  cp "$wasm_file" "$pkg_dir/grammar.wasm"

  # Copy extract.toml from languages/<name>/
  if [[ -f "$LANGUAGES_DIR/$name/extract.toml" ]]; then
    cp "$LANGUAGES_DIR/$name/extract.toml" "$pkg_dir/"
  else
    echo "    WARNING: no extract.toml for $name" >&2
  fi

  # Copy lsp.toml if it exists
  if [[ -f "$LANGUAGES_DIR/$name/lsp.toml" ]]; then
    cp "$LANGUAGES_DIR/$name/lsp.toml" "$pkg_dir/"
  fi

  # Copy stack-graphs.tsg if it exists
  if [[ -f "$TSG_DIR/$name/stack-graphs.tsg" ]]; then
    cp "$TSG_DIR/$name/stack-graphs.tsg" "$pkg_dir/"
  fi

  # Create tarball (files at top level, no directory wrapper)
  local archive="$DIST_DIR/lang-${name}.tar.gz"
  tar czf "$archive" -C "$pkg_dir" .

  local size
  size=$(du -h "$archive" | cut -f1 | xargs)
  echo "    OK: lang-${name}.tar.gz ($size)"

  # Clean up clone to save disk space
  rm -rf "$clone_dir" "$pkg_dir"

  SUCCESS=$((SUCCESS + 1))
}

echo "Building WASM grammar packages..."
echo "  registry: $REGISTRY"
echo "  output:   $DIST_DIR/"
echo ""

while IFS=$'\t' read -r name repo; do
  build_grammar "$name" "$repo"
done <<< "$LANG_DATA"

echo ""
echo "=== Build Summary ==="
echo "  Success: $SUCCESS"
echo "  Failed:  $FAILED"
echo "  Skipped: $SKIPPED"
if [[ $FAILED -gt 0 ]]; then
  echo "  Failed languages: ${FAILED_NAMES[*]}"
fi
echo ""
echo "Packages written to $DIST_DIR/"
ls -lh "$DIST_DIR"/lang-*.tar.gz 2>/dev/null || echo "  (no packages)"
