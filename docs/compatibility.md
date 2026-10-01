# Compatibility matrix

The same shape as redrob-canvas's `docs/compatibility.md`, and for the same reason: a roadmap that
does not name its authority per row cannot be checked, and one that does not distinguish "we have an
implementation" from "we match the authority" reads as finished when it is not.

Status: `planned`, `native`, `adapter`, `parity`.

**`parity` is not a judgement, it is a citation.** A row may claim it only by naming, in its Evidence
column, a harness that runs the authority and shows this product matching it.
`scripts/verify-upstream.mjs` enforces that; a `parity` row with no citation is an error.

The converse is NOT an error, and that distinction was learned here. A citation may sit on a `native`
row, because a harness can verify something that is not parity. This repository's only harness is
exactly that case: `scripts/verify-upstream-grammar.mjs` proves that our search grammar is bloop's
**reduced**, with three constructs that are ours and not bloop's at all. A verified reduction is a real
and checkable relationship, and it is not equivalence. Calling it parity would claim we match a grammar
we deliberately do not match.

| Key | What it compares | Runs |
|---|---|---|
| `grammar:<check>` | our grammar and stopword list against bloop's, read from the pinned source | `scripts/verify-upstream-grammar.mjs` |

Authorities are the pinned upstreams in `docs/upstream-sources.toml`. bloop is the one `code` source
(`431e9e82`, branch `oss`); rows naming it may be filled by porting. Rows naming this product as the
authority are our own bounded scope and are not upstream gaps.

| Domain | Feature | Authority | Initial route | Status | Evidence |
|---|---|---|---|---|---|
| Library | user-chosen folder roots, add/remove, persisted | redrob-recall bounded scope | Rust + SQLite settings | native | |
| Library | filesystem watcher, incremental re-index on change | bloop `background`/`periodic` | Rust `notify` watcher | native | |
| Library | pause and resume indexing | redrob-recall bounded scope | Rust indexer claim flag | native | |
| Library | backup, integrity check, clear | redrob-recall bounded scope | SQLite `integrity_check` + file copy | native | |
| Library | git repository sync, branch awareness, remote clone | bloop `remotes`/`repo` | not started | planned | |
| Formats | PDF page-wise text extraction | redrob-recall bounded scope | Rust extractor | native | |
| Formats | DOCX text extraction | redrob-recall bounded scope | Rust extractor | native | |
| Formats | HTML/XML markup extraction | redrob-recall bounded scope | Rust extractor | native | |
| Formats | plain text and Markdown | redrob-recall bounded scope | Rust extractor | native | |
| Formats | source code, language-aware | bloop `intelligence/language` (13 languages) | not started | planned | |
| Formats | images, audio, spreadsheets, slides | none yet chosen | not started | planned | |
| Retrieval | SQLite FTS5 keyword search with BM25 ranking | redrob-recall bounded scope | `chunks_fts` virtual table | native | |
| Retrieval | embedded vector search | Qdrant Edge | `qdrant_edge` shard | native | |
| Retrieval | hybrid fusion of keyword and vector results | bloop `semantic`/`snippet` | reciprocal rank fusion, k=60 | native | |
| Retrieval | extension and path-prefix filters | redrob-recall bounded scope | post-fusion filter | native | |
| Retrieval | query language: boolean, grouping, phrases, field filters, exclusions, prefix | bloop `query/grammar.pest` + `query/planner` | `src-tauri/src/query.rs` — hand-written parser to an AST, compiled to FTS5 `MATCH`. A malformed query is a reported error, never an empty result. 38 hostile inputs are executed against real FTS5 to prove no input compiles to something it rejects | native | `grammar:rule-disposition`, `grammar:no-negation-upstream`, `grammar:additions-present` — a verified REDUCTION of bloop's grammar, not equivalence, so this stays `native` |
| Retrieval | regex literal search | bloop `query/grammar.pest` | **out of scope** — FTS5 is a tokenised index and this product searches prose; reopens if source code is ever indexed | planned | |
| Retrieval | `heading` as an indexed FTS5 column | redrob-recall bounded scope | schema 3; `chunks_fts` indexes `content, heading` with bm25 weights 1.0 and 2.0. Migration drops and rebuilds the index, which is lossless only because it is an external-content table | native | |
| Retrieval | document `name`/`path` searchable | redrob-recall bounded scope | schema 4: a SECOND index, `documents_fts(name, path)` with bm25 weights 3.0 and 1.0, fused as a third RRF arm represented by each document's first chunk. **Not** denormalised into `chunks` — measured on 500 documents × 40 chunks, that cost +21.1% of the database against this index's +0.4%, and returned 20,000 rows where this returns 500. A query naming a chunk-only field makes this arm abstain rather than fail the search | native | |
| Retrieval | stopword handling | bloop `query/stopwords` | dropped from LOOSE term positions only, never from a phrase or a field-scoped term. **Not a ranking device** — measured, bm25's IDF already scores a term present in every document at -1.6e-6 against a distinctive term's -4.29, and it measures the corpus instead of guessing a language. What they fix is this parser's AND semantics excluding a real match for lacking "the" | native | `grammar:stopword-subset` — all 27 are in bloop's 570, so the list is a reduction and not our invention |
| Retrieval | result snippet selection with context lines | bloop `snippet` | FTS5's own `snippet(chunks_fts, -1, …)` at 40 tokens, selected in the query that ran the MATCH because only it knows where the match was. Column -1 quotes whichever indexed column matched, so a heading hit is quoted from the heading. A vector-only hit has none and falls back to the chunk, honestly: a semantic match has no single place to point at | native | |
| Code intelligence | symbol definitions and references, scope resolution | bloop `intelligence/scope_resolution` (53 files) | not started | planned | |
| Code intelligence | go-to-definition across a repository | bloop `intelligence/namespace` | not started | planned | |
| Embedding | local ONNX embedding of passages and queries | fastembed / ONNX Runtime | `fastembed` via `ort-sys` | native | |
| Embedding | model choice and dimension configuration | none yet chosen | single compiled-in model | planned | |
| Answering | grounded answer over retrieved chunks | Redrob Code engine | `ask_library` through the Redrob API | native | |
| Answering | multi-step agent with tool calls over the library | bloop `agent`/`llm` | not started | planned | |
| Local API | authenticated loopback HTTP search and ask | redrob-recall bounded scope | `axum` on 127.0.0.1 with bearer token | native | |
| Local API | OpenAI-compatible surface | redrob-code `LOCAL-ENGINE-API.md` | not started | planned | |
| Connection | Redrob API key connect, disconnect, status | Redrob Console | Rust HTTP client + OS keyring | native | |
| Connection | shared local-daemon login instead of Console | redrob-code `LOCAL-ENGINE-API.md` | not started | planned | |
| Interface | search, results, sources, settings, onboarding | Redrob design system | React 19 + design tokens | native | |
| Interface | Korean localisation | Redrob design system | not started | planned | |

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

**DECIDED: keep FTS5, port the query language and ranking on top of it.** Item 1b.0, settled by
running every feature of bloop's grammar against FTS5 rather than by reasoning about it. Exactly one
feature is out of reach — regex — and that is a code-search need rather than a document-search one.
The full evidence, including a gap the reading found that was not in the plan, is in
`docs/index-engine-decision.md`.
