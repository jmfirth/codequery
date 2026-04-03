# @codequery/mcp

MCP server for [cq](https://github.com/jmfirth/codequery) — semantic code query tool.

Exposes 18 cq commands as AI-callable tools. 71 languages. Three-tier precision cascade.

## Setup

```json
{
  "mcpServers": {
    "cq": { "command": "npx", "args": ["-y", "@codequery/mcp"] }
  }
}
```

Downloads a pre-built binary for your platform from GitHub releases. No Rust toolchain needed.

Also available via: `npm install -g @codequery/mcp`

## Available Tools

`cq_def`, `cq_body`, `cq_sig`, `cq_refs`, `cq_callers`, `cq_deps`, `cq_outline`, `cq_symbols`, `cq_imports`, `cq_search`, `cq_context`, `cq_tree`, `cq_hover`, `cq_diagnostics`, `cq_rename`, `cq_dead`, `cq_callchain`, `cq_hierarchy`

## How it works

Auto-starts a cq language server daemon for compiler-level precision. Uses `--semantic --no-cache` on every call — results are always fresh, always the best precision available.

See the [cq-mcp README](https://github.com/jmfirth/codequery/tree/main/crates/codequery-mcp#readme) for tool details, and the [main README](https://github.com/jmfirth/codequery#readme) for full cq documentation.

## Supported Platforms

- macOS (Apple Silicon, Intel)
- Linux (x64, ARM64)
- Windows (x64)
