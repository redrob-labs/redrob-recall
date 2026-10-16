// SPDX-License-Identifier: Apache-2.0
//! The Redrob Recall window: a rail of four screens over the one [`Library`].
//!
//! - Library: the folders, the index status and progress, pause / check now, add and remove.
//! - Search: hybrid results with the keyword snippet, open the file or reveal it.
//! - Ask: a question, the console's answer with numbered citations, the sources beneath.
//! - Settings: the embedding model, the local API, the shared account, maintenance.
//!
//! Nothing heavy runs on the UI thread: the indexer runs on its own threads inside the core crate,
//! searches and asks go through [`work::Jobs`], and the snapshot is re-read only when the library
//! says it changed (or once a second while a pass runs).

use crate::{account, services, work};
use redrob_recall_core::{
    AnswerSource, AppSettings, AskResponse, IndexProgress, IndexStatus, Library, LibrarySnapshot,
    SearchFilters, SearchResult,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

/// Redrob Blue, the one accent (20-color.md: "blue acts").
pub const REDROB_BLUE: egui::Color32 = egui::Color32::from_rgb(0x2B, 0x52, 0xFF);
/// The lockup in the top bar: Primary White on the dark chrome (10-logo.md), 20 px tall.
const LOCKUP_WHITE: egui::ImageSource<'static> =
    egui::include_image!("../../../resources/brand/redrob-recall-lockup-white.png");
const TOP_BAR_HEIGHT: f32 = 40.0;
const RAIL_WIDTH: f32 = 150.0;
const STATUS_SECS: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Library,
    Search,
    Ask,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 4] = [Screen::Library, Screen::Search, Screen::Ask, Screen::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Library => "Library",
            Screen::Search => "Search",
            Screen::Ask => "Ask",
            Screen::Settings => "Settings",
        }
    }
}

/// A line at the bottom that fades after a few seconds.
struct Status {
    text: String,
    error: bool,
    since: Instant,
}

struct SearchScreen {
    query: String,
    extension_filter: String,
    results: Vec<SearchResult>,
    searching: bool,
    generation: u64,
    error: Option<String>,
    searched_for: String,
}

struct AskScreen {
    question: String,
    asking: bool,
    generation: u64,
    answer: Option<AskResponse>,
    failure: Option<work::AskFailure>,
    asked: String,
}

pub struct ShellApp {
    library: Library,
    runtime: tokio::runtime::Handle,
    jobs: work::Jobs,
    screen: Screen,
    snapshot: Option<LibrarySnapshot>,
    progress: Option<IndexProgress>,
    dirty: Arc<AtomicBool>,
    last_refresh: Instant,
    settings_draft: AppSettings,
    settings_dirty: bool,
    account: Option<account::Account>,
    account_checked: Instant,
    search: SearchScreen,
    ask: AskScreen,
    status: Option<Status>,
    about_open: bool,
}

impl ShellApp {
    pub fn new(library: Library, runtime: tokio::runtime::Handle, ctx: &egui::Context) -> Self {
        let dirty = Arc::new(AtomicBool::new(true));
        {
            let dirty = dirty.clone();
            let ctx = ctx.clone();
            library.set_on_change(Arc::new(move || {
                dirty.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            }));
        }
        style(ctx);
        let settings_draft = library.settings();
        Self {
            library,
            runtime,
            jobs: work::Jobs::new(),
            screen: Screen::default(),
            snapshot: None,
            progress: None,
            dirty,
            last_refresh: Instant::now() - Duration::from_secs(10),
            settings_draft,
            settings_dirty: false,
            account: account::current(),
            account_checked: Instant::now(),
            search: SearchScreen {
                query: String::new(),
                extension_filter: String::new(),
                results: Vec::new(),
                searching: false,
                generation: 0,
                error: None,
                searched_for: String::new(),
            },
            ask: AskScreen {
                question: String::new(),
                asking: false,
                generation: 0,
                answer: None,
                failure: None,
                asked: String::new(),
            },
            status: None,
            about_open: false,
        }
    }

    fn refresh(&mut self) {
        let indexing = self.library.is_indexing();
        let due = self.dirty.swap(false, Ordering::Relaxed)
            || (indexing && self.last_refresh.elapsed() > Duration::from_secs(1));
        if !due {
            return;
        }
        self.last_refresh = Instant::now();
        match self.library.snapshot() {
            Ok(snapshot) => self.snapshot = Some(snapshot),
            Err(error) => self.say_error(format!("The library could not be read: {error}")),
        }
        self.progress = self.library.progress();
        if !self.settings_dirty {
            self.settings_draft = self.library.settings();
        }
    }

    fn drain_jobs(&mut self) {
        while let Ok(done) = self.jobs.search_rx.try_recv() {
            if done.generation != self.search.generation {
                continue;
            }
            self.search.searching = false;
            match done.result {
                Ok(results) => {
                    self.search.results = results;
                    self.search.error = None;
                }
                Err(error) => {
                    self.search.results.clear();
                    self.search.error = Some(error);
                }
            }
        }
        while let Ok(done) = self.jobs.ask_rx.try_recv() {
            if done.generation != self.ask.generation {
                continue;
            }
            self.ask.asking = false;
            match done.result {
                Ok(answer) => {
                    self.ask.answer = Some(answer);
                    self.ask.failure = None;
                }
                Err(failure) => {
                    self.ask.answer = None;
                    self.ask.failure = Some(failure);
                }
            }
        }
    }

    fn say(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: false,
            since: Instant::now(),
        });
    }

    fn say_error(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: true,
            since: Instant::now(),
        });
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.search.generation += 1;
        self.search.searching = true;
        self.search.error = None;
        self.search.searched_for = query.clone();
        let filters = SearchFilters {
            extensions: self
                .search
                .extension_filter
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
            path_prefix: None,
        };
        self.jobs.search(
            &self.library,
            self.search.generation,
            query,
            24,
            filters,
            ctx.clone(),
        );
    }

    fn start_ask(&mut self, ctx: &egui::Context) {
        let question = self.ask.question.trim().to_string();
        if question.is_empty() {
            return;
        }
        self.ask.generation += 1;
        self.ask.asking = true;
        self.ask.answer = None;
        self.ask.failure = None;
        self.ask.asked = question.clone();
        self.account = account::current();
        let model = self.library.settings().answer_model;
        self.jobs.ask(
            &self.library,
            &self.runtime,
            self.ask.generation,
            question,
            6,
            self.account.clone(),
            model,
            ctx.clone(),
        );
    }

    fn add_folder_dialog(&mut self) {
        let Some(folder) = rfd::FileDialog::new()
            .set_title("Add a folder to your library")
            .pick_folder()
        else {
            return;
        };
        match self.library.add_folder(&folder.to_string_lossy()) {
            Ok(()) => self.say(format!("Added {}", folder.display())),
            Err(message) => self.say_error(message),
        }
    }

    fn open_source(&mut self, path: &str) {
        match self
            .library
            .validated_source(path)
            .and_then(|source| services::open_path(&source))
        {
            Ok(()) => {}
            Err(message) => self.say_error(message),
        }
    }

    fn reveal_source(&mut self, path: &str) {
        match self
            .library
            .validated_source(path)
            .and_then(|source| services::reveal_path(&source))
        {
            Ok(()) => {}
            Err(message) => self.say_error(message),
        }
    }

    // ---- screens -------------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.add(
                egui::Image::new(LOCKUP_WHITE.clone())
                    .max_height(20.0)
                    .maintain_aspect_ratio(true),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if ui.small_button("About").clicked() {
                    self.about_open = true;
                }
                if let Some(snapshot) = &self.snapshot {
                    let stats = &snapshot.stats;
                    let text = match stats.status {
                        IndexStatus::Indexing | IndexStatus::Scanning => match &stats.current_file {
                            Some(file) => format!("Indexing {file}"),
                            None => "Indexing".to_string(),
                        },
                        IndexStatus::Paused => "Indexing paused".to_string(),
                        _ => format!(
                            "{} documents, {} passages",
                            stats.documents, stats.indexed_chunks
                        ),
                    };
                    ui.weak(text);
                }
            });
        });
    }

    fn rail(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        for screen in Screen::ALL {
            let selected = self.screen == screen;
            let response = ui.add_sized(
                [RAIL_WIDTH - 16.0, 30.0],
                egui::Button::selectable(selected, egui::RichText::new(screen.label()).size(15.0)),
            );
            if response.clicked() {
                self.screen = screen;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(8.0);
            ui.weak(format!("v{}", env!("CARGO_PKG_VERSION")));
            let signed_in = self.account.is_some();
            let label = if signed_in { "Signed in" } else { "Not signed in" };
            ui.weak(label);
        });
    }

    fn library_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Library");
        ui.label("The folders Recall reads, indexes locally and keeps current.");
        ui.add_space(8.0);
        let Some(snapshot) = self.snapshot.clone() else {
            ui.spinner();
            return;
        };
        let stats = &snapshot.stats;

        // Progress and controls.
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (label, color) = match stats.status {
                    IndexStatus::Idle => ("Up to date", ui.visuals().weak_text_color()),
                    IndexStatus::Scanning => ("Scanning", REDROB_BLUE),
                    IndexStatus::Indexing => ("Indexing", REDROB_BLUE),
                    IndexStatus::Paused => ("Paused", ui.visuals().warn_fg_color),
                    IndexStatus::Error => ("Error", ui.visuals().error_fg_color),
                };
                ui.label(egui::RichText::new(label).strong().color(color));
                ui.separator();
                ui.label(format!(
                    "{} documents, {} passages, {} failed, {}",
                    stats.documents,
                    stats.indexed_chunks,
                    stats.failed,
                    human_bytes(stats.total_bytes)
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match stats.status {
                        IndexStatus::Indexing | IndexStatus::Scanning => {
                            if ui.button("Pause").clicked() {
                                self.library.pause_indexing();
                            }
                        }
                        IndexStatus::Paused => {
                            if ui.button("Resume").clicked() {
                                self.library.resume_indexing();
                            }
                        }
                        _ => {
                            if ui.button("Check now").clicked() {
                                let started = self.library.request_index();
                                self.say(if started {
                                    "Re-reading your library"
                                } else {
                                    "Queued behind the running pass"
                                });
                            }
                        }
                    }
                    if ui.button("Add folder").clicked() {
                        self.add_folder_dialog();
                    }
                });
            });
            if let Some(progress) = &self.progress {
                if matches!(
                    stats.status,
                    IndexStatus::Indexing | IndexStatus::Scanning | IndexStatus::Paused
                ) {
                    let fraction = if progress.total == 0 {
                        0.0
                    } else {
                        progress.processed as f32 / progress.total as f32
                    };
                    let text = match &progress.current_file {
                        Some(file) => format!("{} / {}  {file}", progress.processed, progress.total),
                        None => progress.message.clone(),
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(text)
                            .fill(REDROB_BLUE)
                            .animate(stats.status != IndexStatus::Paused),
                    );
                }
            }
            if let Some(last) = &stats.last_indexed_at {
                ui.weak(format!("Last indexed {}", last.replace('T', " ").chars().take(19).collect::<String>()));
            }
        });
        ui.add_space(8.0);

        if snapshot.folders.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No folders yet").size(18.0));
                ui.label("Add a folder and Recall indexes it on this machine: nothing leaves it.");
                ui.add_space(8.0);
                if ui.button("Add folder").clicked() {
                    self.add_folder_dialog();
                }
            });
            return;
        }

        let mut remove: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for folder in &snapshot.folders {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&folder.path).strong());
                            let mut line = format!("{} files indexed", folder.files);
                            if folder.failed > 0 {
                                line.push_str(&format!(", {} could not be read", folder.failed));
                            }
                            if !folder.present {
                                line.push_str(" -- folder is missing; its records are kept until it returns");
                            }
                            ui.weak(line);
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let busy = matches!(
                                stats.status,
                                IndexStatus::Indexing | IndexStatus::Scanning
                            );
                            if ui
                                .add_enabled(!busy, egui::Button::new("Remove"))
                                .on_disabled_hover_text("Wait for indexing to finish")
                                .clicked()
                            {
                                remove = Some(folder.path.clone());
                            }
                            if folder.present && ui.button("Reveal").clicked() {
                                if let Err(message) = services::open_path(std::path::Path::new(&folder.path)) {
                                    self.say_error(message);
                                }
                            }
                        });
                    });
                });
            }
        });
        if let Some(path) = remove {
            match self.library.remove_folder(&path) {
                Ok(()) => self.say(format!("Removed {path}")),
                Err(message) => self.say_error(message),
            }
        }
    }

    fn search_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Search");
        ui.horizontal(|ui| {
            let response = ui.add_sized(
                [ui.available_width() - 260.0, 28.0],
                egui::TextEdit::singleline(&mut self.search.query)
                    .hint_text("Search your files: words, \"a phrase\", -excluded, heading:word"),
            );
            let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_sized(
                [120.0, 28.0],
                egui::TextEdit::singleline(&mut self.search.extension_filter).hint_text("pdf, md"),
            )
            .on_hover_text("Only these file types (comma separated)");
            if ui.add_sized([100.0, 28.0], egui::Button::new("Search")).clicked() || submitted {
                self.start_search(ctx);
            }
        });
        ui.add_space(6.0);
        if self.search.searching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(format!("Searching for {}", self.search.searched_for));
            });
        } else if let Some(error) = &self.search.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else if !self.search.searched_for.is_empty() {
            ui.weak(format!(
                "{} results for {}",
                self.search.results.len(),
                self.search.searched_for
            ));
        }
        ui.add_space(4.0);
        let results = self.search.results.clone();
        let mut open: Option<String> = None;
        let mut reveal: Option<String> = None;
        let mut ask_about: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for result in &results {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&result.name).strong().size(15.0));
                        if let Some(page) = result.page {
                            ui.weak(format!("p. {page}"));
                        }
                        if let Some(heading) = &result.heading {
                            ui.weak(heading);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Reveal").clicked() {
                                reveal = Some(result.path.clone());
                            }
                            if ui.small_button("Open").clicked() {
                                open = Some(result.path.clone());
                            }
                            if ui.small_button("Ask about this").clicked() {
                                ask_about = Some(self.search.searched_for.clone());
                            }
                            ui.weak(format!("{:.0}%", result.score * 100.0));
                        });
                    });
                    ui.weak(&result.path);
                    snippet_label(ui, result);
                });
                ui.add_space(4.0);
            }
        });
        if let Some(path) = open {
            self.open_source(&path);
        }
        if let Some(path) = reveal {
            self.reveal_source(&path);
        }
        if let Some(query) = ask_about {
            self.ask.question = query;
            self.screen = Screen::Ask;
        }
    }

    fn ask_screen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Ask");
        match &self.account {
            Some(account) => {
                ui.weak(format!(
                    "Answers come from the Redrob console as {}; only the matching passages are sent.",
                    account.label()
                ));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label("Sign in with Redrob Launcher to ask; search works without it.");
                    if ui.link("Get Redrob Launcher").clicked() {
                        services::open_url(services::LAUNCHER_RELEASES);
                    }
                });
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let response = ui.add_sized(
                [ui.available_width() - 110.0, 28.0],
                egui::TextEdit::singleline(&mut self.ask.question)
                    .hint_text("Ask a question about your files"),
            );
            let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add_enabled_ui(!self.ask.asking, |ui| ui.add_sized([100.0, 28.0], egui::Button::new("Ask")))
                .inner
                .clicked()
                || (submitted && !self.ask.asking)
            {
                self.start_ask(ctx);
            }
        });
        ui.add_space(8.0);
        let mut open: Option<String> = None;
        let mut reveal: Option<String> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.ask.asking {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.weak("Reading your files and asking Redrob");
                });
            }
            if let Some(answer) = &self.ask.answer {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(&self.ask.asked).strong());
                    ui.add_space(4.0);
                    ui.label(&answer.answer);
                    ui.add_space(4.0);
                    ui.weak(format!(
                        "{} in {:.1}s{}",
                        answer.model,
                        answer.latency_ms as f32 / 1000.0,
                        match (answer.input_tokens, answer.output_tokens) {
                            (Some(i), Some(o)) => format!(", {i} in / {o} out tokens"),
                            _ => String::new(),
                        }
                    ));
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Sources").strong());
                sources_list(ui, &answer.sources, &mut open, &mut reveal);
            }
            if let Some(failure) = &self.ask.failure {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(ui.visuals().error_fg_color, &failure.message);
                    ui.weak(format!("({})", failure.code));
                    if failure.code == "not_signed_in" && ui.link("Get Redrob Launcher").clicked() {
                        services::open_url(services::LAUNCHER_RELEASES);
                    }
                });
                if !failure.sources.is_empty() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Sources found locally").strong());
                    sources_list(ui, &failure.sources, &mut open, &mut reveal);
                }
            }
        });
        if let Some(path) = open {
            self.open_source(&path);
        }
        if let Some(path) = reveal {
            self.reveal_source(&path);
        }
    }

    fn settings_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Account.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Redrob account").strong());
                match &self.account {
                    Some(account) => {
                        ui.label(format!("Signed in as {}", account.label()));
                        ui.weak(format!("Console: {}", account.api_base_url()));
                        if account.source == account::Source::Shared {
                            ui.weak("Sign out in Redrob Launcher; this app never writes the shared file.");
                        }
                    }
                    None => {
                        ui.label("Not signed in. Sign in once in Redrob Launcher and every Redrob app on this computer, Recall included, uses that account.");
                        ui.horizontal(|ui| {
                            if ui.button("Open Redrob Launcher").clicked() {
                                services::open_url(services::LAUNCHER_RELEASES);
                            }
                            if ui.button("Check again").clicked() {
                                self.account = account::current();
                            }
                        });
                    }
                }
                if let Some(path) = account::shared_account_path() {
                    ui.weak(format!("Shared account file: {}", path.display()));
                }
            });
            ui.add_space(8.0);

            // Ask.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Ask").strong());
                ui.horizontal(|ui| {
                    ui.label("Answer model");
                    if ui.text_edit_singleline(&mut self.settings_draft.answer_model).changed() {
                        self.settings_dirty = true;
                    }
                    ui.weak("auto lets the console choose");
                });
                if ui
                    .checkbox(
                        &mut self.settings_draft.send_context_to_redrob,
                        "Send the matching passages to Redrob when asking",
                    )
                    .changed()
                {
                    self.settings_dirty = true;
                }
            });
            ui.add_space(8.0);

            // Index.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Index").strong());
                ui.label("Embedding model: multilingual-e5-small (fastembed), downloaded once into the data folder and run on this machine.");
                ui.horizontal(|ui| {
                    ui.label("File types");
                    let mut text = self.settings_draft.include_extensions.join(", ");
                    if ui.text_edit_singleline(&mut text).changed() {
                        self.settings_draft.include_extensions = text
                            .split(|c: char| c == ',' || c.is_whitespace())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                            .collect();
                        self.settings_dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Skip folders named");
                    let mut text = self.settings_draft.excluded_paths.join(", ");
                    if ui.text_edit_singleline(&mut text).changed() {
                        self.settings_draft.excluded_paths = text
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        self.settings_dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Largest file (MB)");
                    if ui
                        .add(egui::DragValue::new(&mut self.settings_draft.max_file_size_mb).range(1..=250))
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                    ui.label("Passage size");
                    if ui
                        .add(egui::DragValue::new(&mut self.settings_draft.chunk_size).range(256..=8192))
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                    ui.label("Overlap");
                    if ui
                        .add(egui::DragValue::new(&mut self.settings_draft.chunk_overlap).range(0..=2048))
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                });
            });
            ui.add_space(8.0);

            // Local API.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Local API").strong());
                ui.label("What the other Redrob apps call to search this library: 127.0.0.1 only, one bearer token (docs/API.md).");
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut self.settings_draft.local_api_enabled, "Serve the local API")
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                    ui.label("Port");
                    if ui
                        .add(egui::DragValue::new(&mut self.settings_draft.local_api_port).range(1024..=65535))
                        .changed()
                    {
                        self.settings_dirty = true;
                    }
                });
                if let Some(snapshot) = &self.snapshot {
                    ui.weak(format!(
                        "Token: {}/local-api-token (never shown here). Changes to the API take effect at the next start.",
                        snapshot.data_directory
                    ));
                }
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.settings_dirty, egui::Button::new("Save"))
                    .clicked()
                {
                    match self.library.update_settings(self.settings_draft.clone()) {
                        Ok(()) => {
                            self.settings_dirty = false;
                            self.say("Settings saved");
                        }
                        Err(error) => self.say_error(error.to_string()),
                    }
                }
                if ui
                    .add_enabled(self.settings_dirty, egui::Button::new("Discard"))
                    .clicked()
                {
                    self.settings_draft = self.library.settings();
                    self.settings_dirty = false;
                }
            });
            ui.add_space(12.0);

            // Maintenance.
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Maintenance").strong());
                if let Some(snapshot) = &self.snapshot {
                    ui.weak(format!("Data folder: {}", snapshot.data_directory));
                }
                ui.horizontal(|ui| {
                    if ui.button("Check health").clicked() {
                        match self.library.check_health() {
                            Ok(message) => self.say(message),
                            Err(message) => self.say_error(message),
                        }
                    }
                    if ui.button("Back up metadata").clicked() {
                        match self.library.create_backup() {
                            Ok(path) => self.say(format!("Backup written to {}", path.display())),
                            Err(message) => self.say_error(message),
                        }
                    }
                    if ui
                        .button(egui::RichText::new("Clear the index").color(ui.visuals().error_fg_color))
                        .on_hover_text("Forgets every indexed document and vector; the folders stay and are re-read on the next check")
                        .clicked()
                    {
                        match self.library.clear() {
                            Ok(()) => self.say("Index cleared"),
                            Err(message) => self.say_error(message),
                        }
                    }
                });
            });
        });
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        let Some(status) = &self.status else {
            return;
        };
        let age = status.since.elapsed().as_secs_f32();
        if age > STATUS_SECS {
            self.status = None;
            return;
        }
        let color = if status.error {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().text_color()
        };
        ui.colored_label(color, &status.text);
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f32(STATUS_SECS - age));
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = self.about_open;
        egui::Window::new("About Redrob Recall")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add(egui::Image::new(LOCKUP_WHITE.clone()).max_height(28.0));
                ui.add_space(6.0);
                ui.label(format!("Redrob Recall {}", env!("CARGO_PKG_VERSION")));
                ui.label("A local memory and embedding-index layer for the Redrob apps, with a thin window.");
                ui.weak("Apache-2.0. Third-party notices: NOTICE, THIRD_PARTY_NOTICES.md, UPSTREAM_NOTICES.md.");
                if ui.link(services::REPOSITORY).clicked() {
                    services::open_url(services::REPOSITORY);
                }
            });
        self.about_open = open;
    }
}

impl eframe::App for ShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh();
        self.drain_jobs();
        if self.account_checked.elapsed() > Duration::from_secs(5) {
            self.account = account::current();
            self.account_checked = Instant::now();
        }
        let ctx = ui.ctx().clone();

        egui::Panel::top("top-bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(0x0A, 0x0B, 0x0C)))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status-line")
            .exact_size(26.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    self.status_line(ui);
                });
            });
        egui::Panel::left("rail")
            .exact_size(RAIL_WIDTH)
            .resizable(false)
            .show(ui, |ui| self.rail(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::same(16)))
            .show(ui, |ui| match self.screen {
                Screen::Library => self.library_screen(ui),
                Screen::Search => self.search_screen(ui, &ctx),
                Screen::Ask => self.ask_screen(ui, &ctx),
                Screen::Settings => self.settings_screen(ui),
            });
        self.about_window(&ctx);
    }
}

fn style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = REDROB_BLUE;
    visuals.hyperlink_color = egui::Color32::from_rgb(0x7F, 0x9B, 0xFF);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
    });
}

/// The keyword snippet with the matched words in Redrob Blue, or the head of the chunk.
fn snippet_label(ui: &mut egui::Ui, result: &SearchResult) {
    let Some(snippet) = &result.snippet else {
        ui.label(work::plain_snippet(result));
        return;
    };
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat {
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let bold = egui::TextFormat {
        color: REDROB_BLUE,
        ..Default::default()
    };
    let mut rest = snippet.as_str();
    while let Some(start) = rest.find("<b>") {
        job.append(&rest[..start], 0.0, normal.clone());
        let after = &rest[start + 3..];
        match after.find("</b>") {
            Some(end) => {
                job.append(&after[..end], 0.0, bold.clone());
                rest = &after[end + 4..];
            }
            None => {
                rest = after;
                break;
            }
        }
    }
    job.append(rest, 0.0, normal);
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn sources_list(
    ui: &mut egui::Ui,
    sources: &[AnswerSource],
    open: &mut Option<String>,
    reveal: &mut Option<String>,
) {
    for source in sources {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("[{}]", source.number)).strong().color(REDROB_BLUE));
                ui.label(egui::RichText::new(&source.name).strong());
                if let Some(page) = source.page {
                    ui.weak(format!("p. {page}"));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Reveal").clicked() {
                        *reveal = Some(source.path.clone());
                    }
                    if ui.small_button("Open").clicked() {
                        *open = Some(source.path.clone());
                    }
                });
            });
            ui.weak(&source.path);
            ui.label(work::strip_bold(&source.excerpt).chars().take(400).collect::<String>());
        });
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
