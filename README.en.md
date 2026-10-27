# byte-wiki

> A local-first, repo-level knowledge engine — turn any Git repository into structured knowledge you can browse, query and share. Your code never leaves your machine.

English | [中文](README.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?logo=rust)
![Tauri](https://img.shields.io/badge/desktop-Tauri%202-24c8d8)

## Contents

- [What it does](#what-it-does)
- [Quick start](#quick-start)
- [`.bytewiki/` layout](#bytewiki-layout)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Development](#development)
- [License](#license)

## What it does

### Knowledge generation

- **One-click Wiki** — index → plan → per-module docs (topological order) → top-level architecture → assembled into `.bytewiki/` inside the repo, fully local
- **Incremental updates** — change detection regenerates only affected modules and their dependents; untouched docs stay byte-identical, with staleness badges
- **Knowledge cards** — hand-written Markdown in `cards/`, never overwritten by generation, retrievable in chat

### Function-level call graph

- **Multi-language extraction** — tree-sitter parses Python / JavaScript / TypeScript / Go / Java / Rust into a function-level call graph
- **SQLite as primary store** — `callgraph.db` gives millisecond queries (overview / subgraph / search / walkthrough chain), with `callgraph.json` double-written for interop
- **Incremental generation** — content fingerprints decide what *really* changed; unchanged files skip re-parsing, and calls that previously failed to resolve revive automatically when new symbols appear
- **Walkthrough chains** — DFS expansion of the full execution flow from any function; broken links produce a boundary report (which line broke, what the candidate targets are)
- **Synthetic event edges** — `emit('x')` ↔ `on('x')` pairing closes cross-function event flows, marked as `event` edges (visible as heuristic hops, never masquerading as precise)
- **LSP refinement** — rust-analyzer `textDocument/definition` upgrades heuristic edges to precise ones and rewrites wrongly-linked targets
- **Per-project rules** — file exclusions / entry exclusions / custom entry regex, configured per repository

### Visualization

- **Call-graph canvas** — three switchable layouts (directed layered / architectural layered / concentric rings), visual depth filter, camera fly-to, blast-radius pulse
- **Project graph page** — a full-bleed immersive showcase: staged entrance animation, hover spotlight (neighbors bright, rest dimmed), click cards

### Branch awareness

- Wiki / call graph / chat artifacts are **isolated per git branch**, with a global switcher
- **Staleness detection** (generated-at HEAD vs current) and **same-commit reuse** (sibling branches copy artifacts instead of recomputing)
- Viewing another branch reads **git snapshots**, so line numbers match that branch's docs

### RAG Q&A

- Vector retrieval over Wiki docs + code chunks, streaming answers
- Answers **cite their sources**: click to open the exact source snippet in a drawer; multi-session chat management (rename / delete)

### Export & sharing

- **Static HTML export** — VitePress-style site with working Mermaid diagrams and syntax highlighting; print from the browser for PDF
- `.bytewiki/` is plain Markdown — commit it to share with your team; clones load instantly with **zero LLM cost**

### Multiple frontends

- **Tauri 2 desktop app** (Vue 3): the full experience
- **`bw` CLI**: generate / query / backup / export, for scripts and pipelines
- **MCP service**: expose repository knowledge to AI agents
- **VS Code extension**: browse Wiki and call graphs inside the editor

## Quick start

### Desktop (recommended)

```bash
# Prerequisites: Rust (MSVC), Node.js 18+, Ollama (optional, local models)
cd apps/desktop
npm install
npm run tauri dev    # dev
npm run tauri build  # package
```

1. Run [Ollama](https://ollama.com) locally and pull models (e.g. `ollama pull qwen3.5:9b` and `ollama pull nomic-embed-text`)
2. Open the app → Settings → Model: keep provider as Ollama, set model names, hit "Test connection"
3. Open a local Git repo on the home page → "Generate Wiki" → browse / walkthrough / ask

### CLI

```bash
cargo run -p byte-wiki-cli -- --help            # binary name: bw

bw callchain <repo>              # generate the call graph (incremental)
bw callchain <repo> --refine     # LSP refinement (requires local rust-analyzer)
bw callchain-query <repo> --overview            # overview / entry list
bw callchain-query <repo> --chain <node-id>     # walkthrough chain (with break report)
bw export <repo> --out <dir>     # export Wiki as a static HTML site
```

Also `backup` / `restore` (data directory backup), `repo`, and `server` (MCP / REST) subcommands.

## `.bytewiki/` layout

```
.bytewiki/
  index.json            # version, model, timestamp, file manifest
  outline.json          # document tree outline
  pages/…               # module docs (machine-generated, overwritten on regen)
  cards/                # human knowledge cards (protected, never overwritten)
  graphs/callchain/     # callgraph.json + callgraph.db
  branches/<name>/      # artifacts of non-current branches (per-branch isolation)
```

## Configuration

| Setting | Where | Notes |
|---|---|---|
| Model | Settings → Model | Local Ollama by default; any OpenAI-compatible endpoint, with connection test |
| Call-graph rules | Call graph page ⚙ | Persisted per repo: file/entry exclusion globs, custom entry regex |
| Excluded dirs | Settings | Shared by indexing and call-graph generation |

## Architecture

```
crates/core        # business core: indexing, Wiki pipeline, LLM client, RAG,
                   # branch system, call-graph storage & queries, LSP refinement, HTML export
crates/callchain   # tree-sitter multi-language call-graph extraction engine (pure parsing)
apps/cli           # bw CLI
apps/desktop       # Tauri 2 + Vue 3 desktop app
apps/vscode        # VS Code extension
```

CLI, desktop app and VS Code extension share the same `crates/core`, so behavior is consistent across frontends.

## Development

```bash
cargo test -p byte-wiki-core    # core unit tests (mock LLM)
cargo test -p byte-wiki-callchain
cd apps/desktop && npm run typecheck && npm run build
```

## License

[MIT](LICENSE) © 2026 cicbyte
