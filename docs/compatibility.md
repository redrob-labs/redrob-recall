# Compatibility matrix

The same shape as redrob-canvas's `docs/compatibility.md`, and for the same reason: a roadmap that
does not name its authority per row cannot be checked, and one that does not distinguish "we have an
implementation" from "we match the authority" reads as finished when it is not.

Status: `planned`, `native`, `adapter`, `parity`.

Authorities are the pinned upstreams in `docs/upstream-sources.toml`. bloop is the one `code` source
(`431e9e82`, branch `oss`); rows naming it may be filled by porting. Rows naming this product as the
authority are our own bounded scope and are not upstream gaps.

| Domain | Feature | Authority | Initial route | Status |
|---|---|---|---|---|
| Library | user-chosen folder roots, add/remove, persisted | redrob-recall bounded scope | Rust + SQLite settings | native |
| Library | filesystem watcher, incremental re-index on change | bloop `background`/`periodic` | Rust `notify` watcher | native |
| Library | pause and resume indexing | redrob-recall bounded scope | Rust indexer claim flag | native |
| Library | backup, integrity check, clear | redrob-recall bounded scope | SQLite `integrity_check` + file copy | native |
| Library | git repository sync, branch awareness, remote clone | bloop `remotes`/`repo` | not started | planned |
| Formats | PDF page-wise text extraction | redrob-recall bounded scope | Rust extractor | native |
| Formats | DOCX text extraction | redrob-recall bounded scope | Rust extractor | native |
| Formats | HTML/XML markup extraction | redrob-recall bounded scope | Rust extractor | native |
| Formats | plain text and Markdown | redrob-recall bounded scope | Rust extractor | native |
| Formats | source code, language-aware | bloop `intelligence/language` (13 languages) | not started | planned |
| Formats | images, audio, spreadsheets, slides | none yet chosen | not started | planned |
| Retrieval | SQLite FTS5 keyword search with BM25 ranking | redrob-recall bounded scope | `chunks_fts` virtual table | native |
| Retrieval | embedded vector search | Qdrant Edge | `qdrant_edge` shard | native |
| Retrieval | hybrid fusion of keyword and vector results | bloop `semantic`/`snippet` | reciprocal rank fusion, k=60 | native |
| Retrieval | extension and path-prefix filters | redrob-recall bounded scope | post-fusion filter | native |
| Retrieval | query language: literal, regex, path and language filters, boolean | bloop `query/grammar.pest` + `query/planner` | not started | planned |
| Retrieval | stopword handling and ranking tuned per query kind | bloop `query/ranking`, `query/stopwords` | not started | planned |
| Retrieval | result snippet selection with context lines | bloop `snippet` | whole chunk returned today | planned |
| Code intelligence | symbol definitions and references, scope resolution | bloop `intelligence/scope_resolution` (53 files) | not started | planned |
| Code intelligence | go-to-definition across a repository | bloop `intelligence/namespace` | not started | planned |
| Embedding | local ONNX embedding of passages and queries | fastembed / ONNX Runtime | `fastembed` via `ort-sys` | native |
| Embedding | model choice and dimension configuration | none yet chosen | single compiled-in model | planned |
| Answering | grounded answer over retrieved chunks | Redrob Code engine | `ask_library` through the Redrob API | native |
| Answering | multi-step agent with tool calls over the library | bloop `agent`/`llm` | not started | planned |
| Local API | authenticated loopback HTTP search and ask | redrob-recall bounded scope | `axum` on 127.0.0.1 with bearer token | native |
| Local API | OpenAI-compatible surface | redrob-code `LOCAL-ENGINE-API.md` | not started | planned |
| Connection | Redrob API key connect, disconnect, status | Redrob Console | Rust HTTP client + OS keyring | native |
| Connection | shared local-daemon login instead of Console | redrob-code `LOCAL-ENGINE-API.md` | not started | planned |
| Interface | search, results, sources, settings, onboarding | Redrob design system | React 19 + design tokens | native |
| Interface | Korean localisation | Redrob design system | not started | planned |

`native` means this product has an owned implementation, not parity with an upstream. A row moves to
`parity` only after a golden-output check against the pinned authority passes — stage 2 and 3 of the
porting plan build that harness, and **no row is `parity` today because no such check has ever run
here.** That is the same honesty rule redrob-canvas's matrix carries, and it is the reason its 27
rows show 19 `native`, 8 `planned`, 0 `parity`.

## What bloop is actually for, and what it is not

Measured at the pinned commit, so the plan is sized against the tree rather than a memory of it.

bloop is a **code** search engine: 53 files of scope resolution across 13 languages, a `pest`
query-language grammar with a planner and optimiser, and Tantivy-backed indexes. This product is a
**document** search engine: PDF, DOCX, markup, plain text, with no notion of a symbol.

So the overlap is retrieval, and the complement is everything above it. The rows worth porting are
the query language, snippet selection, ranking, and — if this product ever indexes source — the
language intelligence. The rows not worth porting are the ones where bloop's answer and ours differ
only in which library provides it.

### The index-storage question is narrower than it looked

The porting plan records an open decision: Tantivy or SQLite FTS5, and do not end up with two
indexes. Reading the code settles most of it.

This product already runs **hybrid retrieval**: `storage.rs` maintains a `chunks_fts` FTS5 virtual
table ranked by `bm25()`, `search.rs` queries Qdrant Edge for vectors, and the two are combined by
reciprocal rank fusion with k=60. That is not a placeholder to be replaced — it is the same
architecture bloop arrives at, reached with a different full-text engine.

Which means taking Tantivy would replace a **working** component, and buy: a richer query grammar and
better ranking controls. Both of those are reachable without Tantivy — the grammar is a parser over
our own filters, and BM25 parameters are FTS5 arguments. Against that, Tantivy adds an index format on
disk that FTS5 already covers, a second thing for `create_backup` and `integrity_check` to keep
consistent, and a migration for existing libraries.

The decision therefore looks like: **keep FTS5, port the query language and ranking on top of it**.
That is item 1b.0 of the porting plan and is recorded there, not settled here — but it is recorded as
a leaning with its reasons, rather than as an open question with none.
