# Redrob Recall Architecture

## Product boundary

Redrob Recall is an installed local application and, first of all, a memory layer for the other Redrob
apps on the same machine. It does not modify Redrob Console and does not require a separately managed
Qdrant server, Docker, or a cloud vector database.

```text
Selected folders
      │
      ▼
Discovery → extraction → chunking → local embedding          crates/redrob-recall-core
      │                           │
      ├── SQLite metadata + FTS5 │
      └── Qdrant Edge vectors ◀──┘
                  │
                  ▼
          Hybrid local search ──────────────┐
                  │                         │
        ┌─────────┴─────────┐               ▼
        ▼                   ▼       loopback HTTP API          crates/redrob-recall-api
   Search screen     numbered evidence    (other Redrob apps)
                            │
                            ▼
              Redrob console → cited answer                     apps/redrob-recall (Ask)
```

## Crates

| Crate | Role |
| --- | --- |
| `redrob-recall-core` | `Library` (the data directory, settings, indexing state), `indexer` (walk, extract, chunk, embed, watch), `storage` (SQLite + FTS5), `embedding` (fastembed), `search` (fusion), `ask` (evidence and prompts), `models` (the types, which are also the wire shapes). Synchronous; passes run on their own threads. No window, no network. |
| `redrob-recall-api` | `server` (axum over a `Library`, 127.0.0.1, one bearer token) and `client` (`RecallClient`, typed, with discovery through the token and address files). [API.md](API.md). |
| `redrob-recall` | The eframe/egui window and the CLI. Owns the tokio runtime the API server and the console call run on; the indexer and the searches run on plain threads. Reads the shared account. |

## Storage

SQLite stores settings, document metadata, extracted chunks, status and an FTS5 index. Qdrant Edge
runs in-process and persists named 384-dimensional cosine vectors. FastEmbed lazily downloads and caches
`multilingual-e5-small` in the model directory.

The canonical data directory is `~/.redrob/recall` (`REDROB_RECALL_DATA_DIR` overrides it). Every
derived file can be rebuilt from the user's original files.

## Indexing lifecycle

1. A folder is added (window, CLI `index`, or `POST /v1/folders`); the path is canonicalised and refused with a sentence when it does not exist, is a file, or cannot be listed.
2. `ignore::WalkBuilder` discovers supported files while respecting standard ignore rules, excluded directory names, size limits and symlink boundaries.
3. Format-specific readers extract text and optional page metadata (PDF pages, DOCX paragraphs, HTML/XML text, plain text).
4. Text is normalised and split into overlapping passages.
5. Passages are committed to SQLite with vector state `pending` and embedded locally in batches.
6. Previous vector IDs are removed and Qdrant Edge receives the replacement vectors.
7. The SQLite record becomes `ready` only after every vector batch succeeds; a file whose embedding failed (model not downloadable) keeps its keywords and stays `pending`, so it is embedded on the next pass.
8. A recursive watcher queues a pass after create, modify and remove events; a request that lands during a pass runs once more when it finishes rather than being dropped.
9. Existing libraries are reconciled at start-up. Missing or unmounted roots are preserved rather than read as deletions.

Per-file failures are recorded without aborting the rest of the library. Extraction size and passage-count ceilings protect against pathological expansion.

## Retrieval

Search runs dense semantic retrieval, SQLite BM25 retrieval (bloop's query grammar, compiled to FTS5 `MATCH`: `docs/upstream-grammar.md`) and a document-name arm independently. Reciprocal-rank fusion combines the ranked lists; scores are normalised to 0..1. Filtering by extension and path prefix is applied after fusion. The keyword snippet, centred on the match, travels with a hit when the keyword arm found it.

## Ask

`ask_context` retrieves the passages locally (or takes explicit chunk ids), numbers them, and builds the two prompts within a 60,000-character budget. The window sends those to the Redrob console (`{apiBaseUrl}/chat/completions`, model from settings, default `auto`) with the shared account's key; the API serves the same evidence at `POST /v1/ask-context` so another app can send it to its own model. Before any network request the window checks that excerpt transmission is enabled and an account exists. The request contains numeric source labels and excerpt content; filenames and paths stay local for opening the citations.

## The shared account

The launcher's contract (redrob-launcher `docs/shared-account.md`): `REDROB_API_KEY`, else this app's own `account.json` in the data directory, else the shared `account.json` the launcher writes. The two directories are watched; the account is re-read when they change and every 30 seconds. The key is never written, logged or displayed.

## Local API

Axum serves `/health`, `/v1/status`, `/v1/search`, `/v1/ask-context`, `/v1/folders` (POST, DELETE), `/v1/index`, `/v1/index/pause` on `127.0.0.1`. Every route requires `Authorization: Bearer <token>`. The token is generated once and written atomically with mode `0600`; the bound address is written beside it. API enablement and port changes apply after restart. At most four searches run at once.

## Storage and recovery

SQLite schema changes use monotonic `PRAGMA user_version` migrations. Start-up performs quick and foreign-key checks, and each migration first creates a consistent `VACUUM INTO` backup. Manual backups use the same mechanism and the newest three are retained. A failed Qdrant Edge load quarantines the derived shard and marks every indexed document for vector repair. A failed SQLite integrity check preserves the database under a timestamped quarantine name before creating a clean library. Databases created by newer versions are refused rather than downgraded. See `RECOVERY.md`.

## Releases

`.github/workflows/release.yml` builds exactly three targets from the one cargo binary, signs and notarizes macOS, signs Windows through Azure Artifact Signing, and hands the artefacts to the shared redrob-labs publish job, which writes `SHA256SUMS.txt` and `redrob-launcher.json` and publishes the release. Redrob Launcher installs and updates the app from those; Recall has no updater of its own. `RELEASING.md` has the steps.

## Trust boundaries

- Open and Reveal canonicalise the path and require it to be inside a library folder.
- The local API never binds a LAN interface, repairs Unix token permissions, creates the token atomically, and limits concurrency.
- The console's error message is shown as the console sent it; response bodies are capped at 2 MB and parsed as the one expected shape.
- No source text is written to logs intentionally.

## Release constraints

Qdrant Edge 0.8.0 requires the narrowly scoped compiler workaround in `.cargo/config.toml`. Apple Developer ID and notarization secrets are organization secrets; Windows signing uses no certificate file (Azure Artifact Signing through the job's OpenID Connect login). A bad release is corrected with a newer release; published assets are not replaced.
