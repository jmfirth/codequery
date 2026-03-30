# cq-mcp

MCP (Model Context Protocol) server for [cq](https://github.com/jmfirth/codequery) — semantic code query tool. Exposes all 12 cq commands as AI-callable tools over JSON-RPC stdio.

75 languages. Three-tier precision cascade: tree-sitter → stack graphs → LSP. Auto-starts a language server daemon for compiler-level precision.

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

Every tool accepts a `project` argument to specify the project root (defaults to cwd).

## How it works

The MCP server shells out to the `cq` CLI with `--json --semantic --no-cache` on every tool call. It auto-starts `cq daemon` on initialization to keep language servers warm, giving sub-second semantic precision. The daemon is stopped on clean shutdown.

`--semantic` is always on because the daemon tracks file changes — results stay fresh even as the agent edits code. `--no-cache` is always on because files change between queries in an agent workflow.

## Learn more

See the [main README](https://github.com/jmfirth/codequery#readme) for full documentation on cq's command surface, language support, precision cascade, and agent integration.
