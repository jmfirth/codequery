# cq-mcp

MCP (Model Context Protocol) server for [cq](https://github.com/jmfirth/codequery) — semantic code query tool. Exposes cq commands as AI-callable tools over JSON-RPC stdio.

71 languages. Three-tier precision cascade: tree-sitter → stack graphs → LSP.

## Setup

### Claude Desktop / Cursor / MCP-compatible tools

**npm** (recommended — no Rust toolchain needed):
```json
{
  "mcpServers": {
    "cq": { "command": "npx", "args": ["-y", "@codequery/mcp"] }
  }
}
```

**pip**:
```json
{
  "mcpServers": {
    "cq": { "command": "uvx", "args": ["codequery-mcp"] }
  }
}
```

**Direct binary** (if cq-mcp is on PATH):
```json
{
  "mcpServers": {
    "cq": { "command": "cq-mcp" }
  }
}
```

### Installing the binary directly

```
cargo install codequery-mcp
```

Or download from [GitHub releases](https://github.com/jmfirth/codequery/releases).

## Available Tools

| Tool | Description |
|------|-------------|
| `cq_def` | Find where a symbol is defined |
| `cq_body` | Extract the full source body of a symbol |
| `cq_sig` | Get the type signature |
| `cq_refs` | Find all references across the project |
| `cq_callers` | Find call sites for a function |
| `cq_deps` | Analyze dependencies of a function |
| `cq_outline` | List all symbols in a file |
| `cq_symbols` | List all symbols in the project |
| `cq_imports` | List imports for a file |
| `cq_search` | Structural AST pattern search |
| `cq_context` | Get enclosing symbol for a line |
| `cq_tree` | Show project structure tree |
| `cq_hover` | Type info and docs at a source location |
| `cq_diagnostics` | Run diagnostics on a file |
| `cq_rename` | Preview or apply a symbol rename |
| `cq_dead` | Find unreferenced symbols |
| `cq_callchain` | Trace call chains to/from a function |
| `cq_hierarchy` | Show type hierarchy for a symbol |

### Optional: `cq_edit`

| Tool | Description |
|------|-------------|
| `cq_edit` | Edit a file by replacing an exact string match (no Read required) |

`cq_edit` is gated behind the `CQ_MCP_EDIT=1` environment variable. It enables agents to edit files directly after using `cq_body` — two calls instead of three.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `CQ_SEMANTIC` | off | Precision tier: `off`, `1`/`on` (oneshot LSP), `daemon` (persistent LSP) |
| `CQ_MCP_EDIT` | off | Set to `1` to enable the `cq_edit` tool |
| `CQ_CACHE` | off | Set to `1` to enable scan caching (off by default for agents — files change between queries) |

### Recommended agent configuration

```json
{
  "mcpServers": {
    "cq": {
      "command": "cq-mcp",
      "env": {
        "CQ_SEMANTIC": "daemon",
        "CQ_MCP_EDIT": "1"
      }
    }
  }
}
```

## How it works

The MCP server shells out to the `cq` CLI for query tools. `cq_edit` operates directly on files without a subprocess.

`CQ_SEMANTIC` controls precision. When set to `daemon`, the server auto-starts `cq daemon` on initialization to keep language servers warm for sub-second semantic precision. Cache is off by default because files change between queries in agent workflows.

## Learn more

See the [main README](https://github.com/jmfirth/codequery#readme) for full documentation on cq's command surface, language support, precision cascade, and agent integration.
