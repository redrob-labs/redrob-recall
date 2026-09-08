# Release Guide

Redrob VectorDB releases are built as signed native installers by `.github/workflows/release.yml`. A pushed `vMAJOR.MINOR.PATCH` tag creates or updates a **draft** GitHub Release. The draft must pass the platform smoke checklist before a maintainer publishes it.

## One-time signing setup

Generate the Tauri updater signing key pair on a trusted, encrypted administrator machine:

```bash
npm run tauri signer generate -- -w ~/.tauri/redrob-vectordb.key
```

Back up the private key and its password in the Redrob secrets manager. Losing it prevents installed copies from trusting future updates. Never put private keys, certificates, passwords, or `.env` files in this repository.

Create a protected GitHub Environment named `production-release`, configure required reviewers or equivalent deployment protection, and add these environment secrets:

| Secret                               | Purpose                                                         |
| ------------------------------------ | --------------------------------------------------------------- |
| `TAURI_UPDATER_PUBLIC_KEY`           | Public updater verification key embedded into release builds    |
| `REDROB_UPDATER_ENDPOINT`            | Public HTTPS dynamic endpoint or static `latest.json` URL       |
| `TAURI_SIGNING_PRIVATE_KEY`          | Private updater artifact signing key or secure key path content |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional updater key password                                   |
| `APPLE_CERTIFICATE`                  | Base64-encoded Developer ID Application `.p12`                  |
| `APPLE_CERTIFICATE_PASSWORD`         | Password for the exported Apple certificate                     |
| `KEYCHAIN_PASSWORD`                  | Ephemeral CI keychain password                                  |
| `APPLE_ID`                           | Apple notarization account                                      |
| `APPLE_PASSWORD`                     | App-specific Apple password                                     |
| `APPLE_TEAM_ID`                      | Apple Developer team identifier                                 |
| `WINDOWS_CERTIFICATE`                | Base64-encoded Windows code-signing `.pfx`                      |
| `WINDOWS_CERTIFICATE_PASSWORD`       | Password for the Windows certificate                            |

The workflow verifies that the tag commit is reachable from `main`, requires `npm run verify` and the pinned RustSec audit to pass, and waits for approval of the protected `production-release` environment before building. It refuses to produce release artifacts when required signing material, Apple notarization credentials, or the HTTPS update endpoint is absent. Release configuration is generated only inside the runner as `src-tauri/tauri.release.conf.json`, which is ignored by Git. Because `savagemanage/openVectorDB` is private, its GitHub Release URL cannot be used directly by unauthenticated desktop clients; set `REDROB_UPDATER_ENDPOINT` to a public Redrob-hosted endpoint or publish release metadata and assets through an intentionally public distribution repository.

Tauri requires signed updater artifacts and describes the key model in its [official updater guide](https://v2.tauri.app/plugin/updater/). Apple certificate and notarization setup is covered by the [Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

## Prepare a release

1. Update `CHANGELOG.md`.
2. Set the same semantic version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
3. Run:

   ```bash
   npm ci
   npm run verify
   ```

4. Merge the reviewed release change into `main`.
5. Create and push the matching tag:

   ```bash
   git tag -s v0.1.0 -m "Redrob VectorDB v0.1.0"
   git push origin v0.1.0
   ```

6. Watch **Signed desktop release**. It builds Linux x64, macOS Intel, macOS Apple Silicon, and Windows x64 installers, signs updater bundles, and uploads `latest.json`.
7. Download the draft artifacts and complete `docs/QA_CHECKLIST.md` on clean machines.
8. Confirm checksums and signatures, then publish the draft GitHub Release.

The production updater endpoint is supplied as a `production-release` environment secret. It may be the `latest.json` asset on an intentionally public GitHub Release or a Redrob-hosted dynamic endpoint. Draft and prerelease builds must not be offered to normal clients.

## Rollback and key rotation

Do not replace a bad release with files under the same version. Remove it from publication, fix the issue, increment the patch version, and ship a new signed release. Tauri rejects downgrades by default.

Updater key rotation requires a transition release trusted by the existing key. Treat suspected key compromise as a security incident and follow `SECURITY.md` before shipping anything else.

## Reproducibility and provenance

CI installs JavaScript dependencies with `npm ci`, Rust dependencies with `--locked`, and pins every third-party GitHub Action to a full commit SHA. Native signatures and notarization timestamps intentionally make installer bytes non-reproducible. Source revision, workflow run, version tag, and uploaded installer signatures form the release audit trail.
