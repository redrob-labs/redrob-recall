# Contributing

**English** · [한국어](./CONTRIBUTING.ko.md)

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

Two long-lived branches. `develop` is where work lands; `main` is what has been released.

- **`develop`** is the default branch and the integration branch. Cut working branches from it and
  open pull requests back into it. Clone the repository and you are on `develop`.
- **`main`** is the released state. It takes pull requests only from `release/*` and `hotfix/*`
  branches, and release tags are cut from it. Nothing else merges here.
- **Working branches** are `<type>/<short-slug>` off `develop`, e.g. `fix/index-restart`, `feat/pdf-extract`. Types: `feat`,
  `fix`, `chore`, `docs`, `test`, `refactor`, `perf`.
- **Merging is squash-only**, and the branch is deleted on merge. One pull request becomes one
  commit, so `git log develop` reads as a list of changes rather than a graph. The squash commit's
  body is the pull request body, not a concatenation of your work-in-progress messages.

Both branches are enforced by GitHub rulesets rather than by this document:

- no direct pushes — every change arrives as a pull request;
- no force pushes and no deletion of the branch;
- linear history;
- required status checks must pass — they do not have to pass against the newest tip, so a queue of
  bot updates does not have to rebase and re-run one at a time;
- review threads must be resolved before merge.

A ruleset cannot express *which* branch a pull request comes from, so "only `release/*` and
`hotfix/*` merge into `main`" is a convention this document carries and reviewers uphold. One part
of it is machine-checked: the release workflow refuses to build a tag whose commit is not reachable
from `origin/main`, so tagging straight off `develop` fails instead of shipping.

### Releasing

```bash
git switch develop && git pull
git switch -c release/v0.2.0
# bump the version, update the changelog, run the release check
# open a pull request into main and merge it, then tag main:
git switch main && git pull
git tag -a v0.2.0 -m "Redrob Recall v0.2.0"
git push origin v0.2.0
# bring main's release commit back so develop does not fall behind:
git switch -c chore/sync-main-to-develop main
# open a pull request into develop
```

A hotfix is the same shape with `hotfix/*` cut from `main` rather than `develop`, and it merges into
both.

Fork the repository, push your branch to your fork, and open the pull request from there. You do not
need write access to contribute, and pull requests from forks run CI with no repository secrets.

## Day to day

```bash
git switch develop && git pull
git switch -c fix/short-description

npm install
npm run check
npm run build

cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace

# open a pull request into develop
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
dependencies; check the same job on `develop` before assuming it is yours. When it is a real finding,
bump the crate rather than allowing the advisory.

**`release.yml`** builds and publishes on a tag. It is maintainer-only by construction: it fires on
`push: tags`, verifies the tag commit is an ancestor of `main`, and puts the credentialed jobs behind
the `production-release` environment. That ancestor check is what makes the branch model real — a tag
cut from `develop` is refused rather than shipped.

Run `npm run check` and `cargo test --workspace` before pushing.

## Dependency updates

Dependabot **version updates are off**: they produced a standing queue of pull requests, each
needing its own CI run, and on this repository the churn cost more than it caught. Two things
replace them:

- **Dependabot security updates are on.** A dependency with a known advisory still gets a pull
  request opened automatically. That is the part worth interrupting for.
- **The advisory job gates every pull request.** `cargo audit` runs against the committed
  lockfile, so a vulnerable dependency cannot merge even if nobody read an alert. A security
  update is a notification; this is the control.

Routine bumps are therefore deliberate: bump what you need for the change you are making, in the
same pull request, and say why in the body. Do not sweep unrelated versions into a feature branch.
