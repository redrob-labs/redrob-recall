// SPDX-License-Identifier: Apache-2.0
//! Where the server leaves what a client on the same machine needs, under the data directory
//! (`~/.redrob/recall`, or `REDROB_RECALL_DATA_DIR`).

use std::path::PathBuf;

/// The bearer token, one line, 0600. Created by the server on first start.
pub const TOKEN_FILE: &str = "local-api-token";
/// The base URL the server bound (`http://127.0.0.1:<port>`), one line, rewritten at every start.
pub const ADDRESS_FILE: &str = "local-api-address";
/// Every token starts with this; a file that does not is refused rather than used.
pub const TOKEN_PREFIX: &str = "rrv_local_";
/// The port the settings default to.
pub const DEFAULT_PORT: u16 = 47331;

/// `~/.redrob/recall`, or `REDROB_RECALL_DATA_DIR`. The same rule as the core crate's
/// `default_data_dir`, repeated here so a client does not need the core crate.
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("REDROB_RECALL_DATA_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".redrob").join("recall"))
}
