# Third-Party Notices

Redrob Recall includes open-source software. Each component remains subject to its own license; no Redrob trademark or application license changes those terms.

This file covers components this product **embeds or depends on**. Source code **copied into this repository** from another project is a different obligation with a different lifetime and is recorded in `UPSTREAM_NOTICES.md`, alongside the pinned upstreams in `docs/upstream-sources.toml`.

## Principal components

| Component                         | License                      | Project                                        |
| --------------------------------- | ---------------------------- | ---------------------------------------------- |
| egui and eframe                   | MIT OR Apache-2.0            | https://github.com/emilk/egui                  |
| wgpu                              | MIT OR Apache-2.0            | https://github.com/gfx-rs/wgpu                 |
| winit                             | Apache-2.0                   | https://github.com/rust-windowing/winit        |
| rfd                               | MIT                          | https://github.com/PolyMeilex/rfd              |
| Qdrant Edge                       | Apache-2.0                   | https://github.com/qdrant/qdrant               |
| FastEmbed for Rust                | Apache-2.0                   | https://github.com/Anush008/fastembed-rs       |
| ONNX Runtime (via `ort`)          | MIT                          | https://github.com/microsoft/onnxruntime       |
| SQLite                            | Public domain                | https://sqlite.org/copyright.html              |
| rusqlite                          | MIT                          | https://github.com/rusqlite/rusqlite           |
| Tokio                             | MIT                          | https://github.com/tokio-rs/tokio              |
| Axum                              | MIT                          | https://github.com/tokio-rs/axum               |
| reqwest                           | Apache-2.0 OR MIT            | https://github.com/seanmonstar/reqwest         |
| rustls                            | Apache-2.0 OR ISC OR MIT     | https://github.com/rustls/rustls               |
| clap                              | Apache-2.0 OR MIT            | https://github.com/clap-rs/clap                |
| notify                            | CC0-1.0 OR Artistic-2.0      | https://github.com/notify-rs/notify            |
| ignore (ripgrep)                  | Unlicense OR MIT             | https://github.com/BurntSushi/ripgrep          |
| pdf-extract                       | MIT                          | https://github.com/jrmuizel/pdf-extract        |
| quick-xml                         | MIT                          | https://github.com/tafia/quick-xml             |
| zip                               | MIT                          | https://github.com/zip-rs/zip2                 |
| serde and serde_json              | MIT OR Apache-2.0            | https://github.com/serde-rs/serde              |
| image                             | MIT OR Apache-2.0            | https://github.com/image-rs/image              |

The committed `Cargo.lock` identifies the exact versions and transitive dependencies used by this build. Their license texts are available from the project links and the crate source distributions.

The Windows setup embeds Pretendard (SIL Open Font License 1.1, https://github.com/orioncactus/pretendard) when the design system delivery is present at build time.

## Qdrant notice

Qdrant and Qdrant Edge are licensed under Apache License 2.0. Copyright belongs to their respective contributors. Redrob Recall uses Qdrant Edge as an embedded vector-search component. “Qdrant” is used only to identify that third-party technology; Redrob Recall is not an official Qdrant product.

Apache License 2.0: https://www.apache.org/licenses/LICENSE-2.0

MIT License text: https://opensource.org/license/mit

ISC License text: https://opensource.org/license/isc-license-txt

SIL Open Font License 1.1: https://openfontlicense.org/
