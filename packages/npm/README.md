# @codequery/mcp

MCP server for [codequery](https://github.com/jmfirth/codequery) — semantic code intelligence for AI agents.

Exposes 18 codequery commands as AI-callable tools. 71 languages. Three-tier precision cascade.

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

Configure precision with `CQ_SEMANTIC` env var (`daemon` recommended). Cache off by default — results are always fresh.

See the [codequery-mcp README](https://github.com/jmfirth/codequery/tree/main/crates/codequery-mcp#readme) for tool details, and the [main README](https://github.com/jmfirth/codequery#readme) for full documentation.

## Supported Platforms

- macOS (Apple Silicon, Intel)
- Linux (x64, ARM64)
- Windows (x64)
