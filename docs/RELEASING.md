# Release Guide

Redrob Recall releases are built as signed native installers by
`.github/workflows/release.yml`. A pushed `vMAJOR.MINOR.PATCH` tag builds every platform,
checks the updater contract over the exact files, and **publishes** the GitHub Release as
`latest` through the shared redrob-labs publish workflow. GitHub Releases is the update
channel itself, so publication is what makes the version live; there is no draft to click and
no CDN.

Production builds check
`https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json`. That
path resolves to the newest **published, non-prerelease** release. A version with a
pre-release suffix (`v0.4.0-rc.1`) is published as a pre-release, which `latest` skips, so a
release candidate can be installed by hand without any installed client seeing it.

## What is shared and what is this repository's

Three pieces are the redrob-labs organization's, in `redrob-labs/.github`, and this repository
calls them rather than carrying a copy:

| Piece | Where | What it does here |
| --- | --- | --- |
| `gitflow.yml` (reusable) | `.github/workflows/gitflow.yml` calls it | Branch names are `<type>/<slug>` from the org type list; `main` is contained in `develop`, and the back-merge pull request is opened automatically when it is not |
| `actions/sign-windows/sign.ps1` | checked out at `.redrob-shared` on the Windows leg | Authenticode through Azure Artifact Signing; Tauri's bundler runs it as `bundle.windows.signCommand` for the executable, the NSIS installer and the MSI |
| `release-publish.yml` (reusable) | the `publish` job | Downloads the `release-*` artifacts, writes `SHA256SUMS.txt` and `redrob-launcher.json`, creates the release and publishes it |

Everything else -- version resolution, the build matrix, macOS notarization, the AppImage
repack, the launcher asset names and the updater manifest -- is this repository's and lives in
`release.yml` and `scripts/`.

## One-time signing setup

Generate the Tauri updater signing key pair on a trusted, encrypted administrator machine:

```bash
npm run tauri signer generate -- -w ~/.tauri/redrob-recall.key
```

Back up the private key and its password in the Redrob secrets manager. Losing it prevents
installed copies from trusting future updates. Never put private keys, certificates,
passwords, or `.env` files in this repository.

The build legs and the publish job run in the GitHub Environment named `release`. That name is
part of the organization's Azure federated credential
(`repo:redrob-labs/redrob-recall:environment:release`), so it cannot be renamed on this side.
The jobs consume these **organization** secrets and variables -- inherited by name, shared with
the other Redrob desktop products:

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
| `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` | secret | Identity `azure/login` exchanges the job's OpenID Connect token for (plain ids, not credentials) |
| `ARTIFACT_SIGNING_ENDPOINT`, `ARTIFACT_SIGNING_ACCOUNT`, `ARTIFACT_SIGNING_CERT_PROFILE` | secret | The Azure Artifact Signing account and certificate profile `sign.ps1` signs with |

Notarization uses the App Store Connect **API key**, not an Apple ID and app-specific
password.

Windows signing has no certificate file and no client secret anywhere. The leg compiles first,
then logs in to Azure with the job's OpenID Connect token (`permissions: id-token: write`),
then bundles: Tauri calls the shared `sign.ps1` for every file it signs, and that script
verifies each one with `signtool verify /pa`, requires `Status: Valid`, and requires the signer's
CN to be `McKinley Rice, Inc.`. The workflow then checks both installers again and refuses
anything but a Valid, timestamped signature from that CN. On a tag the leg refuses to run at all
when the Azure or Artifact Signing configuration is missing -- an unsigned installer that
reaches a user is worse than none. A dry run without that configuration builds unsigned
installers with a warning so a fork can still exercise the workflow.

macOS Intel is not built at all. `ort-sys`, reached through `fastembed`, publishes no prebuilt
ONNX Runtime for `x86_64-apple-darwin`, so the leg fails after compiling the entire dependency
tree. Restoring it would mean building and vendoring `libonnxruntime` for a platform Apple no
longer sells.

The workflow verifies that release tag commits are reachable from `origin/main`, requires
`npm run verify` and the pinned RustSec audit to pass, and refuses missing signing
configuration rather than publishing unsigned artifacts. Release configuration is generated
only inside the runner as `src-tauri/tauri.release.conf.json`, which is ignored by Git.

Tauri requires signed updater artifacts and describes the key model in its
[official updater guide](https://v2.tauri.app/plugin/updater/). Apple certificate and
notarization setup is covered by the
[Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

## Asset names and the updater manifest

Redrob Launcher installs a release asset only when its name is
`redrob-recall-<version>-<os>-<arch>.<ext>` -- for example
`redrob-recall-0.2.1-linux-x86_64.AppImage`, `redrob-recall-0.2.1-macos-aarch64.dmg`,
`redrob-recall-0.2.1-windows-x64.msi`. Tauri's bundler writes `Redrob Recall_0.2.1_amd64.AppImage`,
which the launcher cannot parse, so each build leg copies its bundles into the release artifact
under the launcher names (`scripts/stage-release-assets.mjs`; the rules live in
`scripts/release-asset-names.mjs` and are unit-tested) and writes its updater platform entries
-- signature text plus the **published** download URL, which is fully determined by the tag and
the asset name -- as a fragment.

The `updater-manifest` job merges those fragments into `latest.json`
(`scripts/assemble-updater-manifest.mjs`) and fails the run unless every asset parses as a
launcher name for this version, every built leg left an installable one, every built platform
has a signed entry pointing at this repository's own release assets, and every signature
**verifies against the staged bytes** (`scripts/verify-updater-signature.mjs`). The last check is
why the AppImage repack re-signs: a stale signature looks signed and breaks every future update.
`latest.json` then rides to the publish job as a `release-*` artifact like every other asset.

`redrob-launcher.json` (`{ "schema": 1, "sharedAccount": true }`, the file Redrob Launcher reads
to learn that this release uses its shared sign-in) and `SHA256SUMS.txt` are written by the
shared publish job, not here.

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

6. Watch **Signed desktop release**. `Resolve the release matrix` prints the platforms the run
   builds -- Linux x64, macOS Apple Silicon and Windows x64. The Windows leg logs
   `signed by CN=McKinley Rice, Inc.` for the executable and each installer.
7. The `updater-manifest` job assembles `latest.json` and fails the run on any gap described
   above. Nothing is published until it passes.
8. The `publish` job creates **Redrob Recall v0.1.0**, published as `latest`, with every
   installer, `latest.json`, `redrob-launcher.json` and `SHA256SUMS.txt`. Installed clients see
   the version from that moment.

A release candidate rehearses all of it: tag `v0.2.0-rc.1` publishes a **pre-release**, which
`latest` skips, so it can be installed and checked against `docs/QA_CHECKLIST.md` on clean
machines while installed clients keep their current version. A dry run that publishes nothing is
**Run workflow** on `release.yml` with a version such as `0.2.0-rc.1`: every job builds, signs
and checks exactly as on a tag, and the artifacts stay on the run.

## Acceptance after publishing

- `https://github.com/redrob-labs/redrob-recall/releases/latest/download/latest.json`
  returns the released version and a signed key for every platform the run built.
- Every platform URL is anonymously reachable with GET and returns the exact size and SHA-256
  bytes `SHA256SUMS.txt` lists.
- A previous production build detects, verifies, installs, and relaunches into the new
  version on every platform.

Assets are immutable by construction: GitHub does not let a published release asset be
replaced with different bytes under the same name, and the shared publish job refuses to touch
an already published release, so there is no cache policy to configure and no versioned-key
contract to enforce.

## Rollback and key rotation

Never replace a bad release under the same version. If a bad version reached clients, fix
the issue, increment the patch version, and publish the newer signed release. Tauri rejects
downgrades by default, so pointing the channel at an older release is not a supported
rollback -- unpublishing the bad release (returning it to draft, or marking it a pre-release)
is what removes it from `latest`.

Updater key rotation requires a transition release trusted by the existing key. Treat
suspected key compromise as a security incident and follow `SECURITY.md` before shipping
anything else.

## Reproducibility and provenance

CI installs JavaScript dependencies with `npm ci`, Rust dependencies with `--locked`, and
pins every third-party GitHub Action to a full commit SHA. Native signatures and
notarization timestamps intentionally make installer bytes non-reproducible. Source
revision, workflow run, version tag, GitHub Release ID, `SHA256SUMS.txt` and the updater
signatures form the release audit trail.
