# Redrob VectorDB Handoff

Last updated: 2026-09-08 UTC

## Executive summary

Redrob VectorDB is a local-first desktop search application built with Tauri 2, React 19, Rust, SQLite FTS5, Qdrant Edge, and FastEmbed. Source files, extracted content, embeddings, and indexes remain on the user's computer. Only selected excerpts are sent to the Redrob API when the user explicitly uses Ask.

The application and production-readiness hardening are on `main`. The signed updater/CDN promotion implementation is complete, reviewed, and passing CI, but it is intentionally still isolated in PR #2 because it changes `.github/workflows/release.yml` and must be merged through review rather than pushed directly to `main`.

## Repository

- Canonical repository: `redrob-labs/redrob-vectordb`
- Previous repository URL: `savagemanage/openVectorDB` (redirects to the canonical repository)
- Default branch: `main`
- Product name: Redrob VectorDB
- Binary/package name: `redrob-vectordb`
- Product message: `Everything on your computer, searchable.`

## Current delivery state

### On `main`

- Complete Tauri desktop application
- Local indexing, hybrid search, Ask, backup/recovery, local API, and updater UI
- Production signing/notarization workflow foundation
- Production-readiness hardening merged through PR #1
- This handoff document

### Pending review

- PR: [#2 — Publish signed updates to Redrob CDN](https://github.com/redrob-labs/redrob-vectordb/pull/2)
- Branch: `release/cdn-updater`
- Commit: `2a9c6e6fbe6ee441bdac46d91ad5b903b2924884`
- Target: `main`
- PR state at handoff: open, mergeable, clean
- CI: [run 34203771735](https://github.com/redrob-labs/redrob-vectordb/actions/runs/34203771735) completed successfully, including source/desktop verification and Rust security advisories

Do not bypass PR review for this change. It modifies the release workflow and receives production signing/CDN credentials when release events run.

## Updater and CDN design in PR #2

Production builds use this fixed public updater endpoint:

```text
https://cdn.redrob.ai/vectordb/latest.json
```

Versioned release assets use immutable paths:

```text
https://cdn.redrob.ai/vectordb/v<version>/<asset-name>
```

The release lifecycle is intentionally split:

1. Push a numeric `vMAJOR.MINOR.PATCH` tag whose commit is reachable from `main`.
2. GitHub Actions verifies the source and security audit.
3. Tauri builds and signs Linux x64, macOS Apple Silicon, macOS Intel, and Windows x64 artifacts.
4. `tauri-action` uploads the artifacts and `latest.json` to a draft GitHub Release.
5. Platform QA is completed against those exact draft assets.
6. A maintainer publishes the approved non-prerelease GitHub Release.
7. The protected `publish-cdn` job validates provenance and promotes the release.
8. All versioned assets are uploaded and verified first.
9. `vectordb/latest.json` is conditionally written last.
10. The workflow polls the public CDN and verifies the resulting manifest.

Draft and prerelease assets never enter the normal updater channel.

## Promotion safety properties

PR #2 implements these fail-closed controls:

- Requires a stable numeric `vMAJOR.MINOR.PATCH` tag.
- Requires the tag and release target to be reachable from `origin/main`.
- Requires a successful tag-triggered release workflow for the exact tag commit.
- Downloads the release inventory by release ID and each file by its snapshotted GitHub asset ID.
- Requires GitHub asset state `uploaded` and validates the official SHA-256 digest and size.
- Requires exactly one `latest.json` and all four supported updater platforms.
- Rejects unsupported platform keys, missing signatures, unsafe names, duplicate IDs/names/URLs, and incorrect platform bundle types.
- Rewrites updater URLs without modifying artifact signatures.
- Reads the current stable version directly from the S3 origin, not from a possibly stale CDN cache.
- Rejects a candidate version that is equal to or older than stable.
- Uses create-only conditional writes for versioned keys.
- Refuses an existing versioned key unless metadata, size, content type, cache policy, and downloaded origin bytes match.
- Downloads every referenced updater asset through the public CDN and verifies its SHA-256 and size before publication.
- Uses the previously observed origin ETag, or an object-absence precondition, when publishing `latest.json`.
- Publishes `latest.json` only after all artifact checks pass.
- Keeps the updater signing private key out of the CDN promotion job.
- Runs repository Node validation without CDN credentials in its environment.

Cache policy:

- Versioned assets: `public, max-age=31536000, immutable`
- `latest.json`: `no-store, max-age=0`

## Signing boundaries

Three independent credential groups are involved:

1. **Tauri updater signing**
   - `TAURI_UPDATER_PUBLIC_KEY`
   - `TAURI_SIGNING_PRIVATE_KEY`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   - The private key signs updater artifacts. Installed applications verify them with the embedded public key.

2. **Native platform signing**
   - Apple Developer ID certificate, keychain, and notarization credentials
   - `WIN_CSC_LINK`
   - `WIN_CSC_KEY_PASSWORD`
   - Windows import supports raw Base64 PFX, `data:*;base64,...`, or HTTPS only. It checks validity, private-key presence, Code Signing EKU, and certificate chain. Built NSIS/MSI installers are checked for a valid signer thumbprint and timestamp.

3. **CDN object access**
   - `REDROB_CDN_ACCESS_KEY_ID`
   - `REDROB_CDN_SECRET_ACCESS_KEY`
   - `REDROB_CDN_BUCKET`
   - Region: `ap-northeast-2`
   - Permissions should be restricted to the `vectordb/` distribution path where possible.

Never place secret values, private keys, PFX files, passwords, or `.env` files in this repository or this handoff.

## Required GitHub environment

Use a protected GitHub Environment named `production-release` with required reviewers or equivalent deployment protection. It must contain the updater, Apple, Windows, and CDN secrets described in `docs/RELEASING.md` after PR #2 is merged.

GitHub secret values cannot be read back for verification. Validate them by executing an approved release and observing fail-closed workflow results.

## Files changed by PR #2

- `.github/workflows/release.yml`
- `scripts/prepare-cdn-release.mjs`
- `scripts/prepare-release-config.mjs`
- `package.json`
- `docs/RELEASING.md`
- `docs/QA_CHECKLIST.md`
- `docs/ARCHITECTURE.md`

## Validation completed

The following checks passed for PR #2:

- `npm run format`
- `npm run check:version`
- `npm run build`
- `npm audit --audit-level=high` with zero vulnerabilities
- Node syntax checks for both release scripts
- Release workflow YAML parsing
- `git diff --check`
- `cargo fmt --check`
- `cargo check --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- Positive four-platform CDN fixture
- Negative fixtures for stale/equal versions, missing platforms, missing signatures, digest mismatch, wrong bundle type, duplicate assets, and unsafe asset names
- GitHub CI source/desktop verification
- GitHub CI Rust security advisory audit

A local `cargo test` attempt reached native linking but the Amazon Linux sandbox does not provide the GTK/WebKitGTK/libsoup libraries required by Tauri. GitHub's Ubuntu CI installs the native packages and completed the full verification successfully.

## What has not happened yet

- PR #2 has not been merged.
- No release tag has been pushed for CDN promotion.
- No signed installers from this pipeline have been approved for production.
- No `vectordb/v<version>/` objects have been published by this workflow.
- `https://cdn.redrob.ai/vectordb/latest.json` has not been updated by this workflow.
- Live signing credentials and CDN permissions have not been validated through an actual release run.

Do not claim production publication until the protected promotion job and the post-promotion QA checks both pass.

## Recommended next actions

1. Review and merge PR #2 into `main`.
2. Confirm the `production-release` environment and all required secret names exist.
3. Confirm the CDN credentials can read `vectordb/latest.json` and create objects only under the intended `vectordb/` path.
4. Confirm `cdn.redrob.ai` publicly serves the bucket objects and honors the expected cache behavior.
5. Update `CHANGELOG.md` and bump the version consistently in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
6. Run `npm ci` and `npm run verify`.
7. Merge the version change to `main`.
8. Create and push a signed numeric tag.
9. Keep the generated GitHub Release as a draft while completing `docs/QA_CHECKLIST.md` on clean machines.
10. Publish the non-prerelease Release only after a named release owner approves QA.
11. Verify the protected CDN promotion job and all public updater URLs.
12. Install a previous production version and confirm it detects, verifies, installs, and relaunches into the promoted version on every supported platform.

## Failure and rollback policy

- Never replace bytes under an existing versioned CDN key.
- A failed promotion before `latest.json` publication can be retried only when all existing immutable objects match exactly.
- If a bad version reaches stable clients, fix it and publish a newer signed patch version.
- Do not point `latest.json` at an older version as a rollback; Tauri rejects downgrades by default.
- Updater key rotation requires a transition release trusted by the currently embedded key.
- Treat suspected signing-key compromise as a security incident and follow `SECURITY.md`.

## Project constraints to preserve

- Console remains unchanged; this is a standalone desktop app.
- Original files, extracted data, embeddings, and indexes remain local.
- Ask sends only the question, numeric source labels, and selected excerpt text.
- Development builds do not configure a production updater endpoint.
- The normal updater checks only on explicit user action.
- Task-planning artifacts such as `.tasks/` and `.agents/tasks/` must remain ignored and uncommitted.
- CI/CD workflow changes must always go through a pull request.
