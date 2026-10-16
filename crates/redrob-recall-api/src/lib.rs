// SPDX-License-Identifier: Apache-2.0
//! Redrob Recall's loopback API: the surface the other Redrob apps call to use this machine's local
//! memory. Two halves behind two features:
//!
//! - `server` (what `redrob-recall serve` and the desktop app run): an axum router over a
//!   [`redrob_recall_core::Library`], bound to 127.0.0.1 with one bearer token. [`server::serve`].
//! - `client` (what another app depends on): [`RecallClient`], a typed reqwest client with the same
//!   JSON shapes, and [`RecallClient::discover`], which finds the running server through the files
//!   it leaves in the data directory, so no configuration is needed on the same machine.
//!
//! docs/API.md is the contract both halves implement.

#[cfg(feature = "client")]
pub mod client;
pub mod paths;
#[cfg(feature = "server")]
pub mod server;
pub mod types;

#[cfg(feature = "client")]
pub use client::{ClientError, RecallClient};
pub use types::*;
