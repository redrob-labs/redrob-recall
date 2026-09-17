# Release Guide

Redrob Recall releases are built as signed native installers by `.github/workflows/release.yml`. A pushed `vMAJOR.MINOR.PATCH` tag creates or updates a **draft** GitHub Release. The release assets remain draft-only while the exact signed files pass platform QA. Publishing a non-prerelease GitHub Release promotes those files to the stable updater CDN.

Production builds always check `https://cdn.redrob.ai/recall/latest.json`. Promotion stores artifacts at immutable `recall/vMAJOR.MINOR.PATCH/<asset-name>` keys and publishes the mutable `recall/latest.json` manifest only after every referenced artifact is publicly reachable.

## One-time signing and CDN setup

Generate the Tauri updater signing key pair on a trusted, encrypted administrator machine:

```bash
npm run tauri signer generate -- -w ~/.tauri/redrob-recall.key
```

Back up the private key and its password in the Redrob secrets manager. Losing it prevents installed copies from trusting future updates. Never put private keys, certificates, passwords, or `.env` files in this repository.

Create a protected GitHub Environment named `production-release` and configure required reviewers or equivalent deployment protection. Release builds use these environment secrets:

| Secret                               | Purpose                                                                        |
| ------------------------------------ | ------------------------------------------------------------------------------ |
| `TAURI_UPDATER_PUBLIC_KEY`           | Public updater verification key embedded into release builds                   |
| `TAURI_SIGNING_PRIVATE_KEY`          | Private updater artifact signing key or secure key path content                |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Optional updater key password                                                  |
| `APPLE_CERTIFICATE`                  | Base64-encoded Developer ID Application `.p12`                                 |
| `APPLE_CERTIFICATE_PASSWORD`         | Password for the exported Apple certificate                                    |
| `KEYCHAIN_PASSWORD`                  | Ephemeral CI keychain password                                                 |
| `APPLE_ID`                           | Apple notarization account                                                     |
| `APPLE_PASSWORD`                     | App-specific Apple password                                                    |
| `APPLE_TEAM_ID`                      | Apple Developer team identifier                                                |
| `WIN_CSC_LINK`                       | Windows PFX as raw Base64, a `data:*;base64,...` URI, or an HTTPS download URL |
| `WIN_CSC_KEY_PASSWORD`               | Password for the Windows PFX                                                   |

The Windows import rejects every other `WIN_CSC_LINK` form and requires a currently valid certificate with a private key, the Code Signing extended key usage, and a trusted non-revoked chain. After packaging, CI verifies every NSIS/MSI Authenticode signature, signer thumbprint, and timestamp before the tag workflow can succeed. Apple Developer ID signing and notarization remain required for both macOS architectures.

The published-release promotion job uses these existing organization secrets through the same protected environment:

| Secret                         | Purpose                                             |
| ------------------------------ | --------------------------------------------------- |
| `REDROB_CDN_ACCESS_KEY_ID`     | S3-compatible access key for the updater CDN bucket |
| `REDROB_CDN_SECRET_ACCESS_KEY` | S3-compatible secret key                            |
| `REDROB_CDN_BUCKET`            | Bare S3 bucket name                                 |

The CDN credentials are available only to the promotion's S3 object-access steps, use region `ap-northeast-2`, and must be scoped to the `recall/` distribution path where possible. The asset-validation script runs in a separate credential-free step. Public access is supplied by the bucket/CDN policy; the workflow never sets a `public-read` object ACL.

The workflow verifies that release commits are reachable from `origin/main`, requires `npm run verify` and the pinned RustSec audit to pass, serializes matrix builds to prevent concurrent `latest.json` updates, and refuses missing signing or CDN configuration. Promotion also requires a successful tag-triggered release workflow for the exact tag commit and downloads every snapshotted asset by GitHub asset ID. Release configuration is generated only inside the runner as `src-tauri/tauri.release.conf.json`, which is ignored by Git.

Tauri requires signed updater artifacts and describes the key model in its [official updater guide](https://v2.tauri.app/plugin/updater/). Apple certificate and notarization setup is covered by the [Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

## Prepare and promote a release

1. Update `CHANGELOG.md`.
2. Set the same semantic version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.
3. Run:

   ```bash
   npm ci
   npm run verify
   ```

4. Merge the reviewed release change into `main`.
5. Create and push the matching numeric release tag:

   ```bash
   git tag -s v0.1.0 -m "Redrob Recall v0.1.0"
   git push origin v0.1.0
   ```

6. Watch **Signed desktop release**. The push jobs build Linux x64, macOS Intel, macOS Apple Silicon, and Windows x64 installers, sign updater bundles, and upload all files plus `latest.json` to a draft GitHub Release.
7. Keep the GitHub Release as a draft. Download those exact assets and complete `docs/QA_CHECKLIST.md` on clean machines. Draft assets must not change the stable CDN manifest.
8. Confirm native signatures, updater signatures, checksums, release notes, and QA evidence. A named release owner may then publish the non-prerelease GitHub Release.
9. The `release.published` event starts the protected `publish-cdn` job. It validates the tag, `main` ancestry, and successful tag workflow, then downloads every snapshotted asset by its GitHub asset ID. GitHub's uploaded state, SHA-256 digest, local size, platform bundle type, duplicate/unsafe names, and updater metadata are all checked before promotion.
10. Promotion reads the current manifest directly from the S3 origin, requires the candidate to be newer, uploads every non-manifest asset first, and never overwrites a versioned key with different bytes. Existing objects are accepted only when their origin bytes, SHA-256 metadata, size, content type, and cache policy match.
11. The job downloads every referenced updater payload from the public CDN and checks its size and SHA-256 before conditionally uploading `recall/latest.json` last against the origin state it validated. It then polls the public manifest with cache-busting URLs until it matches the generated manifest semantically.

Only stable numeric versions are promotable. Drafts and prereleases are never sent to the normal client channel. The generated manifest retains Tauri signatures but replaces private GitHub asset URLs with `https://cdn.redrob.ai/recall/v<version>/<URL-encoded-name>` URLs. Promotion does not receive or use the updater private signing key.

## CDN caching and acceptance

Versioned assets use `Cache-Control: public, max-age=31536000, immutable`. The stable `latest.json` uses `Cache-Control: no-store, max-age=0` and `application/json`. Every uploaded object carries SHA-256 metadata and is verified with `head-object`; reused origin objects and every referenced public updater URL are downloaded and hashed before stable metadata changes.

After promotion, confirm:

- `https://cdn.redrob.ai/recall/latest.json` returns the approved version and all four required platform keys;
- every platform URL uses the versioned Redrob CDN prefix and is anonymously reachable;
- downloaded updater bytes and signatures are the approved draft-release bytes;
- immutable assets have the long-lived cache policy and `latest.json` has the no-store policy; and
- a previous production build detects, verifies, installs, and relaunches into the promoted version on every platform.

## Rollback and key rotation

Never replace a bad release or any versioned CDN object under the same version. A failed promotion before `latest.json` publication is safe to retry only when all existing immutable objects match exactly. If a bad version reached stable clients, fix the issue, increment the patch version, repeat draft QA, and publish the newer signed release. Tauri rejects downgrades by default, so pointing `latest.json` at an older version is not a supported rollback.

Updater key rotation requires a transition release trusted by the existing key. Treat suspected key compromise as a security incident and follow `SECURITY.md` before shipping anything else.

## Reproducibility and provenance

CI installs JavaScript dependencies with `npm ci`, Rust dependencies with `--locked`, and pins every third-party GitHub Action to a full commit SHA. Native signatures and notarization timestamps intentionally make installer bytes non-reproducible. Source revision, workflow run, version tag, GitHub Release ID, promoted object hashes, and uploaded installer signatures form the release audit trail.
