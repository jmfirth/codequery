# codequery-mcp

MCP server for [cq](https://github.com/jmfirth/codequery) — semantic code query tool.

Exposes 18 cq commands as AI-callable tools. 71 languages. Three-tier precision cascade.

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

See the [cq-mcp README](https://github.com/jmfirth/codequery/tree/main/crates/codequery-mcp#readme) for tool details, and the [main README](https://github.com/jmfirth/codequery#readme) for full cq documentation.
