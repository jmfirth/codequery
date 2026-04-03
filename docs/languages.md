# Language Support

71 languages supported. All delivered as WASM plugins that auto-install on first use. All validated against real open-source projects.

## Capabilities Key

| Capability | Description |
|------------|-------------|
| Grammar | Tree-sitter parsing |
| Extract | Symbol extraction via `extract.toml` |
| LSP | Language server configuration for semantic precision |
| Stack Graphs | Scope-resolved cross-references |

All 71 languages have Grammar and Extract. LSP and Stack Graphs vary by language.

## Language Table

| Language | Extensions | LSP | Stack Graphs | Status |
|----------|-----------|-----|-------------|--------|
| Ada | `.adb` `.ads` | yes | — | validated |
| Bash | `.sh` `.bash` | yes | — | validated |
| Bicep | `.bicep` | yes | — | validated |
| C | `.c` `.h` | yes | yes | validated |
| C# | `.cs` | yes | yes | validated |
| C++ | `.cpp` `.cc` `.cxx` `.hpp` `.hxx` `.hh` | yes | yes | validated |
| Cairo | `.cairo` | yes | — | validated |
| CMake | `.cmake` `CMakeLists.txt` | yes | — | validated |
| Clojure | `.clj` `.cljs` `.cljc` | — | — | validated |
| Common Lisp | `.lisp` `.cl` `.lsp` `.asd` | — | — | validated |
| CSS | `.css` | — | — | validated |
| CSV | `.csv` `.tsv` | — | — | validated |
| CUDA | `.cu` `.cuh` | yes | — | validated |
| Dart | `.dart` | yes | — | validated |
| Diff | `.diff` `.patch` | — | — | validated |
| Dockerfile | `.dockerfile` `Dockerfile` | — | — | validated |
| Elixir | `.ex` `.exs` | yes | — | validated |
| Elm | `.elm` | yes | — | validated |
| Erlang | `.erl` `.hrl` | — | — | validated |
| F# | `.fs` `.fsi` | yes | — | validated |
| Fortran | `.f90` `.f95` `.f03` `.f08` `.F90` | yes | — | validated |
| GLSL | `.glsl` `.vert` `.frag` `.geom` `.comp` `.tesc` `.tese` | — | — | validated |
| Go | `.go` | yes | yes | validated |
| GraphQL | `.graphql` `.gql` | yes | — | validated |
| Groovy | `.groovy` `.gradle` | yes | — | validated |
| Haskell | `.hs` | yes | — | validated |
| HTML | `.html` `.htm` | — | — | validated |
| INI | `.ini` `.cfg` `.conf` | — | — | validated |
| Java | `.java` | yes | yes | validated |
| JavaScript | `.js` `.jsx` `.mjs` `.cjs` | yes | yes | validated |
| JSON | `.json` | — | — | validated |
| Julia | `.jl` | yes | — | validated |
| Just | `.just` `justfile` `Justfile` | — | — | validated |
| Kotlin | `.kt` | yes | — | validated |
| LaTeX | `.tex` `.sty` `.cls` | yes | — | validated |
| Lua | `.lua` | yes | — | validated |
| Makefile | `.mk` `Makefile` `GNUmakefile` | — | — | validated |
| Markdown | `.md` `.markdown` | yes | — | validated |
| Nginx | `.nginx` `nginx.conf` | — | — | validated |
| Nix | `.nix` | yes | — | validated |
| Objective-C | `.m` `.h` | yes | — | validated |
| OCaml | `.ml` `.mli` | yes | — | validated |
| Org | `.org` | — | — | validated |
| Pascal | `.pas` `.pp` `.lpr` | yes | — | validated |
| Perl | `.pl` `.pm` | — | — | validated |
| PHP | `.php` | yes | — | validated |
| PKL | `.pkl` | — | — | validated |
| Prisma | `.prisma` | yes | — | validated |
| Protobuf | `.proto` | yes | — | validated |
| PureScript | `.purs` | yes | — | validated |
| Python | `.py` `.pyi` | yes | yes | validated |
| R | `.r` `.R` | — | — | validated |
| Racket | `.rkt` | yes | — | validated |
| reStructuredText | `.rst` | — | — | validated |
| Ruby | `.rb` `.rake` `.gemspec` | yes | yes | validated |
| Rust | `.rs` | yes | yes | validated |
| Scala | `.scala` | yes | — | validated |
| Scheme | `.scm` `.ss` | — | — | validated |
| SCSS | `.scss` | — | — | validated |
| Solidity | `.sol` | yes | — | validated |
| SQL | `.sql` | — | — | validated |
| Starlark | `.bzl` `.star` `BUILD` `BUILD.bazel` `WORKSPACE` | — | — | validated |
| Svelte | `.svelte` | yes | — | validated |
| Swift | `.swift` | yes | — | validated |
| Terraform/HCL | `.tf` `.hcl` | yes | — | validated |
| TOML | `.toml` | — | — | validated |
| TypeScript | `.ts` `.tsx` | yes | yes | validated |
| Verilog/SystemVerilog | `.v` `.sv` | yes | — | validated |
| XML | `.xml` `.xsl` `.xsd` `.svg` `.plist` | — | — | validated |
| YAML | `.yaml` `.yml` | — | — | validated |
| Zig | `.zig` | yes | — | validated |

## Stack Graph Languages

Scope-resolved cross-references via stack graphs are available for 8 languages:

- Rust
- TypeScript
- JavaScript
- Python
- Go
- C
- Java
- Ruby

These languages produce `resolution: resolved` results from `cq refs` and `cq callers` without a running language server. All other languages fall back to `resolution: syntactic` unless a daemon is running.

## Grammar Management

All 71 languages are delivered as WASM plugins. Grammars download automatically on first use and are cached in `~/.local/share/cq/languages/`. You can also pre-install:

```
cq grammar install python      # install a specific grammar
cq grammar install --all       # install all available grammars
cq grammar list                # show installed and available grammars
```

Commands that encounter a missing grammar will prompt for auto-install unless `--no-install` is passed.

## Quality Note

cq relies on tree-sitter grammars for parsing. Extraction quality varies by language — well-maintained grammars (Rust, Python, TypeScript, Go) produce excellent results. Less-maintained grammars may have gaps. All 71 languages have been validated end-to-end against real open-source projects using `scripts/validate-languages.sh`.
