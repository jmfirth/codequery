# codequery-mcp

MCP server for [codequery](https://github.com/jmfirth/codequery) — semantic code intelligence for AI agents.

Exposes 18 codequery commands as AI-callable tools. 71 languages. Three-tier precision cascade.

## Setup

```json
{
  "mcpServers": {
    "cq": { "command": "uvx", "args": ["codequery-mcp"] }
  }
}
```

Downloads a pre-built binary on first run. No Rust toolchain needed.

Also available via: `pip install codequery-mcp`

See the [codequery-mcp README](https://github.com/jmfirth/codequery/tree/main/crates/codequery-mcp#readme) for tool details, and the [main README](https://github.com/jmfirth/codequery#readme) for full documentation.
