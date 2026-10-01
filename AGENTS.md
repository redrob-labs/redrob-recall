# AGENTS.md

Operating notes for an automated agent working in this repository. Everything below was read
out of the tree or measured against the live repository. `CONTRIBUTING.md` is the human working
agreement and stays authoritative on process; this file exists so an agent does not have to
infer the parts that are easy to get wrong.

## What this repository is

Redrob Recall is a local-first desktop application that indexes folders you select and answers
questions about the files in them. It is one Tauri 2 shell: a React 19 and TypeScript frontend
under `src/`, and a single Rust crate under `src-tauri/` that owns extraction, indexing, search,
and a loopback HTTP API. Files, extracted text, embeddings, and the whole search index stay on
the device in `~/.redrob/recall`; only the Ask feature leaves the machine, and it sends the
question plus a bounded number of text excerpts to the configured Redrob API, never filenames,
paths, whole files, or the index.

Keyword search is SQLite with FTS5, semantic search is Qdrant Edge embedded in-process, and
embeddings are computed locally by FastEmbed `MultilingualE5Small`. There is no server, no
Docker, and no external database.

## Branch model and release flow

`develop` is the default branch of the GitHub repository and the base of every pull request.
The setting and the practice agree: the most recently merged pull request, #16, targeted
`develop`, and at the time of writing there is no open pull request. Cut your branch from
`origin/develop` and open your pull request back into `develop`.

`main` is released state. It moves by merging `develop` into it through a `release/*` branch,
and release tags are cut from `main`. That last part is machine-checked rather than trusted:
`.github/workflows/release.yml` fetches `origin/main` and fails the run unless the tag commit
is an ancestor of it, so tagging straight off `develop` refuses to build instead of shipping.
Which branch a pull request into `main` comes from is a convention reviewers uphold, because a
GitHub ruleset cannot express it.

A hotfix branches from `main`, merges into `main`, releases, and then `main` is merged back into
`develop`. Do not skip that back-merge. The sibling repository redrob-code spent a month with a
lockfile its own default branch could not install from because that step was missed, and nothing
in CI reports the gap: both branches stay green while they drift. Measure it instead of assuming:

```bash
git fetch origin
git rev-list --count origin/develop..origin/main   # commits on main that develop lacks
git rev-list --count origin/main..origin/develop   # commits on develop not yet released
```

Measured on 2026-09-17, the first number is `0` and the second is `1`. Zero in the first position
is the healthy state: nothing has been released that `develop` has not absorbed. A non-zero first
number is the failure above, and the fix is a pull request from `main` into `develop`, not a force
push. The second number is just unreleased work and carries no warning.

Merges are squash-only and the branch is deleted on merge, so one pull request becomes one commit
on `develop`.

Those branch names are now machine-checked rather than trusted. `.github/workflows/gitflow.yml`
fails a pull request whose head branch is not `<type>/<slug>` with a type drawn from `feat`, `fix`,
`chore`, `docs`, `test`, `refactor`, `perf`, `release`, `hotfix`, which is exactly the set
`CONTRIBUTING.md` declares, and it exempts `develop` and `main` because a promotion or back-merge
branch is not named after a type. It exists because a rule nothing checks is only a preference:
branches have already appeared in this repository under a `kiro/` prefix that no document defines,
and a branch name is supposed to say what the change is, not which tool produced it. The same file
carries a second job, on `push` to `main` rather than on a pull request, that fails when `main`
holds commits `develop` does not. That is the back-merge gap described above, and it is the one
failure the two commands above cannot warn you about on their own, because both branches stay green
while they drift.

## Branch protection, as configured

Both `develop` and `main` are protected identically:

- pull requests only, so no direct pushes;
- force pushes blocked, branch deletion blocked;
- 0 required approving reviews;
- administrator enforcement off;
- required status checks: `Verify source and desktop core` and `Rust security advisories`.

That last line has to be read from two places or you will get it wrong. The classic branch
protection API reports only `Verify source and desktop core`, but the repository ruleset named
`trunk protection` is `active` over `refs/heads/main` and `refs/heads/develop` with no bypass
actors, and it requires both checks. A merge must satisfy the union of the two mechanisms, so both
of them gate.

## What CI actually runs

Two workflow files, and only one of them ever sees a pull request.

`.github/workflows/ci.yml` triggers on `pull_request` with no branch filter, and on `push` to
`main` and `develop`. It defines two jobs:

- **`Verify source and desktop core`** on `ubuntu-24.04`, pinned to Rust 1.95.0, installs the
  Linux desktop packages, runs `npm ci`, then `npm run verify`. This is the required check, so
  this job alone decides whether your pull request can merge.
- **`Rust security advisories`** on `ubuntu-24.04`, on Rust `stable` rather than the pinned
  toolchain, installs `cargo-audit` 0.22.2 and runs `cargo audit --file src-tauri/Cargo.lock`.

The advisories job runs **on every pull request** and on no schedule at all, and the `trunk
protection` ruleset **does** require it, so a red advisory blocks the merge rather than merely
reporting it. Do not read the single-check answer from the branch protection API and conclude a
lockfile change can merge red. The advisory database changes daily, so this job can turn red on a
branch that touched no dependency at all; compare the same job on `develop` before treating a
finding as yours, and when it is real, bump the crate rather than allowing the advisory.

`.github/workflows/release.yml` is named `Signed desktop release` and triggers **only** on
`push` of tags matching `v*.*.*`. It never runs on a pull request, so nothing you do in a pull
request exercises the bundling, signing, notarization, or updater-contract logic. Its credentialed
jobs sit behind the `production-release` environment and hard-fail on missing Tauri signing or
Apple notarization secrets rather than publishing unsigned artifacts. It also contains its own
copy of a `Rust security advisories` job, which is why the name appears twice in the repository.

## Layout

| Path                      | What is there                                                                                                                                         |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/`                    | React frontend. `App.tsx` is the whole interface, `lib/bridge.ts` is the single Tauri command boundary, `types.ts` the shared shapes.                 |
| `src-tauri/`              | The Rust crate. One package, not a workspace.                                                                                                         |
| `src-tauri/src/`          | `lib.rs` wires the app; `indexer.rs`, `search.rs`, `embedding.rs`, `storage.rs`, `models.rs`, `state.rs`, `commands.rs`, `local_api.rs`, `redrob.rs`. |
| `src-tauri/gen/schemas/`  | Generated by `tauri-build` and committed.                                                                                                             |
| `src-tauri/capabilities/` | Tauri permission manifest.                                                                                                                            |
| `scripts/`                | Plain Node ESM `.mjs` release and validation tooling, no bundler.                                                                                     |
| `docs/`                   | `ARCHITECTURE.md`, `RELEASING.md`, `RECOVERY.md`, `QA_CHECKLIST.md`.                                                                                  |

There is no Rust workspace and no `[workspace]` table: `src-tauri/Cargo.toml` is a single package
named `redrob-recall`, library name `redrob_recall_lib`, crate types `staticlib`, `cdylib`, `rlib`.
Every cargo command in this repository therefore needs `--manifest-path src-tauri/Cargo.toml`;
running bare `cargo` from the repository root finds no manifest.

## Commands

All of these come from `package.json`. There is no justfile and no Makefile.

```bash
npm run check:version   # node scripts/check-version.mjs, sub-second, no toolchain needed
npx tsc --noEmit        # types only, needs node_modules
npm run check           # tsc --noEmit plus cargo check --locked
npm run test:rust       # cargo test --locked
npm run lint:rust       # cargo clippy --locked --all-targets -- -D warnings
npm run build           # tsc && vite build
npm run verify          # what the required CI job runs, see below
npm run format          # prettier plus cargo fmt
```

`npm run verify` is the required check's contents in order: `check:version`, `build`,
`npm audit --audit-level=high`, `cargo fmt -- --check`, `test:rust`, `lint:rust`. Run it before
you push if you can; it is the only thing that decides your merge.

Verified in this tree:

- `npm run check:version` exits **0** in about 0.15s and prints that version 0.1.0 is consistent
  across all manifests, that the bundle icons are usable, and that the lockfile carries a package
  block for every declared optional platform dependency. This is the cheapest useful gate.
- `npx tsc --noEmit` exits **0**.

Do not start a full Rust release build to check your work. `cargo check --locked` or
`npm run test:rust` gives the same answer for a source change, and the existing `src-tauri/target`
in a working checkout is already 13 GB.

## What an agent gets wrong first

**The pinned Qdrant Edge unstable-feature workaround.** `Cargo.toml` pins `qdrant-edge = "=0.8.0"`
with an exact-version requirement, and that release uses a standard-library macro still
feature-gated on Rust 1.95. `.cargo/config.toml` compensates by forcing `RUSTC_BOOTSTRAP = "1"`
and passing `-Zcrate-attr=feature(assert_matches)`. This is deliberate and narrow. Do not widen
the enabled feature set, do not delete the file when a build looks odd, and do not relax the `=`
pin: a different Qdrant Edge version changes what the workaround has to cover. Remove it only
together with an upstream release that builds on stable.

**Three files carry the version.** `package.json`, `src-tauri/Cargo.toml`, and
`src-tauri/tauri.conf.json` must all agree, and `scripts/check-version.mjs` throws if they do not.
Bump all three in the same commit.

**Generated files are committed.** `src-tauri/gen/schemas/*.json` are tracked but produced by
`tauri-build`. Do not hand-edit them; change the capability or permission source and let a build
regenerate them.

**The toolchain is pinned, but not everywhere.** `rust-toolchain.toml` selects 1.95.0 and the
required CI job pins the same version, while the advisories job deliberately uses `stable`. A
local `cargo +stable` result is not what the required check measured.

**Two platforms ship, and one of them cannot exist.** Releases build Linux x64 and macOS Apple
Silicon. The Windows leg is skipped, not built unsigned, until the repository variable
`WINDOWS_SIGNING_READY` is `"true"`, because an unsigned installer teaches users to click through
the warning. macOS Intel is absent permanently: `ort-sys`, reached through `fastembed`, publishes
no prebuilt binary for `x86_64-apple-darwin`. `scripts/release-matrix.mjs` is the single list that
drives both the build matrix and the updater platform keys, so add a platform there rather than in
the workflow.

**Linux needs native packages before anything Rust compiles.** WebKitGTK 4.1, the Ayatana
appindicator development package, librsvg, patchelf, and libssl development headers. The CI job
installs exactly that list; copy it rather than guessing.

**The frontend can run without the shell.** `npm run dev` plus `http://localhost:1420/?demo=1`
serves demo data, which is enough for visual work and needs no Rust build.

## Pull request expectations

`.github/PULL_REQUEST_TEMPLATE.md` asks for the behaviour that changed, the problem it solves, and
the commands you actually ran with their output. Its checklist is enforced by reviewers, not by CI,
and two items are easy to trip: nothing new may leave the machine except the excerpts a user
explicitly sends with Ask, and "Qdrant" may appear only where it identifies the upstream engine,
never as Redrob product naming.

Never write an em dash in this repository. Use a colon, a comma, parentheses, or a second
sentence.
