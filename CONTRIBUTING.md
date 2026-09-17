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

One long-lived branch, `main`. Short-lived branches off it, merged by pull request.

- **`main`** is the trunk: no direct pushes, no force pushes, no deletion.
- **Working branches** are `<type>/<short-slug>`, e.g. `fix/index-watcher-restart`,
  `feat/hwp-extraction`. Types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `perf`.
- Release tags are cut from `main`, formatted `v<major>.<minor>.<patch>`.

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

- **`ci.yml`** builds the desktop app and runs the Rust tests and the advisory scan. The advisory
  scan reads a database that changes daily, so it can go red on a branch that changed no
  dependencies; check the same job on `main` before assuming it is yours.
- **`release.yml`** builds and publishes on a tag.

Run `npm run check` and `cargo test --workspace` before pushing.
