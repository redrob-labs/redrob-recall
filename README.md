# Redrob Recall

**English** · [한국어](./README.ko.md)

Redrob Recall is the local memory of the Redrob apps: an embedding and keyword index of the folders
you choose, built and kept current on your own machine, that Redrob Office, Chat, Code, Canvas and
the rest can search through one loopback API. It comes with a thin window of its own for adding
folders, searching, and asking a question whose answer cites your files.

Files, extracted text, embeddings and the whole index stay on the device. Search is free and local.
**Ask** sends only your question and the few matching passages to the Redrob console, with the account
Redrob Launcher signed this computer in with; filenames, paths, whole files and the index are never
sent.

## What it is

| Part | Where | What it does |
| --- | --- | --- |
| `redrob-recall-core` | `crates/redrob-recall-core` | The memory layer as a library: extraction (PDF, DOCX, TXT, Markdown, RST, CSV, JSON, HTML, XML, logs), chunking, SQLite + FTS5 keyword index, local multilingual embeddings in Qdrant Edge, hybrid search, folder watcher, the library store. No window, no network. |
| `redrob-recall-api` | `crates/redrob-recall-api` | The loopback HTTP API (127.0.0.1, one bearer token) the other Redrob apps call, and a typed Rust client for it. [docs/API.md](docs/API.md). |
| `redrob-recall` | `apps/redrob-recall` | The desktop window (eframe/egui on wgpu) and the CLI: `serve`, `index <dir>`, `search <query>`. The window serves the API while it is open. |

Everything lives in `~/.redrob/recall` (`REDROB_RECALL_DATA_DIR` overrides it):

- `metadata.db`: document metadata, extracted passages and the FTS5 index
- `qdrant-edge/`: the local vector index
- `models/`: the downloaded embedding model (`multilingual-e5-small`, 384 dimensions)
- `local-api-token`, `local-api-address`: how a client on this machine finds and authenticates to the API
- `backups/`: consistent copies of `metadata.db` ([Backup and recovery](docs/RECOVERY.md))

Original files are never copied into this directory.

## The window

- **Library**: the folders, how many files each holds, the running pass with its progress, Pause /
  Resume / Check now, Add folder, Remove.
- **Search**: results ranked by meaning and by keyword together, the matching snippet, Open and
  Reveal in the file manager. Quoted phrases, `-excluded` words, `heading:word` and `prefix*` work.
- **Ask**: a question, the console's answer with `[1]`, `[2]` citations, the sources beneath it with
  Open and Reveal. Needs the shared sign-in; without it the locally found sources are still shown.
- **Settings**: the answer model, file types and passage size, the local API switch and port, the
  account status, health check, backup, clear.

## Sign-in

Recall has no sign-in of its own. Sign in once in [Redrob Launcher](https://github.com/redrob-labs/redrob-launcher/releases/latest)
and every Redrob app on the computer, Recall included, uses that account: the launcher writes a
shared `account.json` and Recall reads it (and notices when it changes). A key exported as
`REDROB_API_KEY`, or an `account.json` of Recall's own in its data folder, wins over the shared one.
Recall never writes, logs or displays the key.

## Use it from another app

```bash
TOKEN="$(cat ~/.redrob/recall/local-api-token)"
BASE="$(cat ~/.redrob/recall/local-api-address)"   # http://127.0.0.1:47331 by default

curl -H "Authorization: Bearer $TOKEN" "$BASE/health"

curl -X POST "$BASE/v1/search" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"India launch decision","limit":8}'

curl -X POST "$BASE/v1/ask-context" -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"query":"What did we decide about India?","maxSources":6}'
```

From Rust, depend on `redrob-recall-api` with the `client` feature and call
`RecallClient::discover()`. [docs/API.md](docs/API.md) has every route.

## Run for development

Rust 1.95.0 (selected by `rust-toolchain.toml`). On Linux the window links against the X11/Wayland/GL
headers egui, wgpu and rfd need:

```bash
sudo apt-get install -y libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
  libgl1-mesa-dev libgtk-3-dev
```

```bash
cargo run -p redrob-recall                       # the window
cargo run -p redrob-recall -- serve              # the API alone
cargo run -p redrob-recall -- index ~/Documents  # index a folder from the terminal
cargo run -p redrob-recall -- search "budget"
```

The first index or search downloads the embedding model into `~/.redrob/recall/models`; after that
everything works offline. The gates CI runs:

```bash
cargo fmt --all -- --check
cargo clippy -p redrob-recall -p redrob-recall-core -p redrob-recall-api --all-targets --profile ci -- -D warnings
cargo test -p redrob-recall-core -p redrob-recall-api --profile ci
python3 scripts/check-brand.py && python3 scripts/check-version.py
```

### Qdrant Edge compiler compatibility

The workspace pins Qdrant Edge 0.8.0, which uses a standard-library macro still feature-gated on
Rust 1.95. `.cargo/config.toml` narrowly enables that one crate attribute (`RUSTC_BOOTSTRAP` plus
`-Zcrate-attr=feature(assert_matches)`). Remove it together with an upstream release that builds on
stable; do not widen it.

## Releases

Linux x86_64 (`.tar.gz`, `.deb`), macOS on Apple silicon (`.dmg`, `.zip`; signed, notarized,
stapled) and Windows x64 (`-portable.zip`, `-setup.exe`; Authenticode through Azure Artifact
Signing), with `SHA256SUMS.txt` and `redrob-launcher.json` beside them, from
`.github/workflows/release.yml` ([release guide](docs/RELEASING.md)). The Windows setup speaks English
and Korean. Intel Macs are not built: `ort-sys` (via `fastembed`) publishes no ONNX Runtime binary for
`x86_64-apple-darwin`. Asset names are the ones Redrob Launcher installs.

## Privacy

- Folder selection, extraction, chunking, embedding and both searches happen on the device.
- Search never sends anything anywhere.
- Ask sends the question and up to ten passages; never filenames, paths, whole files or the index.
  Turning off **Send the matching passages to Redrob** in Settings blocks it.
- The local API listens on 127.0.0.1 only and refuses every call without the token.
- No analytics or telemetry.

[PRIVACY.md](PRIVACY.md) has the complete data flow and deletion steps.

## Licensing and attribution

Apache License 2.0 ([LICENSE](LICENSE), [NOTICE](NOTICE)). Embedded components keep their own
licences ([THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)); the query grammar follows bloop's and
the vector index is the `qdrant-edge` crate ([UPSTREAM_NOTICES.md](UPSTREAM_NOTICES.md),
[docs/upstream-sources.toml](docs/upstream-sources.toml)). Qdrant and Qdrant Edge are projects of
Qdrant Solutions GmbH; Redrob Recall is a Redrob product and not an official Qdrant distribution.

## Documentation

- [The local API](docs/API.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Release process](docs/RELEASING.md)
- [Backup and recovery](docs/RECOVERY.md)
- [Why FTS5 stayed](docs/index-engine-decision.md) and [the query grammar](docs/upstream-grammar.md)
- [Privacy](PRIVACY.md), [Security](SECURITY.md), [Changelog](CHANGELOG.md)
