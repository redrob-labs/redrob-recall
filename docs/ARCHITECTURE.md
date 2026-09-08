# Redrob VectorDB Architecture

## Product boundary

Redrob VectorDB is an installed local application. It does not modify Redrob Console and does not require a separately managed Qdrant server, Docker, or a cloud vector database.

```text
Selected folders
      │
      ▼
Discovery → extraction → chunking → local embedding
      │                           │
      ├── SQLite metadata + FTS5 │
      └── Qdrant Edge vectors ◀──┘
                  │
                  ▼
          Hybrid local search
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
 Local Search UI     Relevant excerpts only
                            │
                            ▼
                    Redrob Chat API → cited answer
```

## Desktop process

Tauri owns application lifecycle, single-instance behavior, native dialogs, file opening, credential access, and IPC. React renders the product UI and communicates through typed commands in `src/lib/bridge.ts`. Browser demo mode provides deterministic data for interface review but is never used inside Tauri.

## Storage

SQLite stores settings, document metadata, extracted chunks, status, and an FTS5 index. Qdrant Edge runs in-process and persists named 384-dimensional cosine vectors. FastEmbed lazily downloads and caches `MultilingualE5Small` in the application model directory.

The canonical data directory is `~/.redrob/vectordb`. The app can rebuild all derived data from the user's original files.

## Indexing lifecycle

1. The user selects one or more folders.
2. `ignore::WalkBuilder` discovers supported files while respecting standard ignore rules, excluded directory names, size limits, and symlink boundaries.
3. Format-specific readers extract text and optional page metadata.
4. Text is normalized and split into overlapping passages.
5. Passages are saved to SQLite and embedded locally in batches.
6. Qdrant Edge receives the vectors and searchable payload identifiers.
7. A recursive watcher schedules reconciliation after create, modify, and remove events.
8. Existing libraries are reconciled at application startup to catch offline changes.

Per-file failures are recorded without aborting the rest of the library. Failed documents have stale FTS chunks removed.

## Retrieval

Search runs dense semantic retrieval and SQLite BM25 retrieval independently. Reciprocal-rank fusion combines the ranked lists. Filtering is applied using file extension and path prefix. UI relevance labels are ordinal rather than calibrated probability claims.

## Ask and privacy

Ask first retrieves relevant passages locally unless explicit passage IDs are supplied. Before any network request, the Rust backend verifies that excerpt transmission is enabled and a Redrob credential exists. The request contains numeric source labels and excerpt content; local filename and path metadata are retained only for opening citations after the response.

## Local API

Axum serves `/health`, `/v1/search`, and `/v1/ask` on `127.0.0.1`. Every endpoint requires `Authorization: Bearer <token>`. The token is randomly generated and written with mode `0600` on Unix. API enablement and port changes apply after restart.

## Trust boundaries

- Tauri IPC is available only to the bundled frontend under the configured CSP.
- Source open/reveal commands verify that the path exists before invoking the operating system.
- External Console links use the Tauri opener plugin.
- The local API does not bind to a LAN interface.
- No source text is written to logs intentionally, though parser errors in development may include local paths.

## Release constraints

Qdrant Edge 0.8.0 currently requires the narrowly scoped Rust compiler workaround documented in `.cargo/config.toml`. Cross-platform installer builds require the Tauri native prerequisites and platform signing/notarization credentials. These credentials must never be stored in the repository.
