# Release QA Checklist

Complete this checklist against the exact signed release artifacts: a release candidate tag (`vX.Y.Z-rc.N`), which the workflow publishes as a **pre-release** that `latest` skips, or the artifacts of a dry run (**Run workflow** on `release.yml` with a version). Record the workflow run, commit SHA, operating-system version, architecture, and tester for every run.

## Install and lifecycle

The tag run's `Resolve the release matrix` step prints the platforms it builds — Linux x64, macOS
Apple Silicon and Windows x64 today. Skip an item only when that step reports its platform as not
built.

- [ ] Windows x64 NSIS installs per-user, starts, restarts, upgrades, and uninstalls cleanly.
- [ ] Windows x64 MSI installs, upgrades, and uninstalls cleanly.
- [ ] macOS Apple Silicon DMG opens, passes Gatekeeper, installs, launches, and is notarized.
- [ ] Linux x64 AppImage launches on the oldest supported distribution.
- [ ] Linux x64 DEB installs with dependencies and removes cleanly.
- [ ] A second launch focuses the existing window.
- [ ] Windows NSIS/MSI installers have a valid Authenticode chain from `McKinley Rice, Inc.` and a timestamp; no unsigned-code or unknown-publisher warning appears.

## Local library

- [ ] Index a folder containing every supported format with synthetic content.
- [ ] Test ASCII, Korean, Hindi, emoji, long, and composed/decomposed Unicode filenames.
- [ ] Confirm unsupported, oversized, malformed, and encrypted files do not stop other files.
- [ ] Modify, rename, move, and delete indexed files and verify reconciliation.
- [ ] Disconnect an external watched drive and confirm its records are preserved.
- [ ] Restart during extraction, embedding, and vector writes; confirm the document is repaired.
- [ ] Pause and resume a large scan.
- [ ] Test at least 100,000 files and record indexing duration, peak RAM, index size, and search latency.
- [ ] Simulate low disk space and confirm a recoverable error.

## Search and Ask

- [ ] Search works offline after the embedding model has been downloaded.
- [ ] Keyword-only fallback works when semantic retrieval is unavailable.
- [ ] Filters, citations, open-file, and reveal-file actions stay within selected folders.
- [ ] Capture Redrob API behavior for valid, invalid, expired, and revoked keys.
- [ ] Verify insufficient balance, rate limiting with `Retry-After`, timeout, malformed response, and 5xx UX; confirm each Ask sends exactly one POST with no automatic retry, including connect failures.
- [ ] Confirm Ask transmits only the question, numeric labels, and selected excerpt text.
- [ ] Confirm disabling excerpt transmission blocks network Ask in both UI and local API.
- [ ] Confirm workspace usage/balance changes match the production API contract.

## Backup, migration, and recovery

- [ ] Create a manual backup and verify it with SQLite `PRAGMA integrity_check`.
- [ ] Upgrade from every supported previous schema and retain the pre-migration backup.
- [ ] Restore a backup using `docs/RECOVERY.md`.
- [ ] Corrupt a test SQLite copy and verify quarantine without deleting the original.
- [ ] Corrupt a test vector index and verify quarantine and rebuild.
- [ ] Verify backup and data-directory permissions under each OS account model.

## Local API and security

- [ ] Every endpoint rejects absent and invalid bearer tokens.
- [ ] Token creation is atomic and Unix permissions are `0600`.
- [ ] Local API binds only to `127.0.0.1`.
- [ ] More than two concurrent Ask requests return HTTP 429.
- [ ] Upstream errors and local paths are not leaked in API responses.
- [ ] CSP blocks unexpected network, script, and resource origins.
- [ ] Dependency audits pass and third-party Action SHAs are reviewed.

## Signed updater and release publication

- [ ] A previous signed version, pointed at the release candidate's `latest.json`, detects it.
- [ ] Correct update downloads, verifies, installs, and relaunches on every platform.
- [ ] A modified artifact and invalid signature are rejected.
- [ ] No update is offered for the same or an older version.
- [ ] Backup/migration behavior is verified across the update.
- [ ] `https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json` still returns the PREVIOUS version while only the release candidate is published — a pre-release is not `latest`.
- [ ] The approved release is a non-prerelease numeric `vMAJOR.MINOR.PATCH` tag whose commit is reachable from `origin/main`.
- [ ] The `updater-manifest` job passed on the tag run: `latest.json` assembled with the tag's version, an installer present for every platform the run built, and every updater signature verified against the staged bytes (the job prints the list it required).
- [ ] `latest.json` contains a signed entry for every built platform — `linux-x86_64`, `darwin-aarch64` and `windows-x86_64`; Windows selects the signed NSIS updater payload.
- [ ] Every platform URL begins with `https://github.com/redrob-labs/redrob-recall/releases/download/`, is anonymously reachable with GET after publishing, and returns the exact approved size and SHA-256 bytes.
- [ ] The bytes a client downloads match `SHA256SUMS.txt` on the release — GitHub does not allow a published asset to be replaced under the same name, so verify the digests rather than assuming.
- [ ] A modified artifact and an invalid signature are still rejected by the installed client.
- [ ] The `publish` job is the only action that makes the version live; no separate promotion job exists to wait for.

## Release decision

- [ ] `npm run verify` passes at the release commit.
- [ ] CI and all tag-triggered release jobs pass, including validation and RustSec audit.
- [ ] Known limitations and support scope are in the release notes.
- [ ] Privacy, security, license, notices, and changelog were reviewed.
- [ ] A named release owner approves cutting the release tag; the workflow publishes it.
- [ ] The post-publication acceptance checks in `docs/RELEASING.md` pass: the `latest` manifest returns the approved version, every platform URL is anonymously reachable, and a previous production build updates into it on every platform.
