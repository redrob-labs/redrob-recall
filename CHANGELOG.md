# Changelog

All notable changes to Redrob Recall are documented here.

## [Unreleased]

### Added

- Signed multi-platform GitHub release workflow with Tauri updater artifacts and draft-release QA gates.
- Explicit update check and signed install flow in Settings.
- Versioned SQLite migrations, integrity checks, pre-migration/manual backups, and recovery documentation.
- Automated Rust tests, locked verification, RustSec auditing, and a cross-platform release checklist.

### Changed

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
