# Redrob VectorDB

**Everything on your computer, searchable.**

Redrob VectorDB is a local-first desktop app for finding information across folders and asking grounded questions about your files. Files, extracted text, embeddings, and the complete search index stay on the device. Search is free and local. When you use **Ask**, only your question and the few relevant text excerpts are sent to the existing Redrob API; filenames, paths, complete files, and the full index are not sent.

## What it does

- Indexes selected folders without Docker or a separate database server.
- Reads PDF, DOCX, TXT, Markdown, RST, CSV, JSON, HTML, XML, and log files.
- Combines multilingual semantic retrieval with keyword matching.
- Watches selected folders and updates the local library when files change.
- Opens or reveals every result in its original location.
- Produces Redrob answers with numbered citations back to local files.
- Exposes an optional bearer-authenticated API on loopback only.

## Architecture

| Layer                       | Implementation                                             |
| --------------------------- | ---------------------------------------------------------- |
| Desktop shell               | Tauri 2                                                    |
| Interface                   | React 19 + TypeScript + Vite                               |
| Metadata and keyword search | SQLite + FTS5                                              |
| Vector search               | Qdrant Edge, embedded in-process                           |
| Local embeddings            | FastEmbed `MultilingualE5Small`, 384 dimensions            |
| File watching               | `notify`                                                   |
| Redrob answers              | `POST /chat/completions` on the configured Redrob base URL |

Application data is stored in `~/.redrob/vectordb`:

- `metadata.db` — document metadata, extracted passages, and FTS data
- `qdrant-edge/` — local semantic index
- `models/` — downloaded local embedding model
- `local-api-token` — generated local API bearer token (`0600` on Unix)

Original files are never copied into this directory. Consistent metadata backups are retained under `~/.redrob/vectordb/backups`; see [Backup and recovery](docs/RECOVERY.md).

## Reliability and recovery

Redrob VectorDB validates its metadata database at startup, applies ordered schema migrations, and creates a pre-migration backup. Documents remain pending until their semantic vectors are committed, so an interrupted index can be repaired on the next scan. Damaged derived indexes are quarantined rather than silently overwritten, and disconnected watched drives are not treated as deleted libraries.

Use **Settings → Storage** to run a health check or create a manual metadata backup. Backups include extracted passages and paths and should be protected with operating-system full-disk encryption.

## Run for development

### Prerequisites

- Node.js 22 or newer
- npm 11 or newer
- Rust 1.95.0 (automatically selected by `rust-toolchain.toml`)
- Tauri 2 system dependencies for your operating system

Linux package names vary by distribution. A typical Debian/Ubuntu setup needs WebKitGTK 4.1, GTK 3, librsvg, and build essentials:

```bash
sudo apt update
sudo apt install -y build-essential curl wget file libwebkit2gtk-4.1-dev \
  libappindicator3-dev librsvg2-dev libssl-dev patchelf
```

Install dependencies and run the desktop app:

```bash
npm install
npm run tauri dev
```

Frontend-only demo data is available for visual development:

```bash
npm run dev
# Open http://localhost:1420/?demo=1
```

The first real index or search downloads the multilingual embedding model. After that download, indexing and search work locally without network access.

## Build installers

Unsigned local development bundles can be built with:

```bash
npm ci
npm run tauri build
```

Tauri writes platform bundles under `src-tauri/target/release/bundle/`. Production installers are created only by the reviewed **Signed desktop release** workflow. It requires the external updater, Apple, and Windows signing secrets documented in [the release guide](docs/RELEASING.md); missing credentials stop the release instead of publishing unsigned artifacts.

Published builds check the signed stable update channel from **Settings → Updates**. Update metadata and installer signatures are verified by Tauri before installation. Local development builds intentionally have no update key or endpoint configured.

### Qdrant Edge compiler compatibility

The application pins Qdrant Edge 0.8.0. That release uses a standard-library macro that remains feature-gated in Rust 1.95. `.cargo/config.toml` narrowly enables the required crate attribute using `RUSTC_BOOTSTRAP` and `-Zcrate-attr=feature(assert_matches)`. Remove this workaround when upgrading to an upstream release that builds on stable Rust without it. Do not broaden the enabled feature set.

## Connect Redrob

1. Create or select a workspace API key at `https://console.redrob.ai/api-keys`.
2. Open **Ask** or **Settings → Redrob connection**.
3. Paste the key. The app stores it in the operating system credential manager.

Local indexing and Search do not require Redrob. Ask uses the workspace and balance associated with the supplied key. This app does not alter Redrob Console.

## Privacy behavior

- Folder selection, extraction, chunking, embedding, keyword search, and semantic search happen on-device.
- Search never sends a query or result to Redrob.
- Ask sends the question and up to ten relevant excerpt strings. It does not send filenames, paths, original files, or the complete index.
- Turning off **Allow Ask to send relevant excerpts** blocks Ask at the backend boundary.
- No analytics or telemetry SDK is included.

See [PRIVACY.md](PRIVACY.md) for the complete data flow and deletion instructions.

## Local API

The API binds only to `127.0.0.1` and defaults to port `47331`. Every endpoint requires the token stored at `~/.redrob/vectordb/local-api-token`:

```bash
TOKEN="$(cat ~/.redrob/vectordb/local-api-token)"

curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:47331/health

curl -X POST http://127.0.0.1:47331/v1/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"India launch decision","limit":8,"filters":{"extensions":[],"pathPrefix":null}}'

curl -X POST http://127.0.0.1:47331/v1/ask \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"What did we decide about India?","sourceIds":[],"maxSources":6}'
```

Changing the Local API switch or port takes effect after restarting Redrob VectorDB. Ask through the local API follows the same privacy setting and Redrob connection requirements as the UI.

## Useful commands

```bash
npm run check                    # TypeScript and locked Rust checks
npm run check:version            # Ensure all release versions match
npm run test:rust                # Migration, validation, and API error tests
npm run verify                   # Full build, audits, formatting, tests, and Clippy
npm run format                   # Prettier and rustfmt
npm audit                        # JavaScript dependency audit
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## Licensing and attribution

Redrob VectorDB application code is proprietary unless Redrob publishes a different license. Embedded open-source components retain their own licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Qdrant and Qdrant Edge are projects of Qdrant Solutions GmbH; Redrob VectorDB is a Redrob product and is not presented as an official Qdrant distribution.

## Project documentation

- [Architecture and trust boundaries](docs/ARCHITECTURE.md)
- [Signed release process](docs/RELEASING.md)
- [Backup and recovery](docs/RECOVERY.md)
- [Release QA checklist](docs/QA_CHECKLIST.md)
- [Privacy and deletion](PRIVACY.md)
- [Security policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
- [Third-party notices](THIRD_PARTY_NOTICES.md)

Before publishing a build, run the full local verification command:

```bash
npm run verify
```

This runs version consistency checks, the production frontend build, JavaScript dependency audit, Rust formatting, Rust tests, and Clippy with warnings treated as errors. Linux hosts must have the Tauri native development packages installed first. CI separately runs the RustSec advisory database against the locked dependency graph.
