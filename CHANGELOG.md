# Changelog

All notable changes to Redrob Recall are documented here.

## [Unreleased]

### Fixed

- Ask failed with "Redrob could not accept this request" while the connection panel said Disconnect. Two causes: the request named the model `auto`, which the Redrob Code daemon resolves only as `redrob/auto` (it answered 400 "model auto is not available on this engine"), and the key the daemon held was not the one Redrob Launcher had signed the computer in with. The daemon path now sends `redrob/<model>` when the setting names no provider, and a refusal appends what Redrob actually said ("Redrob said: ...") instead of only its class.
- The Windows installer showed the stock NSIS art and only English. It now carries the Recall icon, a header and Welcome/Finish sidebar drawn from the app icon, offers English and Korean with the language selector, and names Redrob as the publisher (Apps & features, the MSI and the dmg).

### Added

- The app reads Redrob Launcher's shared sign-in itself (`account.json` in `$XDG_CONFIG_HOME/redrob-shared`, `~/Library/Application Support/ai.redrob.shared` or `%APPDATA%\Redrob\Shared`; `REDROB_SHARED_DIR` overrides) at startup, on every connection refresh and whenever the file changes. A key there is this app's sign-in: the panel says "Signed in with Redrob Launcher as <account>" and Ask spends that key at the file's `apiBaseUrl`, the shared-account contract's own path. Without the file the Redrob Code daemon path is unchanged.

### Changed

- The release workflow uses the shared redrob-labs pieces: `gitflow.yml` calls the org's reusable workflow (one branch-type list, automatic back-merge pull request); Windows installers are Authenticode-signed through Azure Artifact Signing with the job's OpenID Connect login and the shared `sign.ps1` as Tauri's sign command (no certificate file, no `WIN_CSC_*` secrets, `signtool verify /pa` must read Valid from `McKinley Rice, Inc.`); and the release is created and **published as latest** by the shared `release-publish` workflow, which also writes `SHA256SUMS.txt` and `redrob-launcher.json`. Nothing is drafted for a human to publish any more; a version with a `-` suffix publishes as a pre-release, which `latest` skips.
- Each build leg stages its bundles under the Redrob Launcher names and writes its updater entries with the published download URLs; the `updater-manifest` job assembles `latest.json` from them and verifies every signature against the staged bytes before anything is published. The draft-release asset renamer, URL rewriter, manifest writer and release checker scripts are gone. `workflow_dispatch` with a version is a dry run. Build legs run in parallel. The `WINDOWS_SIGNING_READY` variable is no longer read. No application change.

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
