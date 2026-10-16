// SPDX-License-Identifier: Apache-2.0
//! Redrob Recall: a local memory and embedding-index layer for the Redrob apps, with a thin window.
//!
//! Usage:
//!   redrob-recall                     the desktop window (serves the local API while open)
//!   redrob-recall serve [--port N]    the local API alone, headless, until Ctrl+C
//!   redrob-recall index <dir>         add a folder to the library and index it now, in the terminal
//!   redrob-recall search <query>      search the library from the terminal
//!   redrob-recall --version
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod account;
mod console;
mod i18n;
mod prefs;
mod services;
mod shell;
mod theme;
mod work;

use clap::{Parser, Subcommand};
use redrob_recall_core::{Library, SearchFilters, SearchRequest, default_data_dir};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "redrob-recall",
    version,
    about = "Redrob Recall: local memory and search for your own files"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the local API (127.0.0.1, bearer token) without a window, until Ctrl+C.
    Serve {
        /// Port to bind; the settings' port (default 47331) when omitted.
        #[arg(long)]
        port: Option<u16>,
    },
    /// Add a folder to the library and index it now, printing progress.
    Index {
        /// The folder to index.
        dir: PathBuf,
    },
    /// Search the library and print the results.
    Search {
        /// What to look for (quoted phrases, -excluded words and heading:word work).
        query: Vec<String>,
        /// How many results (1..50).
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "redrob_recall=info,redrob_recall_core=info,redrob_recall_api=info".into()
            }),
        )
        .compact()
        .init();
    let cli = Cli::parse();
    match cli.command {
        None => run_window(),
        Some(Command::Serve { port }) => run_serve(port),
        Some(Command::Index { dir }) => run_index(dir),
        Some(Command::Search { query, limit }) => run_search(query.join(" "), limit),
    }
}

fn open_library() -> anyhow::Result<Library> {
    Library::open(default_data_dir()?)
}

fn run_window() -> anyhow::Result<()> {
    let library = open_library()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    // The local API: what the other Redrob apps call while this window is open.
    let api_library = library.clone();
    runtime.spawn(async move {
        if let Err(error) = redrob_recall_api::server::serve(api_library).await {
            tracing::warn!(%error, "local API stopped");
        }
    });
    library.start_background_services();
    let handle = runtime.handle().clone();
    let app_library = library.clone();
    let mut options = services::native_options();
    // Nothing draws indirectly, so wgpu's indirect-call validation only costs a compute shader at
    // start-up that some drivers cannot compile (as redrob-canvas found).
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        create.instance_descriptor.flags = create
            .instance_descriptor
            .flags
            .difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL)
            .with_env();
    }
    let result = eframe::run_native(
        services::APP_NAME,
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            // Fonts once, before the first frame; the shell sets the one style in `new`.
            theme::install_fonts(&cc.egui_ctx);
            let app = shell::ShellApp::new(app_library.clone(), handle.clone(), &cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
    library.shutdown();
    runtime.shutdown_background();
    result.map_err(|error| anyhow::anyhow!("{error}"))
}

fn run_serve(port: Option<u16>) -> anyhow::Result<()> {
    let library = open_library()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    library.start_background_services();
    runtime.block_on(async move {
        let server = async {
            match port {
                Some(port) => redrob_recall_api::server::serve_on(library.clone(), port).await,
                None => redrob_recall_api::server::serve(library.clone()).await,
            }
        };
        tokio::select! {
            result = server => result,
            _ = tokio::signal::ctrl_c() => {
                library.shutdown();
                Ok(())
            }
        }
    })
}

fn run_index(dir: PathBuf) -> anyhow::Result<()> {
    let library = open_library()?;
    let path = dir.to_string_lossy().to_string();
    match library.add_folder(&path) {
        Ok(()) => println!("added {path}"),
        Err(message) if message.ends_with("is already in your library") => {
            println!("{message}; re-reading it");
            library.request_index();
        }
        Err(message) => anyhow::bail!(message),
    }
    // add_folder started (or queued) the pass on its own thread; follow it from here.
    let mut last = String::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if let Some(progress) = library.progress() {
            let line = match &progress.current_file {
                Some(file) => format!("{}/{} {}", progress.processed, progress.total, file),
                None => format!(
                    "{}/{} {}",
                    progress.processed, progress.total, progress.message
                ),
            };
            if line != last {
                println!("{line}");
                last = line;
            }
        }
        if !library.is_indexing() {
            break;
        }
    }
    let stats = library.stats()?;
    println!(
        "{} documents, {} passages, {} failed",
        stats.documents, stats.indexed_chunks, stats.failed
    );
    library.shutdown();
    Ok(())
}

fn run_search(query: String, limit: usize) -> anyhow::Result<()> {
    let library = open_library()?;
    let results = redrob_recall_core::search(
        &library,
        SearchRequest {
            query,
            limit,
            filters: SearchFilters::default(),
        },
    )?;
    if results.is_empty() {
        println!("no results");
    }
    for (index, result) in results.iter().enumerate() {
        let location = match result.page {
            Some(page) => format!("{} p.{page}", result.path),
            None => result.path.clone(),
        };
        println!("{:>2}. {:.2}  {location}", index + 1, result.score);
        let excerpt = work::plain_snippet(result);
        for line in excerpt.lines().take(3) {
            println!("      {line}");
        }
    }
    library.shutdown();
    Ok(())
}
