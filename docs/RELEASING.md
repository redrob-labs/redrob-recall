# Release Guide

Redrob Recall releases are built as signed native installers by
`.github/workflows/release.yml`. A pushed `vMAJOR.MINOR.PATCH` tag creates or updates a
**draft** GitHub Release. Publishing that draft is the release: GitHub Releases is the
update channel itself, so there is no separate promotion step and no CDN.

Production builds check
`https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json`. That
path resolves to the newest **published, non-prerelease** release, which is what makes the
draft gate real — a draft is not "latest", so no installed client can see a version until a
named owner publishes it.

## One-time signing setup

Generate the Tauri updater signing key pair on a trusted, encrypted administrator machine:

```bash
npm run tauri signer generate -- -w ~/.tauri/redrob-recall.key
```

Back up the private key and its password in the Redrob secrets manager. Losing it prevents
installed copies from trusting future updates. Never put private keys, certificates,
passwords, or `.env` files in this repository.

Create a protected GitHub Environment named `production-release` and configure required
reviewers or equivalent deployment protection. The build job runs inside it and consumes
these **organization** secrets and variables — they are inherited by name, and the same set
is shared with the other Redrob desktop products:

| Name | Kind | Purpose |
| --- | --- | --- |
| `TAURI_SIGNING_PUBLIC_KEY` | variable | Public updater verification key embedded into release builds |
| `TAURI_SIGNING_PRIVATE_KEY` | secret | Private updater artifact signing key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | Updater key password |
| `APPLE_CODESIGN_CERT_P12_BASE64` | secret | Base64-encoded Developer ID Application `.p12` |
| `APPLE_CODESIGN_CERT_PASSWORD` | secret | Password for the exported Apple certificate |
| `APPLE_NOTARY_API_KEY_ID` | secret | App Store Connect API key id used for notarization |
| `APPLE_NOTARY_API_ISSUER_ID` | secret | App Store Connect issuer id |
| `APPLE_NOTARY_API_KEY_P8_BASE64` | secret | Base64-encoded `.p8` private key, written to the runner temp only |
| `WIN_CSC_LINK` | secret | Windows PFX as raw Base64, a `data:*;base64,...` URI, or an HTTPS download URL |
| `WIN_CSC_KEY_PASSWORD` | secret | Password for the Windows PFX |

Notarization uses the App Store Connect **API key**, not an Apple ID and app-specific
password. The Windows import rejects every other `WIN_CSC_LINK` form and requires a
currently valid certificate with a private key, the Code Signing extended key usage, and a
trusted non-revoked chain. After packaging, CI verifies every NSIS/MSI Authenticode
signature, signer thumbprint, and timestamp before the tag workflow can succeed. Apple
Developer ID signing and notarization remain required for macOS.

Windows is only built when the repository variable `WINDOWS_SIGNING_READY` is `"true"`. Until
then the leg is skipped with a notice in the run log rather than failing every tag, and no
unsigned Windows installer is produced — the omission is deliberate and visible, not a silent
downgrade. Setting that variable, once `WIN_CSC_LINK` holds a usable certificate, is the whole
re-enable step.

macOS Intel is not built at all. `ort-sys`, reached through `fastembed`, publishes no prebuilt
ONNX Runtime for `x86_64-apple-darwin`, so the leg fails after compiling the entire dependency
tree. Restoring it would mean building and vendoring `libonnxruntime` for a platform Apple no
longer sells.

No CDN credential is involved. The CDN promotion step was removed along with the CDN
itself; the release consumes only the signing material above plus the run's own
`GITHUB_TOKEN`.

The workflow verifies that release commits are reachable from `origin/main`, requires
`npm run verify` and the pinned RustSec audit to pass, serializes matrix builds to prevent
concurrent `latest.json` updates, and refuses missing signing configuration rather than
publishing unsigned artifacts. Release configuration is generated only inside the runner as
`src-tauri/tauri.release.conf.json`, which is ignored by Git.

Tauri requires signed updater artifacts and describes the key model in its
[official updater guide](https://v2.tauri.app/plugin/updater/). Apple certificate and
notarization setup is covered by the
[Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

## Cut a release

1. Update `CHANGELOG.md`.
2. Set the same semantic version in `package.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/tauri.conf.json`.
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

6. Watch **Signed desktop release**. Its `Resolve the release matrix` step prints exactly which
   platforms this run builds — Linux x64 and macOS Apple Silicon today, plus Windows x64 when
   `WINDOWS_SIGNING_READY` is set. It signs the updater bundles and uploads every file plus
   `latest.json` to a **draft** GitHub Release.
7. The `verify-release-assets` job then fails the run unless the draft carries `latest.json`,
   an installer for every platform that was built, and a manifest whose version matches the tag
   and whose every platform entry is signed and points at this repository's own release assets.
   A manifest whose URLs point anywhere else is the silent-freeze failure this job exists to
   catch. It also rewrites those URLs from the REST asset form `tauri-action` must use on a
   draft into the published `releases/download/<tag>/<name>` form clients will actually fetch.
8. Keep the release a draft. Download those exact assets and complete
   `docs/QA_CHECKLIST.md` on clean machines. Draft assets are not reachable through
   `releases/latest/download/...`, so no installed client sees them.
9. Confirm native signatures, updater signatures, checksums, release notes, and QA
   evidence. A named release owner then publishes the non-prerelease GitHub Release, and
   that publication is what makes the version live.

## Acceptance after publishing

- `https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json`
  returns the approved version and a signed key for every platform the run built.
- Every platform URL is anonymously reachable with GET and returns the exact approved size
  and SHA-256 bytes — the same bytes QA downloaded from the draft.
- A previous production build detects, verifies, installs, and relaunches into the new
  version on every platform.

Assets are immutable by construction: GitHub does not let a published release asset be
replaced with different bytes under the same name, so there is no cache policy to configure
and no versioned-key contract to enforce.

## Rollback and key rotation

Never replace a bad release under the same version. If a bad version reached clients, fix
the issue, increment the patch version, repeat draft QA, and publish the newer signed
release. Tauri rejects downgrades by default, so pointing the channel at an older release is
not a supported rollback — unpublishing the bad release (returning it to draft) is what
removes it from `latest`.

Updater key rotation requires a transition release trusted by the existing key. Treat
suspected key compromise as a security incident and follow `SECURITY.md` before shipping
anything else.

## Reproducibility and provenance

CI installs JavaScript dependencies with `npm ci`, Rust dependencies with `--locked`, and
pins every third-party GitHub Action to a full commit SHA. Native signatures and
notarization timestamps intentionally make installer bytes non-reproducible. Source
revision, workflow run, version tag, GitHub Release ID, and uploaded installer signatures
form the release audit trail.
