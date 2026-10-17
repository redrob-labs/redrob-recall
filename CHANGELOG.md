# Changelog

All notable changes to Redrob Recall are documented here.

## 0.6.0 - 2026-10-17

### Added

- **The window serves its tools to Redrob Code.** While it is open, Recall registers with the shared Redrob Code daemon on loopback as MCP server `app:recall` (through `redrob-app-bridge`, pinned to redrob-code v0.7.0), so the agent in any Redrob surface can drive it: `status`, `list_sources`, `add_source {path}`, `remove_source {path}`, `reindex {source?}` (answers a job id), `index_status {job_id?}`, `search {query, limit?}` (ranked hits, shown in the Search view too) and `open_hit {id, open?}`. Requests that touch window state run once per frame through the same code paths the UI uses; the search itself runs off the UI thread. A daemon that cannot be reached is logged and the window runs as before. README: a "Control from Redrob Code" section lists the tools.

## 0.5.0 - 2026-10-16

### Changed

- **The window is redesigned in the Redrob Canvas shape.** A top bar with the app name, the view tabs (Library, Search, Ask, Settings; Ctrl+1..4), a Library menu (Add folder…, Check now, Pause / Resume) and a Help menu (Language, Theme, Shortcuts, About), and a right slot with the account chip ("Signed in with Redrob Launcher" / Connect), the indexing chip and the messages toggle. The left panel is the library tree: every folder with its document count and its index state (ready, indexing, failures, missing); picking one scopes Search to it. The right dock (Ctrl+Shift+M) lists the status history and the files the indexer could not read, with their errors, in place of modal dialogs. The bottom status line carries the live progress (files done / total, the current file) with Pause / Resume.
- **One theme, set once.** The Redrob Canvas colour and spacing tokens, light or dark, following the operating system with a Help > Theme override; Inter and JetBrains Mono are bundled (SIL OFL 1.1) and the system's CJK font is merged once at start-up as the fallback. Panels draw with the panel fill and a 1 px border; cards replace the stock group frames.
- **Search as you type.** The box is focused at launch and on Ctrl+K; a search starts 250 ms after the last keystroke. A result row is the title, the page and heading, the path ellipsized to one line (the whole path on hover), the keyword snippet with the matches in the accent colour and the score. ↑ / ↓ move between results, Enter opens the selected file, Ctrl+Enter reveals it in the file manager, Esc clears; double-click opens.
- **Ask citations jump.** The answer's `[n]` citations are buttons: one click scrolls to source n, highlights it and opens the file. The sources list and the not-signed-in notice are cards beneath.
- **Settings is one page** -- account, answer model, index (embedding model, file types, skipped folders, largest file, passage size and overlap), library location, local API, appearance (language, theme) -- with Apply and Reset; maintenance (check health, back up, clear the index) beneath.
- **First run** shows one primary action, Add folder, and accepts a folder dropped onto the window (a drop target is drawn while a folder hovers).
- **Korean.** Every string in the window is drawn through the interface language: English or Korean, following Redrob Launcher's shared `language.json`, else the operating system, with a Help > Language override kept in `ui.json` beside the library. Help > Shortcuts lists the keys.

### Added

- The release uploads `redrob-recall.png` (the 512 px icon) and names it in `redrob-launcher.json` (`"icon": "redrob-recall.png"`, schema 1), so Redrob Launcher's catalog shows this release's own icon. `tools/release_manifest.py` writes both in the Linux leg and checks them before the release is published.
- `Library::failed_documents` in `redrob-recall-core`: the documents the last pass could not read, newest first, with their errors. The loopback API is unchanged.

## 0.4.0 - 2026-10-16

### Changed

- **Rewritten as a Rust egui/wgpu application on a reusable memory layer.** The Tauri + React app is replaced outright on `develop`: a Cargo workspace in the redrob-canvas shape with `crates/redrob-recall-core` (extraction, chunking, SQLite + FTS5 keyword index, local multilingual embeddings in Qdrant Edge, hybrid search, folder watcher, library store: the Tauri app's engine modules promoted unchanged, with their tests), `crates/redrob-recall-api` (the loopback HTTP API and a typed Rust client) and `apps/redrob-recall` (an eframe window with Library, Search, Ask and Settings screens, plus `serve`, `index <dir>` and `search <query>` subcommands). Recall is first of all the local memory the other Redrob apps call; the window is a thin client on it.
- **Ask calls the Redrob console directly** (`POST {apiBaseUrl}/chat/completions`, model `auto`) with the account Redrob Launcher signed this computer in with, read from the launcher's shared `account.json` (`REDROB_API_KEY` or Recall's own `account.json` win over it) and watched for changes while the window is open. The Redrob Code daemon is no longer involved and is not required. The console's own error message is shown when a request fails, with the locally found sources beneath. There is no sign-in UI of Recall's own beyond "Open Redrob Launcher".
- **The local API** gains `GET /v1/status`, `POST /v1/ask-context` (the numbered evidence and the prompts, for a caller that talks to its own model), `POST`/`DELETE /v1/folders` and `POST /v1/index`, `/v1/index/pause`; `POST /v1/ask` is gone (Recall no longer holds a key to answer with). The bound address is written to `<data dir>/local-api-address` so a client finds the server without configuration. `docs/API.md` is the contract.
- **Releases** are built by the redrob-canvas release workflow shape: exactly three targets (Linux x86_64 `.tar.gz` + `.deb`, macOS arm64 `.dmg` + `.zip`, Windows x64 `-portable.zip` + `-setup.exe`), `SHA256SUMS.txt` and `redrob-launcher.json` through the shared redrob-labs publish job. The Windows setup is the branded native installer with an English / Korean language page; the macOS disk image carries an uninstaller. The Tauri updater, its signing keys and `latest.json` are gone: Redrob Launcher updates the app. Brand assets are built from the design system by `scripts/build-brand-assets.py` and pinned in `brand/SOURCES.json`.
- CI is one pull-request workflow: a seconds-long scripts job (brand pins, version) and one Rust job (fmt, clippy, the tests of our crates, `--profile ci`), skipped when no Rust input changed or when the pull request targets `main`.
- Data stays in `~/.redrob/recall` and an existing index is read as is (`REDROB_RECALL_DATA_DIR` overrides the folder).

### Removed

- The web frontend (`src/`, Vite, TypeScript, npm), the end-to-end WebDriver tests, the Node release scripts, the Redrob Code engine client, the in-app key paste, the credential-manager key.

## [0.3.2] - 2026-10-14

### Fixed

- The Windows release leg probed for the built binary without `.exe`; Git Bash reports that path as present when only the `.exe` exists, so the end-to-end-hook check then failed on a file that was not there (v0.3.1). The probe now prefers the `.exe`. No application change.

## [0.3.1] - 2026-10-14

### Fixed

- The Linux release leg repacks the AppImage with appimagetool, which refuses to run without `desktop-file-validate`; the runner did not have it, so the v0.3.0 Linux build failed. The workflow now installs `desktop-file-utils`. No application change.

## [0.3.0] - 2026-10-14

### Changed

- Ask reaches the Redrob console only through the shared Redrob Code daemon (`<daemon>/v1/chat/completions`, with the daemon's authorization, `x-redrob-app: recall` and the query id as `x-redrob-session`). The daemon holds the console key: this app no longer stores one in the credential manager, reads `REDROB_API_KEY`, or reads Redrob Launcher's `account.json` -- the engine reads the launcher's sign-in itself. "Connect Redrob" hands the pasted key to the engine (`PUT /auth/redrob`); the connection panel reads signed-in status from the engine and says "Signed in with Redrob Launcher" when that is whose key it uses. A key an earlier version stored is handed to the engine once and deleted. The `redrobBaseUrl` setting is ignored. **Requires Redrob Code**: without it the panel says "Redrob Code is not installed. Install it with Redrob Launcher." with the launcher link, offers no Connect, and Ask reports the same; local indexing and search are unaffected.

### Added

- Release assets are named `redrob-recall-<version>-<os>-<arch>.<ext>` (for example `redrob-recall-0.2.1-linux-x86_64.AppImage`), the shape Redrob Launcher installs, and the release workflow fails unless every asset parses that way. The updater manifest is repointed at the new names; installed clients are unaffected.
- Every release carries `redrob-launcher.json` (`{ "schema": 1, "sharedAccount": true }`), so Redrob Launcher learns from the release itself that this version uses its shared sign-in. The release workflow fails unless the file is attached and valid.

### Fixed

- The AppImage re-sign step matched manifest entries by file name, but on a draft release the manifest carries asset ids, so it would have failed every release since it was added. It now matches the id it is about to replace and writes the published URL.

## [0.2.0] - 2026-10-01

### Added

- Query language ported from bloop: quoted phrases, `-` exclusions, prefix `*`, and a `heading:` field, parsed rather than ORed.
- Heading index, so a term that appears only in a chunk's heading is findable.
- Document-name index, a second index rather than duplicated rows, so a document is findable by its own filename.
- Compatibility matrix with a named authority per row, and a citation gate: a `parity` claim must name a harness that exists.
- Grammar-reduction harness, which fetches bloop at its pin and checks our grammar against it mechanically.
- Redrob design system applied to every surface.
- Branch model enforced in CI.
- Signed multi-platform GitHub release workflow with Tauri updater artifacts and draft-release QA gates.
- Explicit update check and signed install flow in Settings.
- Versioned SQLite migrations, integrity checks, pre-migration/manual backups, and recovery documentation.
- Automated Rust tests, locked verification, RustSec auditing, and a cross-platform release checklist.

### Changed

- Search results quote the matching passage instead of the start of the chunk.
- bloop is recorded as reimplemented from upstream, not copied: no bloop source ships here. The grammar keeps 7 of its constructs, drops 11, and adds 3 bloop never had -- exclusions, prefix matching and the heading field -- so the relationship is a measured reduction plus our own additions, which is why no row claims parity.
- The redistributed artefact is named as what it is: qdrant-edge 0.8.0.
- Tantivy was decided against; FTS5 stays and the query language is ported onto it.
- Ask failures now classify credentials, balance, rate limits, timeouts, service failures, and malformed responses without exposing upstream bodies.
- Index records remain pending until semantic vectors commit, enabling interruption recovery.
- Missing external library roots are preserved, settings are backend-validated, and extraction work is bounded.
- Source opening is restricted to selected library roots and the unused Tauri shell capability was removed.
- Local API token creation is atomic, Unix permissions are repaired, and concurrent Ask requests are limited.

## [0.1.0] - 2026-09-08

### Added

- Tauri 2 desktop application for Windows, macOS, and Linux.
- Local folder onboarding, recursive discovery, file watching, and startup reconciliation.
- PDF, DOCX, text, Markdown, RST, CSV, JSON, HTML, XML, and log extraction.
- On-device multilingual embeddings with FastEmbed.
- Embedded Qdrant Edge semantic index and SQLite FTS5 keyword retrieval.
- Hybrid search with source previews, file opening, path filtering, and extension filtering.
- Redrob-powered grounded answers with numbered local citations.
- OS credential-store integration for Redrob workspace API keys.
- Privacy control that blocks excerpt transmission at the backend boundary.
- Bearer-authenticated loopback API for health, search, and Ask.
- Search, Ask, Sources, Settings, onboarding, progress, empty, and error interfaces.
- Responsive desktop layouts, keyboard navigation, accessible dialog focus management, and reduced-motion support.
- Product icons, packaging metadata, privacy documentation, and third-party notices.

### Security

- Original files, extracted passages, embeddings, and the full index remain local.
- All local API routes bind to `127.0.0.1` and require a random bearer token.
- Redrob API keys are stored in the operating system credential manager.
- Ask omits filenames and paths from transmitted model context.
