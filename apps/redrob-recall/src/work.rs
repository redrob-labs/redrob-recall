// SPDX-License-Identifier: Apache-2.0
//! Background work the window waits on: a search (embeds the query, so it is not done on the UI
//! thread) and an ask (the search, then the console round trip). Each job answers once through a
//! channel the screen drains every frame; a job whose answer arrives after a newer job started is
//! dropped by its generation number.

use crate::{account::Account, console};
use redrob_recall_core::{
    AskContext, AskRequest, AskResponse, Library, SearchFilters, SearchRequest, SearchResult,
    ask_context, search,
};
use std::sync::mpsc::{Receiver, Sender, channel};

pub struct SearchDone {
    pub generation: u64,
    pub result: Result<Vec<SearchResult>, String>,
}

/// What Ask produced: the answer with its sources, or the evidence alone when the model call failed
/// (the sources are still worth showing) with the error message.
pub struct AskDone {
    pub generation: u64,
    pub result: Result<AskResponse, AskFailure>,
}

pub struct AskFailure {
    pub message: String,
    pub code: String,
    pub sources: Vec<redrob_recall_core::AnswerSource>,
}

pub struct Jobs {
    search_tx: Sender<SearchDone>,
    pub search_rx: Receiver<SearchDone>,
    ask_tx: Sender<AskDone>,
    pub ask_rx: Receiver<AskDone>,
    http: reqwest::Client,
}

impl Jobs {
    pub fn new() -> Self {
        let (search_tx, search_rx) = channel();
        let (ask_tx, ask_rx) = channel();
        Self {
            search_tx,
            search_rx,
            ask_tx,
            ask_rx,
            http: reqwest::Client::builder()
                .user_agent(concat!("redrob-recall/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn search(
        &self,
        library: &Library,
        generation: u64,
        query: String,
        limit: usize,
        filters: SearchFilters,
        ctx: egui::Context,
    ) {
        let library = library.clone();
        let tx = self.search_tx.clone();
        let _ = std::thread::Builder::new()
            .name("recall-search".into())
            .spawn(move || {
                let result = search(
                    &library,
                    SearchRequest {
                        query,
                        limit,
                        filters,
                    },
                )
                .map_err(|error| error.to_string());
                let _ = tx.send(SearchDone { generation, result });
                ctx.request_repaint();
            });
    }

    /// Gather the evidence on a blocking thread, then ask the console on the runtime.
    pub fn ask(
        &self,
        library: &Library,
        runtime: &tokio::runtime::Handle,
        generation: u64,
        query: String,
        max_sources: usize,
        account: Option<Account>,
        model: String,
        ctx: egui::Context,
    ) {
        let library = library.clone();
        let tx = self.ask_tx.clone();
        let http = self.http.clone();
        runtime.spawn(async move {
            let request = AskRequest {
                query,
                source_ids: Vec::new(),
                max_sources,
            };
            let evidence_library = library.clone();
            let evidence = tokio::task::spawn_blocking(move || ask_context(&evidence_library, &request))
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.message));
            let result = match evidence {
                Err(message) => Err(AskFailure {
                    message,
                    code: "local_error".into(),
                    sources: Vec::new(),
                }),
                Ok(evidence) => match account {
                    None => Err(AskFailure {
                        message: "Sign in with Redrob Launcher to ask. The sources below were found locally.".into(),
                        code: "not_signed_in".into(),
                        sources: evidence.sources,
                    }),
                    Some(account) => complete(&http, &account, &model, evidence).await,
                },
            };
            let _ = tx.send(AskDone { generation, result });
            ctx.request_repaint();
        });
    }
}

async fn complete(
    http: &reqwest::Client,
    account: &Account,
    model: &str,
    evidence: AskContext,
) -> Result<AskResponse, AskFailure> {
    let sources = evidence.sources.clone();
    console::ask(http, account, model, evidence)
        .await
        .map_err(|error| AskFailure {
            message: error.message,
            code: error.code,
            sources,
        })
}

/// The snippet without FTS5's `<b>` markers, or the head of the chunk when there is none.
pub fn plain_snippet(result: &SearchResult) -> String {
    match &result.snippet {
        Some(snippet) => strip_bold(snippet),
        None => result.content.chars().take(280).collect(),
    }
}

pub fn strip_bold(text: &str) -> String {
    text.replace("<b>", "").replace("</b>", "")
}
