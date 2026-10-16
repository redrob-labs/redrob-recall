// SPDX-License-Identifier: Apache-2.0
//! Redrob Recall's memory layer: the part every Redrob app can reuse.
//!
//! One [`Library`] owns a data directory (`~/.redrob/recall` by default) holding the SQLite metadata
//! and keyword index (`storage`), the Qdrant Edge vector index, the local multilingual embedding model
//! (`embedding`) and the settings. The `indexer` walks the chosen folders, extracts text (PDF, DOCX,
//! HTML/XML, plain text), chunks it, embeds it and keeps the index current through a file watcher;
//! `search` fuses the vector, keyword and document-name arms; `ask` turns a question into numbered
//! evidence for a model to answer from.
//!
//! Nothing here knows about a window or a network: the desktop shell (`apps/redrob-recall`) and the
//! loopback API (`crates/redrob-recall-api`) sit on top. The crate is single-process and
//! synchronous; long passes run on their own threads and report through [`Library::snapshot`].
//!
//! Parts of `storage` and `query` are ported from bloop (Apache-2.0) and the vector index is the
//! `qdrant-edge` crate (Apache-2.0, Qdrant Solutions GmbH): see UPSTREAM_NOTICES.md and NOTICE.

pub mod ask;
pub mod embedding;
pub mod indexer;
pub mod library;
pub mod models;
pub mod query;
pub mod search;
pub mod storage;

pub use ask::{AskContext, AskError, ask_context};
pub use library::{Library, default_data_dir};
pub use models::*;
pub use search::search;
