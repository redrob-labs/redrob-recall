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
5. Passages are committed to SQLite with vector state `pending` and embedded locally in batches.
6. Previous vector IDs are removed and Qdrant Edge receives the replacement vectors.
7. The SQLite record becomes `ready` only after every vector batch succeeds; interrupted records are reprocessed on startup.
8. A recursive watcher schedules reconciliation after create, modify, and remove events.
9. Existing libraries are reconciled at application startup to catch offline changes. Missing or unmounted roots are preserved rather than interpreted as file deletions.

Per-file failures are recorded without aborting the rest of the library. Failed documents have stale FTS chunks removed. Extraction size and passage-count ceilings protect against pathological expansion.

## Retrieval

Search runs dense semantic retrieval and SQLite BM25 retrieval independently. Reciprocal-rank fusion combines the ranked lists. Filtering is applied using file extension and path prefix. UI relevance labels are ordinal rather than calibrated probability claims.

## Ask and privacy

Ask first retrieves relevant passages locally unless explicit passage IDs are supplied. Before any network request, the Rust backend verifies that excerpt transmission is enabled and a Redrob credential exists. The request contains numeric source labels and excerpt content; local filename and path metadata are retained only for opening citations after the response.

## Local API

Axum serves `/health`, `/v1/search`, and `/v1/ask` on `127.0.0.1`. Every endpoint requires `Authorization: Bearer <token>`. The token is randomly generated and written with mode `0600` on Unix. API enablement and port changes apply after restart.

## Storage and recovery

SQLite schema changes use monotonic `PRAGMA user_version` migrations. Startup performs SQLite quick and foreign-key checks, and each migration first creates a consistent `VACUUM INTO` backup. Manual backups use the same mechanism and the newest three are retained. Metadata backups contain extracted passages and paths and require the same protection as source documents.

A failed Qdrant Edge load quarantines the derived shard and marks every indexed document for vector repair. A failed SQLite integrity check preserves the database under a timestamped quarantine filename before creating a clean library. Databases created by newer application versions are refused rather than downgraded. See `docs/RECOVERY.md`.

## Signed updates

Development builds do not configure an update endpoint. Production builds embed the updater public verification key and the fixed stable endpoint `https://cdn.redrob.ai/vectordb/latest.json`. A numeric release tag builds signed updater artifacts into a draft GitHub Release, where the exact files remain private from the stable channel during QA.

Publishing an approved non-prerelease GitHub Release starts a separate protected promotion job. It requires a successful tag workflow for the exact `main`-reachable tag commit, downloads the release's snapshotted assets by asset ID, and verifies GitHub SHA-256 digests before reading the current stable manifest directly from the CDN origin. It copies non-manifest assets byte-for-byte to immutable `vectordb/v<version>/` CDN keys. It rewrites only the platform URLs in the signed Tauri metadata, downloads and hashes every referenced public URL, and conditionally uploads `vectordb/latest.json` last against the origin state it validated. Versioned assets use a one-year immutable public cache policy; stable metadata uses `no-store, max-age=0`. Existing versioned keys may be reused only when their origin bytes, SHA-256 metadata, size, content type, and cache policy match, and stable versions can move only forward.

The CDN is a distribution boundary, not a signing authority. It never receives the updater private key. The frontend checks for updates only on explicit user action, and Tauri accepts an artifact only when its signature verifies against the public key embedded in the installed application.

## Trust boundaries

- Tauri IPC is available only to the bundled frontend under the configured CSP.
- Source open/reveal commands canonicalize paths and require membership in a selected library folder.
- External Console links use the Tauri opener plugin; the unused shell plugin is not included.
- The local API does not bind to a LAN interface, repairs Unix token permissions, creates tokens atomically, and permits at most two concurrent Ask requests.
- Redrob errors are classified and upstream response bodies are never returned to the UI or local API.
- No source text is written to logs intentionally, though parser errors in development may include local paths.

## Release constraints

Qdrant Edge 0.8.0 currently requires the narrowly scoped Rust compiler workaround documented in `.cargo/config.toml`. Cross-platform installer builds require the Tauri native prerequisites and platform signing/notarization credentials. Apple Developer ID certificate/notarization secrets, `WIN_CSC_LINK`, `WIN_CSC_KEY_PASSWORD`, updater signing keys, and CDN credentials must never be stored in the repository.

A bad stable release is corrected with a newer signed patch release; immutable CDN assets are not replaced and clients are not rolled back to an older version. Updater key rotation still requires a transition release trusted by the existing embedded key.
