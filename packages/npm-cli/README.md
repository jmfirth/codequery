# @codequery/cli

Pre-built binary distribution of [codequery](https://github.com/jmfirth/codequery) — semantic code intelligence for the command line.

71 languages. Three-tier precision: tree-sitter, stack graphs, and LSP.

## Install

```sh
npx -y @codequery/cli outline main.py    # run without installing
npm install -g @codequery/cli             # or install globally
```

Downloads a pre-built binary for your platform from GitHub releases.

## What is codequery?

codequery answers structural questions about code: where is a symbol defined, what does it look like, who calls it. One binary, 71 languages, zero setup.

```
$ cq def handle_request
@@ src/api/routes.rs:42:4 function handle_request @@

$ cq body handle_request --raw
pub async fn handle_request(req: Request) -> Response {
    let auth = authenticate(&req).await?;
    process(auth).await
}
```

See the [main README](https://github.com/jmfirth/codequery#readme) for full documentation.

## Supported Platforms

- macOS (Apple Silicon, Intel)
- Linux (x64, ARM64)
- Windows (x64)
