# cq

Semantic code intelligence for AI agents. One binary, 71 languages, zero setup.

`cq` gives agents structured answers about code — definitions, references, call hierarchies, type info — using 10-50x fewer tokens than reading files. It works without a language server, without a compilable project, without any configuration. Three precision tiers activate automatically based on what's available.

```
$ cq body detect_project_root --in crates/codequery-core
@@ meta resolution=syntactic completeness=exhaustive total=1 @@

@@ crates/codequery-core/src/project.rs:33:0 function detect_project_root @@
pub fn detect_project_root(start: &Path) -> Result<PathBuf> {
    let canonical = start.canonicalize()
        .map_err(|e| CoreError::Path(format!("cannot canonicalize {}: {e}", start.display())))?;
    let mut current = canonical.as_path();
    loop {
        for marker in MARKERS {
            if current.join(marker).exists() {
                return Ok(current.to_path_buf());
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return Err(CoreError::ProjectNotFound(start.to_path_buf())),
        }
    }
}
```

That's real output. Source code passes through unescaped — no JSON string escaping, no wrapper objects. The `@@ meta @@` header tells you the precision tier and result count. Parseable by grep and LLMs equally.

---

## Why cq

**For agent harness builders:** Give your agent semantic code understanding without LSP overhead. `cq body handle_request` returns 5 lines instead of reading a 500-line file. Every result carries `resolution` and `completeness` metadata so agents self-calibrate trust.

**For developers using AI tools:** Your agent gets better results with cq than with grep. MCP integration means Claude, Cursor, and any MCP-compatible agent can use all 24 commands as native tools — zero prompt engineering.

**For humans:** The CLI is fast and genuinely useful. `cq def Symbol` finds it instantly across 71 languages. IDE plugins are a natural next step, but the CLI is productive today.

---

## Precision Cascade

cq doesn't force you to choose between speed and accuracy. Three tiers activate automatically:

```
Tree-sitter    instant, all 71 languages, works on broken code
     |
Stack graphs   import-aware, follows qualified names, 10 languages
     |
LSP daemon     compiler-level, full type resolution, 40+ languages
```

The same command produces the same output format at every tier. Only the `resolution` metadata changes — `syntactic`, `resolved`, or `semantic`. No configuration, no flags, no setup.

```
$ cq refs greet --project tests/fixtures/rust_project
@@ meta resolution=resolved completeness=best_effort total=5 @@

@@ src/lib.rs:9:0 function greet (definition) @@
@@ src/lib.rs:15:14 call @@
    let msg = greet("world");
@@ tests/integration.rs:1:21 import @@
    use fixture_project::greet;
@@ tests/integration.rs:5:15 call @@
    assert_eq!(greet("world"), "Hello, world!");

5 references (resolved)
```

Stack graphs traced the import and resolved call sites across files — no language server involved.

---

## Install

**npm** (no Rust toolchain needed):
```
npx -y @codequery/cli
```

**pip**:
```
uvx codequery-cli
```

**Cargo**:
```
cargo install codequery-cli
```

**Binary**: Pre-built for Linux, macOS, and Windows at [github.com/jmfirth/codequery/releases](https://github.com/jmfirth/codequery/releases).

---

## Agent Integration

### MCP Server (recommended)

All 18 query commands become native tool calls. Works with Claude Code, Cursor, and any MCP-compatible agent.

```json
{
  "mcpServers": {
    "cq": { "command": "npx", "args": ["-y", "@codequery/mcp"] }
  }
}
```

Also available via pip (`uvx codequery-mcp`) or direct binary (`cq-mcp`).

### CLI + llms.txt

Install `cq` on the agent's PATH and include [`llms.txt`](llms.txt) in the system prompt. The agent can immediately use all commands via shell.

---

## 71 Languages

Languages install automatically on first use. `cq outline app.ex` downloads Elixir support in ~2 seconds, then shows results. No manual setup.

**Built-in** (compiled into the binary): Rust, TypeScript, JavaScript, Python, Go, C, C++, Java, Ruby, PHP, HTML, CSS, JSON, YAML, TOML

**Installable** (56 more): Elixir, Haskell, Dart, Scala, Swift, Kotlin, SQL, F#, OCaml, Clojure, Erlang, Julia, Lua, Zig, Bash, Dockerfile, Terraform, Protobuf, GraphQL, SCSS, Solidity, and [many more](docs/languages.md).

**Scope-resolved cross-references** for 10 languages: Rust, TypeScript, JavaScript, Python, Go, C, C++, Java, Ruby, C#.

All 71 languages [validated end-to-end](scripts/validate-languages.sh) against real open-source projects.

---

## Commands

| Command | What it does |
|---------|-------------|
| `cq def <symbol>` | Find where a symbol is defined |
| `cq body <symbol>` | Extract the full source body |
| `cq sig <symbol>` | Type signature without the implementation |
| `cq refs <symbol>` | All references across the project |
| `cq callers <symbol>` | Call sites for a function |
| `cq deps <symbol>` | Internal dependencies of a function |
| `cq outline [file]` | All symbols in a file with nesting |
| `cq symbols` | All symbols in the project |
| `cq imports <file>` | Imports and dependencies |
| `cq search <pattern>` | Structural search (tree-sitter S-expressions) |
| `cq context <file:line>` | Enclosing symbol for a line |
| `cq tree [path]` | Symbol tree for a directory |
| `cq hover <file:line>` | Type info and docs at a location |
| `cq diagnostics [file]` | Syntax errors |
| `cq rename <old> <new>` | Rename across the project |
| `cq dead` | Find unreferenced symbols |
| `cq callchain <symbol>` | Multi-level call hierarchy |
| `cq hierarchy <type>` | Type hierarchy (supertypes/subtypes) |
| `cq grammar` | Manage language support |
| `cq daemon` | Manage LSP daemon for semantic precision |

See the [full usage guide](docs/guide.md) and [real output examples](docs/examples.md).

---

## Output Format

**Framed** (default) — `@@ file:line:column kind name @@` headers with raw source between them. Designed for humans and agents alike: no JSON escaping, source code passes through verbatim.

**JSON** (`--json`) — structured output with metadata. Compose with `jq`.

**Raw** (`--raw`) — content only. Pipe into other tools: `cq body handle_request --raw | wc -l`.

Every response starts with `@@ meta resolution=... completeness=... total=N @@` so consumers know the precision tier and result count before reading the body.

---

## Development

```
just check          # fmt + clippy
just test           # 2000+ tests
just build          # debug build
just release        # release build
```

See [architecture docs](docs/architecture.md) for crate structure and design.

---

## License

Apache 2.0
