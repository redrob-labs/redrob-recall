# AGENTS.md

Operating notes for an automated agent working in this repository. `CONTRIBUTING.md` is the human
working agreement and stays authoritative on process; this file exists so an agent does not have to
infer the parts that are easy to get wrong.

## What this repository is

Redrob Recall is the local memory of the Redrob apps: a Cargo workspace in the redrob-canvas shape.

| Path | What is there |
| --- | --- |
| `crates/redrob-recall-core/` | The memory layer as a library: `library.rs` (the `Library`: data directory, settings, indexing state, folder add/remove), `indexer.rs` (+ `indexer_tests.rs`), `storage.rs` (SQLite + FTS5), `embedding.rs` (fastembed), `search.rs` (fusion), `query.rs` (the FTS5 query compiler), `ask.rs` (evidence and prompts), `models.rs` (the types; also the wire shapes). `tests/fixtures/library/` are the indexer tests' documents. No window, no network. |
| `crates/redrob-recall-api/` | `server.rs` (axum, 127.0.0.1, bearer token; feature `server`), `client.rs` (`RecallClient`; feature `client`), `types.rs` (the wire types, mirroring the core models so a client does not pull the core crate), `paths.rs`. `docs/API.md` is the contract. |
| `apps/redrob-recall/` | The eframe/egui window and the CLI: `main.rs` (clap: none = window, `serve`, `index`, `search`), `shell.rs` (Library / Search / Ask / Settings), `work.rs` (background search and ask jobs), `account.rs` (the shared account reader and watcher), `console.rs` (the chat-completions call), `services.rs` (window options, open/reveal), `fonts.rs` (CJK fallback), `build.rs` (the Windows icon resource). |
| `brand/SOURCES.json` | Pins (sha256) of every design-system input and every built brand asset. Written by `scripts/build-brand-assets.py`, checked by `scripts/check-brand.py`. Never edit by hand. |
| `resources/icons/`, `resources/brand/` | The built icons (ico, icns, hicolor, svg) and lockups. Owned by the brand pipeline; a stray file here fails the check. |
| `installer/windows/` | The native bilingual setup and uninstaller (C++/Direct2D; from redrob-launcher). `installer/macos/`: the disk image and its uninstaller (from redrob-canvas). |
| `tools/release/`, `tools/release_matrix.py` | Staging, `.deb`, the Linux rehearsal script, the one list of release legs. |
| `scripts/` | `build-brand-assets.py`, `check-brand.py`, `check-version.py`. Python 3, no dependencies for the checks. |
| `docs/` | `API.md`, `ARCHITECTURE.md`, `RELEASING.md`, `RECOVERY.md`, `index-engine-decision.md`, `upstream-grammar.md`, `upstream-sources.toml`. |

Everything the app stores is under `~/.redrob/recall` (`REDROB_RECALL_DATA_DIR` overrides it).

## Branch model and release flow

`develop` is the default branch and the base of every pull request. `main` is released state: a
`release/<version>` branch bumps the version and dates the CHANGELOG, merges into `develop`, then
`develop` is promoted into `main` with a merge commit, the tag `v<version>` is pushed from `main`, and
`main` is back-merged into `develop`. `.github/workflows/release.yml` refuses a tag that is not an
ancestor of `main`. The shared redrob-labs `gitflow` workflow checks branch names (`<type>/<slug>`)
and opens the back-merge pull request when `main` is ahead of `develop`.

## What CI runs

`.github/workflows/ci.yml`, on pull requests only:

- **brand pins, version**: `scripts/check-brand.py` and `scripts/check-version.py`. Seconds.
- **fmt, clippy, test (profile ci)**: `cargo fmt --all -- --check`, `cargo clippy -p redrob-recall -p
  redrob-recall-core -p redrob-recall-api --all-targets --locked --profile ci -- -D warnings`, `cargo
  test` of the same crates. Skipped when no Rust input changed, and on pull requests into `main`.

`gitflow.yml` and `pr-policy.yml` only call the org-wide reusable workflows. There is no advisory job in
CI; run `cargo audit` before a release.

## The gates, locally

```bash
cargo fmt --all -- --check
cargo clippy -p redrob-recall -p redrob-recall-core -p redrob-recall-api --all-targets --profile ci -- -D warnings
cargo test -p redrob-recall-core -p redrob-recall-api --profile ci
python3 scripts/check-brand.py && python3 scripts/check-version.py
```

The indexer tests load the real embedding model (465 MB, downloaded once into
`crates/redrob-recall-core/target/test-model-cache`) and run one at a time. The workspace lints deny
`unwrap`, `expect`, `panic`, `unimplemented` and `unreachable` in non-test code; test modules carry
`#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.

## What an agent gets wrong first

**The pinned Qdrant Edge unstable-feature workaround.** `qdrant-edge = "=0.8.0"` uses a standard-library
macro still feature-gated on Rust 1.95; `.cargo/config.toml` forces `RUSTC_BOOTSTRAP = "1"` and passes
`-Zcrate-attr=feature(assert_matches)`. Do not widen it, do not delete it, do not relax the `=` pin.

**One version.** `version` lives once, in the root `Cargo.toml` `[workspace.package]`; every crate
inherits it. `scripts/check-version.py` requires a dated `## <version> - <date>` CHANGELOG section.

**Brand assets are built, not drawn.** Run `scripts/build-brand-assets.py --design-system
<redrob-design-system>` (needs ImageMagick 7 and `resvg`) and commit `brand/SOURCES.json` with the
outputs. The icon is the Desk tile (`product-desk`, `#D87101`) with the shipped Redrob symbol and a lens
in the doorway; the 16..48 px renders carry no lens.

**The account is read, never written.** `apps/redrob-recall/src/account.rs` follows the launcher's
contract: `REDROB_API_KEY`, else the app's own `account.json`, else the shared one. Nothing in this
repository writes any of them, and the key is never logged or displayed.

**The core crate is synchronous.** Passes run on `std::thread`s inside `redrob-recall-core`; only the
app and the API crate use tokio. Do not add an async runtime to the core crate.

**Three platforms ship, and a fourth cannot exist.** Linux x86_64, macOS arm64, Windows x64, from
`tools/release_matrix.py`. Intel macOS has no ONNX Runtime binary. No Flatpak, AppImage or FreeBSD.

**Linux needs native packages before the window compiles.** `libxkbcommon-dev libwayland-dev
libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libgtk-3-dev`; CI installs exactly that list.

**The Windows installer cannot be built here.** `installer/windows/build.py` needs Visual Studio; the
release workflow builds and signs it. Keep its strings bilingual (`Tr(L"...", L"...")`).

## Pull request expectations

`.github/PULL_REQUEST_TEMPLATE.md` asks for the behaviour that changed, the problem it solves, and the
commands you actually ran with their output. Two items are easy to trip: nothing new may leave the
machine except the passages a user explicitly sends with Ask, and "Qdrant" may appear only where it
identifies the upstream engine, never as Redrob product naming.

Never write an em dash in this repository. Use a colon, a comma, parentheses, or a second sentence.
