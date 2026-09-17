# Redrob Recall Architecture

## Product boundary

Redrob Recall is an installed local application. It does not modify Redrob Console and does not require a separately managed Qdrant server, Docker, or a cloud vector database.

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

The canonical data directory is `~/.redrob/recall`. The app can rebuild all derived data from the user's original files.

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

Development builds do not configure an update endpoint. Production builds embed the updater public verification key and the fixed stable endpoint `https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json`. A numeric release tag builds signed updater artifacts into a draft GitHub Release.

GitHub Releases is the channel itself; there is no CDN and no promotion job. The draft gate is enforced by how the endpoint resolves rather than by a separate pipeline: `releases/latest/download/...` serves only the newest published, non-prerelease release, so the exact signed files stay invisible to installed clients while QA runs on them. Publishing the reviewed draft is what makes the version live, and the assets a client downloads are byte-identical to the ones QA approved, because GitHub does not permit a published asset to be replaced with different bytes under the same name.

The tag workflow's `verify-release-assets` job asserts the contract before anyone can publish: `latest.json` must be attached, its version must match the tag, and every platform the run actually built must have a signed entry pointing at this repository's own release assets. That platform list is not written twice — `scripts/release-matrix.mjs` produces both the build matrix and the keys this job requires, so a platform that is deliberately not built is not demanded, and one that is built cannot be quietly missing. Today the list is `linux-x86_64` and `darwin-aarch64`: `windows-x86_64` returns when a code-signing certificate is configured, and `darwin-x86_64` is absent because `ort-sys` publishes no ONNX Runtime binary for Intel macOS. A missing or misdirected manifest does not break installation — it silently freezes every existing client on its current version, which is why it is a build failure rather than a review item.

The distribution channel is not a signing authority. It never receives the updater private key. The frontend checks for updates only on explicit user action, and Tauri accepts an artifact only when its signature verifies against the public key embedded in the installed application.

## Trust boundaries

- Tauri IPC is available only to the bundled frontend under the configured CSP.
- Source open/reveal commands canonicalize paths and require membership in a selected library folder.
- External Console links use the Tauri opener plugin; the unused shell plugin is not included.
- The local API does not bind to a LAN interface, repairs Unix token permissions, creates tokens atomically, and permits at most two concurrent Ask requests.
- Redrob errors are classified and upstream response bodies are never returned to the UI or local API.
- No source text is written to logs intentionally, though parser errors in development may include local paths.

## Release constraints

Qdrant Edge 0.8.0 currently requires the narrowly scoped Rust compiler workaround documented in `.cargo/config.toml`. Cross-platform installer builds require the Tauri native prerequisites and platform signing/notarization credentials. Apple Developer ID certificate/notarization secrets, `WIN_CSC_LINK`, `WIN_CSC_KEY_PASSWORD`, and updater signing keys must never be stored in the repository.

A bad stable release is corrected with a newer signed patch release; published release assets are not replaced and clients are not rolled back to an older version. Returning the bad release to draft removes it from `latest`. Updater key rotation still requires a transition release trusted by the existing embedded key.
