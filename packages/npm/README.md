# cq-mcp

MCP (Model Context Protocol) server for [cq](https://github.com/jmfirth/codequery) -- a semantic code query tool.

Tree-sitter-powered structural navigation across 75 languages with a three-tier precision cascade: stack graphs, LSP, and structural search.

## Installation

```sh
npm install -g cq-mcp
```

This downloads a pre-built binary for your platform from GitHub releases.

## Usage

```sh
cq-mcp
```

The MCP server communicates over stdio. Configure it in your MCP client (e.g., Claude Desktop) as a stdio transport.

## Supported Platforms

- macOS (Apple Silicon, Intel)
- Linux (x64, ARM64)
- Windows (x64)

## Links

- [cq repository](https://github.com/jmfirth/codequery)
- [Releases](https://github.com/jmfirth/codequery/releases)
