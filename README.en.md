# byte-wiki

> A local-first, repo-level knowledge engine: generates a structured Wiki (architecture / modules / graphs) for local Git repositories, with RAG Q&A grounded in your codebase. Your code never leaves your machine by default.

## What it does

- **One-click Wiki generation** (FR-01/02): pick a local Git repo; byte-wiki runs index → plan → per-module docs (topological order) → top-level architecture → assemble into `.bytewiki/` inside the repo
- **Structured browsing** (FR-03/04): architecture / modules / graphs tree navigation; Mermaid dependency graph built from real code references (not LLM guesses), zoomable
- **RAG chat** (FR-05): vector retrieval over Wiki docs and code chunks, streaming answers with cited sources, multi-turn
- **Configurable models** (FR-09): local Ollama by default, switchable to any OpenAI-compatible endpoint (Settings → Model, with connection test)
- **Share via Git** (FR-08): the Wiki is plain Markdown + Mermaid; commit `.bytewiki/` to share with your team; clones load it instantly with zero LLM cost

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
3. Open a local Git repo on the home page → "Generate Wiki" → browse / ask

### CLI

```bash
cargo run -p bs -- --help
# backup / restore: data directory backup & restore
# mcp / api: MCP and REST services (skeleton)
```

## `.bytewiki/` layout

```
.bytewiki/
  index.json            # version, model, timestamp, file manifest
  architecture.md       # top-level architecture doc
  modules/<name>.md     # per-module tech docs (role/structure/API/deps)
  graphs/dependency.mmd # Mermaid module dependency graph
```

> `modules/` is machine-generated and overwritten on regeneration; put human knowledge in `cards/` (protected; full support in M2).

## Tech stack

Tauri 2 (Rust) + Vue 3 + the in-house theme system (reused from the byte-wiki desktop skeleton);
core library `crates/core`: indexing (`ignore` + multi-language import extraction), generation pipeline, LLM client (Ollama / OpenAI-compatible, streaming), RAG (SQLite vectors + cosine search), MCP/REST service skeletons.
CLI and desktop share the same core.

## Development

```bash
cargo test -p byte-wiki-core          # core unit tests (mock LLM)
cargo test -p byte-wiki-core real_e2e_smoke -- --ignored --nocapture  # real ollama E2E
cd apps/desktop && npm run build      # frontend typecheck + build
```

## Roadmap

- **M2 (delivered)**: change detection (staleness badge + change list), manual incremental updates (only affected modules and their dependents are regenerated; untouched docs stay byte-identical), `cards/` human knowledge cards (create/delete, never overwritten by generation, retrievable in chat)
- **M2 (blocked)**: SQLCipher encryption — vendored OpenSSL requires Strawberry Perl + NASM; enable once the toolchain is installed (see PRD open questions)
- **M3**: Knowledge Engine fusion (Wiki + Card + Memory), macOS/Linux

## License

See [LICENSE](LICENSE).
