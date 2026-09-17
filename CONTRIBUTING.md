# Contributing

Thanks for helping. This is the working agreement for the repository: how branches are named, what has
to be green before a merge, and the promise the product makes that a change must not quietly break.

## The promise

Redrob Recall is **local-first**. Files, extracted text, embeddings and the whole search index stay
on the device. Search is local and free. When a user asks a question, only that question and the few
relevant excerpts leave the machine; filenames, paths, whole files and the index do not.

That sentence is on the README and in the product, so treat it as a contract:

- **Adding a network call is a review item, not a detail.** Say in the pull request what leaves the
  device and why.
- **Never send a path or a filename** to an API. Excerpt text is the boundary.
- **Local artefacts stay under the app's own data directory**: `metadata.db`, `qdrant-edge/`,
  `models/`, and the generated `local-api-token`, which is `0600` on Unix because it is a credential
  even though it never leaves the machine.

## Branch model

One long-lived branch, `main`. Short-lived branches off it, merged by pull request. That is the whole
model — there is no `develop`, no release branch and no long-lived integration branch to keep in sync.

- **`main`** is the trunk. A GitHub ruleset enforces it rather than trusting this document:
  - no direct pushes — every change arrives as a pull request;
  - no force pushes and no deletion of the branch;
  - linear history, so `main` reads as a list of changes rather than a graph;
  - required status checks must pass — they do not have to pass against the newest
    `main`, so a queue of bot updates does not have to rebase and re-run one at a time;
  - review threads must be resolved before merge.
- **Working branches** are `<type>/<short-slug>`, e.g. `fix/index-watcher-restart`,
  `feat/hwp-extraction`. Types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `perf`.
- **Merging is squash-only**, and the branch is deleted on merge. One pull request becomes one commit
  on `main`, so `git log main` is the changelog. The squash commit's body is the pull request body,
  not a concatenation of your work-in-progress messages.
- Release tags are cut from `main`, formatted `v<major>.<minor>.<patch>`.

Fork the repository, push your branch to your fork, and open the pull request from there. You do not
need write access to contribute, and pull requests from forks run CI with no repository secrets — the
signing credentials belong to `release.yml`, which only a maintainer's tag can start.

[redrob-code](https://github.com/redrob-labs/redrob-code) runs Git Flow with a `develop` branch.
This one does not, so cut from `main`.

## Day to day

```bash
git switch main && git pull
git switch -c fix/short-description

npm install
npm run check
npm run build

cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace

# open a pull request into main
```

A change to extraction should come with a file that exercises it. The formats this reads (PDF, DOCX,
HWP, CSV, and the rest) fail in ways that only a real document shows: a PDF whose text layer is
missing, a spreadsheet whose numbers are dates.

### Commits

Subject in the imperative. Use the body to say _why_, and when a change touches what leaves the
device, say so in the body rather than only in the diff.

## What CI checks

Both `ci.yml` jobs are required before a merge, and neither consumes a secret, so a pull request from
a fork gets the same run as one from a branch here:

| Check | What it runs |
| --- | --- |
| **Verify source and desktop core** | `npm ci` then `npm run verify` — the full build, formatting, tests and Clippy |
| **Rust security advisories** | `cargo audit` against the locked dependency set |

The advisory scan reads a database that changes daily, so it can go red on a branch that changed no
dependencies; check the same job on `main` before assuming it is yours. When it is a real finding,
bump the crate rather than allowing the advisory.

**`release.yml`** builds and publishes on a tag. It is maintainer-only by construction: it fires on
`push: tags` and `release: published`, verifies the tag commit is an ancestor of `main`, and puts the
credentialed jobs behind the `production-release` environment.

Run `npm run check` and `cargo test --workspace` before pushing.
