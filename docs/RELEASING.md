# Release Guide

Redrob Recall releases are built by `.github/workflows/release.yml`, the redrob-canvas release shape:
exactly three targets from the one cargo binary `redrob-recall`, signed where the platform expects it,
and published as `latest` by the shared redrob-labs publish job. GitHub Releases is the channel;
Redrob Launcher reads `SHA256SUMS.txt` and `redrob-launcher.json` from it to install and update the
app. Recall has no updater of its own.

| Target | Assets |
| --- | --- |
| Linux x86_64 | `redrob-recall-<v>-linux-x86_64.tar.gz`, `.deb` |
| macOS arm64 | `redrob-recall-<v>-macos-arm64.zip`, `.dmg` (Developer ID, notarized, stapled) |
| Windows x64 | `redrob-recall-<v>-windows-x64-portable.zip`, `-setup.exe` (Azure Artifact Signing) |

Plus `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, `UPSTREAM_NOTICES.md`, `SHA256SUMS.txt`,
`redrob-recall.png` (the 512 px icon, which Redrob Launcher shows in its catalog) and
`redrob-launcher.json` (`{ "schema": 1, "sharedAccount": true, "icon": "redrob-recall.png" }`), the
last two written by `tools/release_manifest.py` in the Linux leg and checked by the verify job. Intel macOS is not built: `ort-sys`
publishes no ONNX Runtime binary for it. `tools/release_matrix.py` is the one list of legs; the
Windows leg is skipped (with a notice) unless the repository variable `WINDOWS_SIGNING_READY` is `true`.

## Steps

1. On a `release/<version>` branch from `develop`: set `version` in the root `Cargo.toml`
   (`[workspace.package]`), turn the CHANGELOG's section into `## <version> - <date>`, run
   `python3 scripts/check-version.py`. Open the pull request into `develop`; CI's `checks` job runs
   the same script.
2. Merge it. Open the promotion pull request `develop` -> `main` and merge it with a merge commit.
3. Tag the merge commit on `main` and push the tag with an explicit refspec:

   ```bash
   git fetch origin main
   git tag v<version> origin/main
   git push origin refs/tags/v<version>:refs/tags/v<version>
   ```

   The tag push starts the release. `gh workflow run release.yml --ref main -f tag=v<version>` runs
   the same release by hand; `gh workflow run release.yml --ref <branch> -f dry_run=true` builds and
   signs the dispatched ref and publishes nothing.
4. Watch the run; the `publish` job's output is the release URL. Open the back-merge pull request
   `main` -> `develop` (the shared gitflow workflow opens one itself when `main` is ahead).

`validate` refuses a tag that is not on `main`, does not equal the workspace version, or has no dated
CHANGELOG section; then it runs the brand check and `cargo fmt --check`. Clippy and the tests ran on
the pull request and are not repeated.

## What is shared and what is this repository's

| Piece | Where | What it does here |
| --- | --- | --- |
| `gitflow.yml` (reusable) | `.github/workflows/gitflow.yml` calls it | Branch names are `<type>/<slug>`; `main` is contained in `develop`, else the back-merge pull request is opened |
| `pr-policy.yml` (reusable) | `.github/workflows/pr-policy.yml` calls it | Open pull request limits and the self-review rule |
| `actions/sign-windows` | the Windows leg | Authenticode through Azure Artifact Signing for the executable, the uninstaller and the setup |
| `release-publish.yml` (reusable) | the `publish` job | Downloads the `release-*` artifacts, writes `SHA256SUMS.txt`, creates and publishes the release (`launcher_manifest: false`: the manifest with the icon is this repository's) |

Everything else lives here: `tools/release_matrix.py` (the legs), `tools/release_manifest.py` (the
launcher manifest and icon), `tools/release/stage.py` (the
staging tree and the macOS bundle), `tools/release/make_deb.py`, `installer/macos/make-dmg.sh` (the
disk image with the uninstaller), `installer/windows/build.py` (the bilingual setup).

## Signing

The build legs run in the GitHub Environment `release`; the name is part of the organization's Azure
federated credential (`repo:redrob-labs/redrob-recall:environment:release`). The organization secrets
they consume:

| Name | Purpose |
| --- | --- |
| `APPLE_CODESIGN_CERT_P12_BASE64`, `APPLE_CODESIGN_CERT_PASSWORD` | Developer ID Application certificate, imported into a throwaway keychain |
| `APPLE_NOTARY_API_KEY_ID`, `APPLE_NOTARY_API_ISSUER_ID`, `APPLE_NOTARY_API_KEY_P8_BASE64` | App Store Connect API key for `notarytool` |
| `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` | The identity `azure/login` exchanges the job's OpenID Connect token for (plain ids) |
| `ARTIFACT_SIGNING_ENDPOINT`, `ARTIFACT_SIGNING_ACCOUNT`, `ARTIFACT_SIGNING_CERT_PROFILE` | The Azure Artifact Signing account and certificate profile |

Windows: build, then `azure/login` (the token is short-lived and the build is long), then sign the
executable BEFORE it is zipped; the uninstaller is built and signed, embedded with the zip into the
setup, and the setup is signed. There is no certificate file and no client secret anywhere. macOS:
sign with the hardened runtime, notarize, staple, and Gatekeeper-assess the app, then the disk image
in its own right.

## Local rehearsal

`tools/release/build-linux.sh` builds the Linux leg here (release build, stage, tar.gz, deb,
checksums), which is what to run before cutting a release on a machine with the Linux headers.
`gh workflow run release.yml --ref <branch> -f dry_run=true` rehearses every leg on the runners.
