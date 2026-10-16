// SPDX-License-Identifier: Apache-2.0
//! The Redrob Code app bridge: this window's tools, served to the shared daemon over MCP on
//! loopback through `redrob-app-bridge`, so the agent in any Redrob surface (a phone chat included)
//! can read the index, add and remove folders, start a pass, search and point the window at a hit.
//!
//! The crate runs every tool handler on a thread of its own. What only needs the [`Library`]
//! (status, the folder list, a search -- the library is thread-safe and the search embeds the
//! query, which must not run on the UI thread anyway) is answered right there. What touches the
//! window's state -- adding or removing a folder with its message in the dock, starting a pass,
//! showing results, selecting a hit -- is a request/reply round trip: the handler queues a
//! [`BridgeRequest`], asks for a repaint and blocks on the reply the shell sends from `update()`
//! after running the same functions the UI uses. A request the window does not answer within
//! [`REPLY_TIMEOUT`] fails with a sentence the agent can read.
//!
//! The daemon is optional: when it cannot be reached the failure is logged and the window runs
//! without the bridge.

use redrob_app_bridge::{AppBridge, AppBridgeConfig, Tool};
use redrob_recall_core::{IndexStatus, Library, SearchFilters, SearchRequest, SearchResult};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The daemon lists the tools under `app:recall`.
const APP_NAME: &str = "recall";
/// How long a handler waits for the window to answer a queued request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// Search answers at most this many hits; the default when `limit` is omitted.
const MAX_HITS: usize = 50;
const DEFAULT_HITS: usize = 10;
/// A snippet in a search answer is cut here.
const SNIPPET_CHARS: usize = 300;

pub type Reply = Result<String, String>;

/// What a handler asks the window to do on the UI thread.
pub enum Request {
    /// Add a folder to the library (what Add folder does), with its message in the dock.
    AddSource { path: String },
    /// Remove a folder from the library and drop what was indexed under it.
    RemoveSource { path: String },
    /// Start a pass over the library, or queue one behind the running pass.
    Reindex,
    /// Show these results in the Search view, as if typed into the box, and select the first.
    ShowResults {
        query: String,
        results: Vec<SearchResult>,
    },
    /// Select the result with this chunk id in the Search view; `open` also opens its file.
    OpenHit { id: u64, open: bool },
}

/// One queued request and where its answer goes.
pub struct BridgeRequest {
    pub request: Request,
    pub reply: Sender<Reply>,
}

/// What every handler shares: the queue to the window, the library, the pass counter.
struct Shared {
    tx: Mutex<Sender<BridgeRequest>>,
    ctx: egui::Context,
    library: Library,
    /// Passes started through `reindex`, so the agent gets a job id to ask about.
    passes: AtomicU64,
}

impl Shared {
    /// Queue `request`, wake the window and wait for its answer.
    fn ask(&self, request: Request) -> Reply {
        let (reply_tx, reply_rx) = channel();
        let request = BridgeRequest {
            request,
            reply: reply_tx,
        };
        self.tx
            .lock()
            .map_err(|_| "the app bridge queue is poisoned".to_string())?
            .send(request)
            .map_err(|_| "the Redrob Recall window is closed".to_string())?;
        self.ctx.request_repaint();
        match reply_rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(reply) => reply,
            Err(_) => Err("the Redrob Recall window did not answer in time".to_string()),
        }
    }
}

/// The live bridge and the queue the shell drains once per frame.
pub struct Bridge {
    bridge: Option<AppBridge>,
    rx: Receiver<BridgeRequest>,
}

impl Bridge {
    /// Serve the tools and register with the daemon. A daemon that cannot be reached leaves the
    /// bridge empty: the queue still exists, nothing ever lands in it.
    pub fn start(library: Library, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            tx: Mutex::new(tx),
            ctx: ctx.clone(),
            library,
            passes: AtomicU64::new(0),
        });
        let config = AppBridgeConfig {
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..AppBridgeConfig::new(APP_NAME, tools(&shared))
        };
        let bridge = match AppBridge::start(config) {
            Ok(bridge) => {
                tracing::info!(id = %bridge.id(), url = bridge.mcp_url(), "app bridge up");
                Some(bridge)
            }
            Err(error) => {
                tracing::warn!(%error, "app bridge not started; the window runs without it");
                None
            }
        };
        Self { bridge, rx }
    }

    /// The requests that arrived since the last frame, oldest first.
    pub fn drain(&self) -> Vec<BridgeRequest> {
        let mut requests = Vec::new();
        while let Ok(request) = self.rx.try_recv() {
            requests.push(request);
        }
        requests
    }

    /// Unregister from the daemon and stop serving. Dropping the bridge does the same.
    pub fn stop(&mut self) {
        if let Some(bridge) = self.bridge.take() {
            bridge.stop();
        }
    }
}

// ---- tools ------------------------------------------------------------------------------------

fn tools(shared: &Arc<Shared>) -> Vec<Tool> {
    let status = {
        let shared = shared.clone();
        Tool::new(
            "status",
            "Report the index: its data directory, document and passage counts, the index state, when it last finished, and whether the local API is answering.",
            json!({ "type": "object", "properties": {} }),
            move |_| status(&shared.library),
        )
    };
    let list_sources = {
        let shared = shared.clone();
        Tool::new(
            "list_sources",
            "List the folders in the library with how many files each one has indexed and failed, and whether it still exists.",
            json!({ "type": "object", "properties": {} }),
            move |_| list_sources(&shared.library),
        )
    };
    let add_source = {
        let shared = shared.clone();
        Tool::new(
            "add_source",
            "Add a folder to the library and start reading it; the path must be an existing, readable folder.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Absolute path of the folder to index." } },
                "required": ["path"]
            }),
            move |args| {
                let path = required_str(&args, "path")?;
                shared.ask(Request::AddSource { path })
            },
        )
    };
    let remove_source = {
        let shared = shared.clone();
        Tool::new(
            "remove_source",
            "Remove a folder from the library and forget what was indexed under it; use the exact path list_sources shows. Refused while a pass is running.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "The folder's path as list_sources shows it." } },
                "required": ["path"]
            }),
            move |args| {
                let path = required_str(&args, "path")?;
                shared.ask(Request::RemoveSource { path })
            },
        )
    };
    let reindex = {
        let shared = shared.clone();
        Tool::new(
            "reindex",
            "Start a pass that re-reads the library (every folder; a pass always covers the whole library) or queue one behind the running pass; answers a job id for index_status.",
            json!({
                "type": "object",
                "properties": { "source": { "type": "string", "description": "Optional: a folder the pass must cover; it is checked to be in the library." } }
            }),
            move |args| reindex(&shared, &args),
        )
    };
    let index_status = {
        let shared = shared.clone();
        Tool::new(
            "index_status",
            "Report the indexing pass: idle, indexing (with files processed of total and the current file) or paused, and whether the job from reindex has finished.",
            json!({
                "type": "object",
                "properties": { "job_id": { "type": "integer", "description": "Optional: the job id reindex answered." } }
            }),
            move |args| index_status(&shared, &args),
        )
    };
    let search = {
        let shared = shared.clone();
        Tool::new(
            "search",
            "Search the library and show the hits in the window; answers ranked hits with their id, score, source path and a short snippet. Use open_hit with an id to select one.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for; quoted phrases, -excluded words and heading:word work." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_HITS, "description": "How many hits (default 10)." }
                },
                "required": ["query"]
            }),
            move |args| search(&shared, &args),
        )
    };
    let open_hit = {
        let shared = shared.clone();
        Tool::new(
            "open_hit",
            "Select a hit from the last search in the window and scroll to it; with open true the file is also opened in its own app.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "The hit's id from search." },
                    "open": { "type": "boolean", "description": "Also open the file with its app (default false)." }
                },
                "required": ["id"]
            }),
            move |args| {
                let id = args.get("id").and_then(Value::as_u64).ok_or_else(|| {
                    "id is required and must be a non-negative integer".to_string()
                })?;
                let open = args.get("open").and_then(Value::as_bool).unwrap_or(false);
                shared.ask(Request::OpenHit { id, open })
            },
        )
    };
    vec![
        status,
        list_sources,
        add_source,
        remove_source,
        reindex,
        index_status,
        search,
        open_hit,
    ]
}

fn required_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{key} is required"))
}

fn status_word(status: &IndexStatus) -> &'static str {
    match status {
        IndexStatus::Idle => "idle",
        IndexStatus::Scanning => "scanning",
        IndexStatus::Indexing => "indexing",
        IndexStatus::Paused => "paused",
        IndexStatus::Error => "error",
    }
}

fn status(library: &Library) -> Reply {
    let snapshot = library.snapshot().map_err(|error| error.to_string())?;
    let stats = &snapshot.stats;
    let api = local_api_state(library);
    let mut lines = vec![
        format!("index: {}", snapshot.data_directory),
        format!("version: {}", snapshot.version),
        format!("folders: {}", snapshot.folders.len()),
        format!(
            "documents: {} ({} passages, {} pending, {} failed)",
            stats.documents, stats.indexed_chunks, stats.pending, stats.failed
        ),
        format!("state: {}", status_word(&stats.status)),
        format!(
            "last indexed: {}",
            stats.last_indexed_at.as_deref().unwrap_or("never")
        ),
        format!("local API: {api}"),
    ];
    if let Some(file) = &stats.current_file {
        lines.push(format!("current file: {file}"));
    }
    Ok(lines.join("\n"))
}

/// Whether the loopback API this window serves answers on the address it wrote down.
fn local_api_state(library: &Library) -> String {
    if !library.settings().local_api_enabled {
        return "off (disabled in settings)".to_string();
    }
    let address = std::fs::read_to_string(
        library
            .data_dir()
            .join(redrob_recall_api::paths::ADDRESS_FILE),
    )
    .map(|a| a.trim().to_string())
    .unwrap_or_default();
    let Some(host_port) = address.strip_prefix("http://") else {
        return "not up (no address written)".to_string();
    };
    let reachable = host_port
        .parse::<std::net::SocketAddr>()
        .ok()
        .is_some_and(|addr| {
            std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
        });
    if reachable {
        format!("up at {address}")
    } else {
        format!("not answering at {address}")
    }
}

fn list_sources(library: &Library) -> Reply {
    let folders = library.folders().map_err(|error| error.to_string())?;
    if folders.is_empty() {
        return Ok("no folders in the library".to_string());
    }
    Ok(folders
        .iter()
        .map(|folder| {
            let presence = if folder.present { "" } else { ", missing" };
            format!(
                "{} ({} files, {} failed{presence})",
                folder.path, folder.files, folder.failed
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn reindex(shared: &Shared, args: &Value) -> Reply {
    if let Some(source) = args.get("source").and_then(Value::as_str).map(str::trim)
        && !source.is_empty()
    {
        let in_library = shared
            .library
            .settings()
            .library_paths
            .iter()
            .any(|path| path == source);
        if !in_library {
            return Err(format!(
                "{source} is not in the library; list_sources shows the folders, add_source adds one"
            ));
        }
    }
    let answer = shared.ask(Request::Reindex)?;
    let job = shared.passes.fetch_add(1, Ordering::Relaxed) + 1;
    Ok(format!(
        "job {job}: {answer} (the pass covers every folder)"
    ))
}

fn index_status(shared: &Shared, args: &Value) -> Reply {
    let library = &shared.library;
    let state = library.status();
    let mut lines = Vec::new();
    if let Some(job) = args.get("job_id").and_then(Value::as_u64) {
        let latest = shared.passes.load(Ordering::Relaxed);
        let verdict = if job == 0 || job > latest {
            "unknown job id (reindex answered none like it this session)"
        } else if state == IndexStatus::Idle {
            "finished"
        } else if job < latest {
            "finished (a later pass is running)"
        } else {
            "running"
        };
        lines.push(format!("job {job}: {verdict}"));
    }
    lines.push(format!("state: {}", status_word(&state)));
    if let Some(progress) = library.progress() {
        lines.push(format!(
            "progress: {}/{} {}",
            progress.processed, progress.total, progress.message
        ));
        if let Some(file) = progress.current_file.filter(|_| library.is_indexing()) {
            lines.push(format!("current file: {file}"));
        }
    }
    if let Ok(stats) = library.stats() {
        lines.push(format!(
            "documents: {} ({} passages, {} pending, {} failed)",
            stats.documents, stats.indexed_chunks, stats.pending, stats.failed
        ));
        lines.push(format!(
            "last indexed: {}",
            stats.last_indexed_at.as_deref().unwrap_or("never")
        ));
    }
    Ok(lines.join("\n"))
}

/// The search runs here, on the handler's thread (it embeds the query); the results then go to
/// the window, which shows them as if typed into the box.
fn search(shared: &Shared, args: &Value) -> Reply {
    let query = required_str(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(DEFAULT_HITS, |n| (n as usize).clamp(1, MAX_HITS));
    let results = redrob_recall_core::search(
        &shared.library,
        SearchRequest {
            query: query.clone(),
            limit,
            filters: SearchFilters::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    let answer = if results.is_empty() {
        format!("no results for {query:?}")
    } else {
        results
            .iter()
            .enumerate()
            .map(|(index, result)| {
                let location = match result.page {
                    Some(page) => format!("{} p.{page}", result.path),
                    None => result.path.clone(),
                };
                let snippet: String = crate::work::plain_snippet(result)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(SNIPPET_CHARS)
                    .collect();
                format!(
                    "{}. id {} score {:.2} {location}\n   {snippet}",
                    index + 1,
                    result.chunk_id,
                    result.score
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    shared.ask(Request::ShowResults { query, results })?;
    Ok(answer)
}
