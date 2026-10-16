// SPDX-License-Identifier: Apache-2.0
//! The Redrob Recall window, in the Redrob Canvas shape: a top bar (app name, the view tabs, the
//! Library and Help menus, the right slot with the account chip and the indexing chip), a left
//! panel with the library tree, the view in the centre, a collapsible messages dock on the right
//! and a status line at the bottom. One style, set once by the shell; every string through `i18n`.
//!
//! - Library: the folders, the index state and progress, add / remove / reveal, pause / resume.
//! - Search: search as you type over the one [`Library`], the result rows, open / reveal.
//! - Ask: a question, the console's answer with numbered citations that jump to their source.
//! - Settings: one page -- account, answer model, index, library location, local API, appearance.
//!
//! Nothing heavy runs on the UI thread: the indexer runs on its own threads inside the core crate,
//! searches and asks go through [`work::Jobs`], and the snapshot is re-read only when the library
//! says it changed (or once a second while a pass runs).

use crate::i18n::{self, t, tf};
use crate::prefs::UiPrefs;
use crate::theme::{self, ThemeKind, ThemePref, Tokens};
use crate::{account, services, work};
use redrob_recall_core::{
    AnswerSource, AppSettings, AskResponse, DocumentRecord, IndexProgress, IndexStatus, Library,
    LibrarySnapshot, SearchFilters, SearchResult,
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

/// Height of the top bar, in points (the Redrob Canvas value).
pub const TOP_BAR_HEIGHT: f32 = 28.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;
const LIBRARY_PANEL_DEFAULT: f32 = 240.0;
const MESSAGES_DOCK_DEFAULT: f32 = 320.0;
/// How long a status line stays before it has faded out, in seconds.
pub const STATUS_SECS: f32 = 5.0;
/// Keystrokes in the search box settle for this long before a search starts.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
/// How many messages the dock keeps.
const MAX_MESSAGES: usize = 200;
/// How many failed files the dock lists.
const MAX_FAILED: usize = 200;

/// The lockups for the About window: the Primary lockup on a light ground and the white one on a
/// dark ground, built by scripts/build-brand-assets.py and pinned in brand/SOURCES.json.
const LOCKUP: egui::ImageSource<'static> =
    egui::include_image!("../../../resources/brand/redrob-recall-lockup.png");
const LOCKUP_WHITE: egui::ImageSource<'static> =
    egui::include_image!("../../../resources/brand/redrob-recall-lockup-white.png");

pub const FOCUS_SEARCH_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::K);
pub const ADD_FOLDER_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
pub const MESSAGES_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::M,
);

/// Which view the centre shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Library,
    Search,
    Ask,
    Settings,
}

impl View {
    pub const ALL: [View; 4] = [View::Library, View::Search, View::Ask, View::Settings];

    /// The English name: the identifier in logs and the translation key.
    pub fn label(self) -> &'static str {
        match self {
            View::Library => "Library",
            View::Search => "Search",
            View::Ask => "Ask",
            View::Settings => "Settings",
        }
    }

    /// The tab's label, in the interface language.
    pub fn name(self) -> &'static str {
        t(self.label())
    }

    /// Ctrl+1 .. Ctrl+4.
    pub fn shortcut(self) -> egui::KeyboardShortcut {
        let key = match self {
            View::Library => egui::Key::Num1,
            View::Search => egui::Key::Num2,
            View::Ask => egui::Key::Num3,
            View::Settings => egui::Key::Num4,
        };
        egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key)
    }
}

/// The status line: one line that fades out after [`STATUS_SECS`].
struct Status {
    text: String,
    error: bool,
    since: Instant,
}

impl Status {
    /// Opacity now: 1 while fresh, fading over the last second; `None` once gone.
    fn alpha(&self) -> Option<f32> {
        let age = self.since.elapsed().as_secs_f32();
        if age >= STATUS_SECS {
            None
        } else {
            Some(((STATUS_SECS - age) / 1.0).clamp(0.0, 1.0))
        }
    }
}

/// One line of the messages dock: what the status line said, kept.
struct Message {
    text: String,
    error: bool,
    when: String,
}

struct SearchState {
    query: String,
    extension_filter: String,
    results: Vec<SearchResult>,
    searching: bool,
    generation: u64,
    error: Option<String>,
    searched_for: String,
    /// The last keystroke, while a search is waiting to start.
    pending_since: Option<Instant>,
    /// The highlighted row; Enter opens it.
    selected: Option<usize>,
    /// Scroll the list so the selected row is visible this frame.
    scroll_to_selected: bool,
    focus_box: bool,
}

struct AskState {
    question: String,
    asking: bool,
    generation: u64,
    answer: Option<AskResponse>,
    failure: Option<work::AskFailure>,
    asked: String,
    /// A citation was clicked: the source to scroll to and when, for a short highlight.
    jump_to: Option<(usize, Instant)>,
    focus_box: bool,
}

pub struct ShellApp {
    library: Library,
    runtime: tokio::runtime::Handle,
    jobs: work::Jobs,
    view: View,
    snapshot: Option<LibrarySnapshot>,
    progress: Option<IndexProgress>,
    failed: Vec<DocumentRecord>,
    dirty: Arc<AtomicBool>,
    last_refresh: Instant,
    settings_draft: AppSettings,
    settings_dirty: bool,
    prefs: UiPrefs,
    prefs_draft: UiPrefs,
    language: i18n::Language,
    theme_applied: Option<ThemeKind>,
    account: Option<account::Account>,
    account_watcher: account::Watcher,
    account_checked: Instant,
    /// The folder picked in the library tree; `None` is every folder.
    selected_folder: Option<String>,
    search: SearchState,
    ask: AskState,
    status: Option<Status>,
    messages: Vec<Message>,
    messages_open: bool,
    about_open: bool,
    shortcuts_open: bool,
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
        let prefs = UiPrefs::load();
        let language = i18n::Language::new(&prefs.language);
        i18n::set_current(language.effective());
        let settings_draft = library.settings();
        let mut app = Self {
            library,
            runtime,
            jobs: work::Jobs::new(),
            view: View::default(),
            snapshot: None,
            progress: None,
            failed: Vec::new(),
            dirty,
            last_refresh: Instant::now() - Duration::from_secs(10),
            settings_draft,
            settings_dirty: false,
            prefs_draft: prefs.clone(),
            prefs,
            language,
            theme_applied: None,
            account: account::current(),
            account_watcher: account::Watcher::start({
                let ctx = ctx.clone();
                move || ctx.request_repaint()
            }),
            account_checked: Instant::now(),
            selected_folder: None,
            search: SearchState {
                query: String::new(),
                extension_filter: String::new(),
                results: Vec::new(),
                searching: false,
                generation: 0,
                error: None,
                searched_for: String::new(),
                pending_since: None,
                selected: None,
                scroll_to_selected: false,
                focus_box: true,
            },
            ask: AskState {
                question: String::new(),
                asking: false,
                generation: 0,
                answer: None,
                failure: None,
                asked: String::new(),
                jump_to: None,
                focus_box: false,
            },
            status: None,
            messages: Vec::new(),
            messages_open: false,
            about_open: false,
            shortcuts_open: false,
        };
        // The one style: set here, before the first frame, and re-applied only on a theme change.
        app.sync_theme(ctx);
        // Search is where a person lands with something to find: start there when the library has
        // folders, on Library (its empty state) when it has none.
        if !app.library.settings().library_paths.is_empty() {
            app.view = View::Search;
        }
        app
    }

    /// Apply the theme the preference resolves to (following the OS when asked) when it changed.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let kind = self.prefs.theme.resolve(ctx.system_theme());
        if self.theme_applied != Some(kind) {
            theme::apply(ctx, kind);
            self.theme_applied = Some(kind);
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
            Ok(snapshot) => {
                if snapshot.stats.failed > 0 || !self.failed.is_empty() {
                    self.failed = self.library.failed_documents(MAX_FAILED).unwrap_or_default();
                }
                self.snapshot = Some(snapshot);
            }
            Err(error) => self.say_error(tf(
                "The library could not be read: {error}",
                &[("error", &error.to_string())],
            )),
        }
        self.progress = self.library.progress();
        if !self.settings_dirty {
            self.settings_draft = self.library.settings();
        }
        if let Some(selected) = &self.selected_folder
            && let Some(snapshot) = &self.snapshot
            && !snapshot.folders.iter().any(|f| &f.path == selected)
        {
            self.selected_folder = None;
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
                    self.search.selected = if results.is_empty() { None } else { Some(0) };
                    self.search.results = results;
                    self.search.error = None;
                }
                Err(error) => {
                    self.search.results.clear();
                    self.search.selected = None;
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
        self.post(text.into(), false);
    }

    fn say_error(&mut self, text: impl Into<String>) {
        self.post(text.into(), true);
    }

    /// Show `text` on the status line and keep it in the messages dock.
    fn post(&mut self, text: String, error: bool) {
        self.messages.insert(
            0,
            Message {
                text: text.clone(),
                error,
                when: chrono::Local::now().format("%H:%M").to_string(),
            },
        );
        self.messages.truncate(MAX_MESSAGES);
        self.status = Some(Status {
            text,
            error,
            since: Instant::now(),
        });
    }

    // ---- actions -------------------------------------------------------------------------------

    fn set_view(&mut self, view: View) {
        self.view = view;
        match view {
            View::Search => self.search.focus_box = true,
            View::Ask => self.ask.focus_box = true,
            _ => {}
        }
    }

    fn set_language(&mut self, pref: &str) {
        self.language.set(pref);
        self.prefs.language = self.language.pref().to_owned();
        self.prefs_draft.language = self.prefs.language.clone();
        self.prefs.save();
        i18n::set_current(self.language.effective());
    }

    fn set_theme(&mut self, pref: ThemePref, ctx: &egui::Context) {
        self.prefs.theme = pref;
        self.prefs_draft.theme = pref;
        self.prefs.save();
        self.sync_theme(ctx);
    }

    fn search_filters(&self) -> SearchFilters {
        SearchFilters {
            extensions: self
                .search
                .extension_filter
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                .collect(),
            path_prefix: self.selected_folder.clone(),
        }
    }

    fn start_search(&mut self, ctx: &egui::Context) {
        self.search.pending_since = None;
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            self.search.generation += 1;
            self.search.searching = false;
            self.search.results.clear();
            self.search.selected = None;
            self.search.error = None;
            self.search.searched_for.clear();
            return;
        }
        self.search.generation += 1;
        self.search.searching = true;
        self.search.error = None;
        self.search.searched_for = query.clone();
        let filters = self.search_filters();
        self.jobs.search(
            &self.library,
            self.search.generation,
            query,
            24,
            filters,
            ctx.clone(),
        );
    }

    /// A keystroke in the search box: the search starts once the typing settles.
    fn schedule_search(&mut self) {
        self.search.pending_since = Some(Instant::now());
    }

    fn poll_search_debounce(&mut self, ctx: &egui::Context) {
        let Some(since) = self.search.pending_since else {
            return;
        };
        let elapsed = since.elapsed();
        if elapsed >= SEARCH_DEBOUNCE {
            self.start_search(ctx);
        } else {
            ctx.request_repaint_after(SEARCH_DEBOUNCE - elapsed);
        }
    }

    fn clear_search(&mut self) {
        self.search.query.clear();
        self.search.pending_since = None;
        self.search.generation += 1;
        self.search.searching = false;
        self.search.results.clear();
        self.search.selected = None;
        self.search.error = None;
        self.search.searched_for.clear();
        self.search.focus_box = true;
    }

    fn start_ask(&mut self, ctx: &egui::Context) {
        let question = self.ask.question.trim().to_string();
        if question.is_empty() || self.ask.asking {
            return;
        }
        self.ask.generation += 1;
        self.ask.asking = true;
        self.ask.answer = None;
        self.ask.failure = None;
        self.ask.asked = question.clone();
        self.ask.jump_to = None;
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
            .set_title(t("Add a folder to your library"))
            .pick_folder()
        else {
            return;
        };
        self.add_folder(&folder);
    }

    fn add_folder(&mut self, folder: &Path) {
        if !folder.is_dir() {
            self.say_error(tf(
                "{path} is not a folder",
                &[("path", &folder.display().to_string())],
            ));
            return;
        }
        match self.library.add_folder(&folder.to_string_lossy()) {
            Ok(()) => {
                self.say(tf(
                    "Added {path}",
                    &[("path", &folder.display().to_string())],
                ));
                self.view = View::Library;
            }
            Err(message) => self.say_error(message),
        }
    }

    fn remove_folder(&mut self, path: &str) {
        match self.library.remove_folder(path) {
            Ok(()) => {
                if self.selected_folder.as_deref() == Some(path) {
                    self.selected_folder = None;
                }
                self.say(tf("Removed {path}", &[("path", path)]));
            }
            Err(message) => self.say_error(message),
        }
    }

    fn check_now(&mut self) {
        let started = self.library.request_index();
        self.say(if started {
            t("Re-reading your library")
        } else {
            t("Queued behind the running pass")
        });
    }

    fn open_source(&mut self, path: &str) {
        if let Err(message) = self
            .library
            .validated_source(path)
            .and_then(|source| services::open_path(&source))
        {
            self.say_error(message);
        }
    }

    fn reveal_source(&mut self, path: &str) {
        if let Err(message) = self
            .library
            .validated_source(path)
            .and_then(|source| services::reveal_path(&source))
        {
            self.say_error(message);
        }
    }

    fn open_selected_result(&mut self, reveal: bool) {
        let Some(index) = self.search.selected else {
            return;
        };
        let Some(path) = self.search.results.get(index).map(|r| r.path.clone()) else {
            return;
        };
        if reveal {
            self.reveal_source(&path);
        } else {
            self.open_source(&path);
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.search.results.is_empty() {
            self.search.selected = None;
            return;
        }
        let last = self.search.results.len() as isize - 1;
        let current = self.search.selected.map_or(-1, |i| i as isize);
        let next = (current + delta).clamp(0, last);
        self.search.selected = Some(next as usize);
        self.search.scroll_to_selected = true;
    }

    fn apply_settings(&mut self, ctx: &egui::Context) {
        if self.settings_dirty {
            match self.library.update_settings(self.settings_draft.clone()) {
                Ok(()) => self.settings_dirty = false,
                Err(error) => {
                    self.say_error(error.to_string());
                    return;
                }
            }
        }
        if self.prefs_draft != self.prefs {
            let draft = self.prefs_draft.clone();
            self.set_language(&draft.language);
            self.set_theme(draft.theme, ctx);
        }
        self.say(t("Settings saved"));
    }

    fn reset_settings(&mut self) {
        self.settings_draft = self.library.settings();
        self.settings_dirty = false;
        self.prefs_draft = self.prefs.clone();
    }

    fn settings_changed(&self) -> bool {
        self.settings_dirty || self.prefs_draft != self.prefs
    }

    // ---- input ---------------------------------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.about_open || self.shortcuts_open {
            return;
        }
        for view in View::ALL {
            if ctx.input_mut(|i| i.consume_shortcut(&view.shortcut())) {
                self.set_view(view);
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&FOCUS_SEARCH_SHORTCUT)) {
            self.set_view(View::Search);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&ADD_FOLDER_SHORTCUT)) {
            self.add_folder_dialog();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&MESSAGES_SHORTCUT)) {
            self.messages_open = !self.messages_open;
        }
        if self.view == View::Search {
            let (down, up, enter, ctrl_enter, escape) = ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            });
            if down {
                self.move_selection(1);
            }
            if up {
                self.move_selection(-1);
            }
            if ctrl_enter {
                self.open_selected_result(true);
            } else if enter {
                if self.search.pending_since.is_some() {
                    // Enter before the debounce fired: search now; the result opens on the next Enter.
                    self.start_search(ctx);
                } else {
                    self.open_selected_result(false);
                }
            }
            if escape {
                self.clear_search();
            }
        }
    }

    /// Folders dropped onto the window join the library; files are refused with a status line.
    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            self.add_folder(&path);
        }
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let t_ = Tokens::get(ctx);
        let rect = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("recall_drop_overlay"),
        ));
        painter.rect_filled(rect, 0.0, t_.shadow);
        painter.rect_stroke(
            rect.shrink(12.0),
            egui::CornerRadius::same(t_.radius_lg as u8),
            egui::Stroke::new(2.0, t_.accent),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            t("Drop to add the folder"),
            theme::semibold(18.0),
            t_.accent_text,
        );
    }

    // ---- chrome --------------------------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let frame = theme::panel_frame(ui);
        egui::Panel::top("recall_top_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(services::APP_NAME).strong());
                    ui.add_space(12.0);
                    ui.separator();
                    let mut next = self.view;
                    for view in View::ALL {
                        let tab = ui
                            .selectable_label(self.view == view, view.name())
                            .on_hover_text(format!(
                                "{}  ({})",
                                view.name(),
                                ui.ctx().format_shortcut(&view.shortcut())
                            ));
                        if tab.clicked() {
                            next = view;
                        }
                    }
                    if next != self.view {
                        self.set_view(next);
                    }
                    ui.separator();
                    ui.menu_button(t("Library"), |ui| self.library_menu(ui));
                    ui.menu_button(t("Help"), |ui| self.help_menu(ui));
                    self.status_in_top_bar(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        self.right_slot(ui);
                    });
                });
            });
    }

    fn library_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(t("Add folder…"))
            .on_hover_text(ui.ctx().format_shortcut(&ADD_FOLDER_SHORTCUT))
            .clicked()
        {
            ui.close();
            self.add_folder_dialog();
        }
        ui.separator();
        let status = self.snapshot.as_ref().map(|s| s.stats.status.clone());
        match status {
            Some(IndexStatus::Indexing | IndexStatus::Scanning) => {
                if ui.button(t("Pause indexing")).clicked() {
                    ui.close();
                    self.library.pause_indexing();
                }
            }
            Some(IndexStatus::Paused) => {
                if ui.button(t("Resume indexing")).clicked() {
                    ui.close();
                    self.library.resume_indexing();
                }
            }
            _ => {
                if ui.button(t("Check now")).clicked() {
                    ui.close();
                    self.check_now();
                }
            }
        }
    }

    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t("Language"), |ui| self.language_menu(ui));
        ui.menu_button(t("Theme"), |ui| self.theme_menu(ui));
        ui.separator();
        if ui.button(t("Shortcuts")).clicked() {
            ui.close();
            self.shortcuts_open = true;
        }
        if ui.button(t("Open the repository")).clicked() {
            ui.close();
            services::open_url(services::REPOSITORY);
        }
        ui.separator();
        if ui
            .button(tf("About {app}", &[("app", services::APP_NAME)]))
            .clicked()
        {
            ui.close();
            self.about_open = true;
        }
    }

    /// Help > Language: the launcher's / system's language, then English and Korean.
    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let fallback = self.language.fallback();
        let auto = ui
            .radio(
                self.language.is_auto(),
                tf("System ({name})", &[("name", fallback.name)]),
            )
            .on_hover_text(t(
                "Follow Redrob Launcher's language, else the operating system's",
            ));
        if auto.clicked() {
            ui.close();
            self.set_language(i18n::AUTO);
        }
        ui.separator();
        let effective = self.language.effective();
        for lang in i18n::LANGUAGES {
            let selected = !self.language.is_auto() && effective.code == lang.code;
            if ui.radio(selected, lang.name).clicked() {
                ui.close();
                self.set_language(lang.code);
            }
        }
    }

    /// Help > Theme: follow the system, or light or dark.
    fn theme_menu(&mut self, ui: &mut egui::Ui) {
        for pref in ThemePref::ALL {
            let item = ui.radio(self.prefs.theme == pref, t(pref.label()));
            let item = if pref == ThemePref::System {
                item.on_hover_text(t("Follow the operating system's appearance"))
            } else {
                item
            };
            if item.clicked() {
                ui.close();
                let ctx = ui.ctx().clone();
                self.set_theme(pref, &ctx);
            }
        }
    }

    /// The last status, fading, in the top bar (the Redrob Canvas place for it); the bottom status
    /// line carries the index progress.
    fn status_in_top_bar(&mut self, ui: &mut egui::Ui) {
        let Some(status) = &self.status else {
            return;
        };
        let Some(alpha) = status.alpha() else {
            self.status = None;
            return;
        };
        ui.add_space(8.0);
        let color = if status.error {
            ui.visuals().error_fg_color
        } else {
            ui.visuals().text_color()
        };
        ui.add(
            egui::Label::new(egui::RichText::new(&status.text).color(color.gamma_multiply(alpha)))
                .truncate(),
        )
        .on_hover_text(&status.text);
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    /// The right side of the top bar: the messages toggle, the indexing chip, the account chip.
    fn right_slot(&mut self, ui: &mut egui::Ui) {
        let t_ = Tokens::get(ui.ctx());
        let (label, hover) = if self.messages_open {
            ("▸", t("Hide messages"))
        } else {
            ("◂", t("Show messages"))
        };
        let unread_errors = self.messages.iter().filter(|m| m.error).count() + self.failed.len();
        let toggle_text = if unread_errors > 0 && !self.messages_open {
            format!("{label} {unread_errors}")
        } else {
            label.to_string()
        };
        if ui
            .selectable_label(self.messages_open, toggle_text)
            .on_hover_text(format!(
                "{}  ({})",
                hover,
                ui.ctx().format_shortcut(&MESSAGES_SHORTCUT)
            ))
            .clicked()
        {
            self.messages_open = !self.messages_open;
        }
        ui.separator();
        // The account chip: a coloured dot and a short label; the whole account on hover.
        let signed_in = self.account.is_some();
        let dot = if signed_in { t_.success } else { t_.text_faint };
        let text = if signed_in {
            t("Signed in with Redrob Launcher")
        } else {
            t("Connect")
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(
            "● ",
            0.0,
            egui::TextFormat {
                color: dot,
                ..Default::default()
            },
        );
        job.append(
            text,
            0.0,
            egui::TextFormat {
                color: if signed_in { t_.text_dim } else { t_.text },
                ..Default::default()
            },
        );
        let chip = ui.add(egui::Button::new(job).frame(false));
        let chip = match &self.account {
            Some(account) => chip.on_hover_text(account.label()),
            None => chip.on_hover_text(t(
                "Sign in once in Redrob Launcher; every Redrob app on this computer uses that account",
            )),
        };
        if chip.clicked() {
            self.set_view(View::Settings);
        }
        // The indexing chip, while a pass runs.
        if let Some(status) = self.snapshot.as_ref().map(|s| s.stats.status.clone()) {
            let text = match (&status, &self.progress) {
                (IndexStatus::Indexing | IndexStatus::Scanning, Some(p)) if p.total > 0 => Some(
                    tf(
                        "{done} / {total}",
                        &[("done", &p.processed.to_string()), ("total", &p.total.to_string())],
                    ),
                ),
                (IndexStatus::Indexing, _) => Some(t("Indexing").to_string()),
                (IndexStatus::Scanning, _) => Some(t("Scanning").to_string()),
                (IndexStatus::Paused, _) => Some(t("Paused").to_string()),
                _ => None,
            };
            if let Some(text) = text {
                ui.separator();
                let chip = ui.add(
                    egui::Button::new(egui::RichText::new(text).color(t_.accent)).frame(false),
                );
                if status != IndexStatus::Paused {
                    ui.add(egui::Spinner::new().size(12.0).color(t_.accent));
                }
                if chip.clicked() {
                    self.set_view(View::Library);
                }
            }
        }
    }

    /// The bottom status line: the index progress with Pause / Resume, the counts, the version.
    fn status_line(&mut self, ui: &mut egui::Ui) {
        let frame = theme::panel_frame(ui);
        egui::Panel::bottom("recall_status_line")
            .exact_size(STATUS_BAR_HEIGHT)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    let t_ = Tokens::get(ui.ctx());
                    let Some(snapshot) = self.snapshot.clone() else {
                        ui.spinner();
                        return;
                    };
                    let stats = &snapshot.stats;
                    let active = matches!(
                        stats.status,
                        IndexStatus::Indexing | IndexStatus::Scanning | IndexStatus::Paused
                    );
                    if active {
                        match stats.status {
                            IndexStatus::Paused => {
                                if ui.small_button(t("Resume")).clicked() {
                                    self.library.resume_indexing();
                                }
                            }
                            _ => {
                                if ui.small_button(t("Pause")).clicked() {
                                    self.library.pause_indexing();
                                }
                            }
                        }
                        let text = match &self.progress {
                            Some(p) if p.total > 0 => match &p.current_file {
                                Some(file) => tf(
                                    "{done} / {total}  {file}",
                                    &[
                                        ("done", &p.processed.to_string()),
                                        ("total", &p.total.to_string()),
                                        ("file", file),
                                    ],
                                ),
                                None => tf(
                                    "{done} / {total}",
                                    &[
                                        ("done", &p.processed.to_string()),
                                        ("total", &p.total.to_string()),
                                    ],
                                ),
                            },
                            Some(p) => p.message.clone(),
                            None => match stats.status {
                                IndexStatus::Paused => t("Indexing paused").to_string(),
                                IndexStatus::Scanning => t("Scanning").to_string(),
                                _ => t("Indexing").to_string(),
                            },
                        };
                        let fraction = self
                            .progress
                            .as_ref()
                            .filter(|p| p.total > 0)
                            .map(|p| p.processed as f32 / p.total as f32);
                        if let Some(fraction) = fraction {
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .desired_width(120.0)
                                    .desired_height(8.0)
                                    .fill(t_.accent)
                                    .animate(stats.status != IndexStatus::Paused),
                            );
                        }
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).color(t_.text_dim))
                                .truncate(),
                        );
                        ui.ctx().request_repaint_after(Duration::from_millis(250));
                    } else {
                        let mut line = tf(
                            "{docs} documents, {passages} passages, {size}",
                            &[
                                ("docs", &stats.documents.to_string()),
                                ("passages", &stats.indexed_chunks.to_string()),
                                ("size", &human_bytes(stats.total_bytes)),
                            ],
                        );
                        if stats.failed > 0 {
                            line.push_str(" · ");
                            line.push_str(&tf(
                                "{n} files could not be read",
                                &[("n", &stats.failed.to_string())],
                            ));
                        }
                        ui.label(egui::RichText::new(line).color(t_.text_dim));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .color(t_.text_faint),
                        );
                    });
                });
            });
    }

    /// The left panel: the library tree -- every folder, its document count and its index state.
    fn library_panel(&mut self, ui: &mut egui::Ui) {
        let frame = theme::panel_frame(ui).inner_margin(egui::Margin::same(8));
        egui::Panel::left("recall_library_panel")
            .default_size(LIBRARY_PANEL_DEFAULT)
            .size_range(180.0..=420.0)
            .resizable(true)
            .frame(frame)
            .show(ui, |ui| {
                let t_ = Tokens::get(ui.ctx());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("Library")).font(theme::semibold(12.5)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("+")
                            .on_hover_text(format!(
                                "{}  ({})",
                                t("Add folder"),
                                ui.ctx().format_shortcut(&ADD_FOLDER_SHORTCUT)
                            ))
                            .clicked()
                        {
                            self.add_folder_dialog();
                        }
                    });
                });
                ui.add_space(4.0);
                let Some(snapshot) = self.snapshot.clone() else {
                    ui.spinner();
                    return;
                };
                let stats = &snapshot.stats;
                let mut select: Option<Option<String>> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let all = tree_row(
                            ui,
                            &t_,
                            self.selected_folder.is_none(),
                            t("All folders"),
                            &tf("{n} documents", &[("n", &stats.documents.to_string())]),
                            folder_dot(&t_, stats.status.clone(), stats.failed, true),
                            None,
                        );
                        if all.clicked() {
                            select = Some(None);
                        }
                        for folder in &snapshot.folders {
                            let name = folder_name(&folder.path);
                            let indexing_here = matches!(
                                stats.status,
                                IndexStatus::Indexing | IndexStatus::Scanning
                            ) && stats
                                .current_file
                                .as_deref()
                                .is_some_and(|f| f.starts_with(folder.path.as_str()));
                            let state = if !folder.present {
                                IndexStatus::Error
                            } else if indexing_here {
                                IndexStatus::Indexing
                            } else {
                                IndexStatus::Idle
                            };
                            let detail = if folder.failed > 0 {
                                tf(
                                    "{n} documents, {m} failed",
                                    &[
                                        ("n", &folder.files.to_string()),
                                        ("m", &folder.failed.to_string()),
                                    ],
                                )
                            } else {
                                tf("{n} documents", &[("n", &folder.files.to_string())])
                            };
                            let row = tree_row(
                                ui,
                                &t_,
                                self.selected_folder.as_deref() == Some(folder.path.as_str()),
                                &name,
                                &detail,
                                folder_dot(&t_, state, folder.failed, folder.present),
                                Some(&folder.path),
                            );
                            if row.clicked() {
                                select = Some(Some(folder.path.clone()));
                            }
                        }
                    });
                if let Some(selection) = select {
                    self.selected_folder = selection;
                    self.view = match self.view {
                        View::Search => View::Search,
                        _ => View::Library,
                    };
                    if self.view == View::Search && !self.search.query.trim().is_empty() {
                        self.schedule_search();
                    }
                }
            });
    }

    /// The right dock: the status history and the files the indexer could not read. A list,
    /// never a modal.
    fn messages_dock(&mut self, ui: &mut egui::Ui) {
        if !self.messages_open {
            return;
        }
        let frame = theme::panel_frame(ui).inner_margin(egui::Margin::same(8));
        egui::Panel::right("recall_messages_dock")
            .default_size(MESSAGES_DOCK_DEFAULT)
            .size_range(240.0..=560.0)
            .resizable(true)
            .frame(frame)
            .show(ui, |ui| {
                let t_ = Tokens::get(ui.ctx());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("Messages")).font(theme::semibold(12.5)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t("Hide")).clicked() {
                            self.messages_open = false;
                        }
                        if !self.messages.is_empty() && ui.small_button(t("Clear")).clicked() {
                            self.messages.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                let mut reveal: Option<String> = None;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.messages.is_empty() {
                            ui.label(egui::RichText::new(t("No messages")).color(t_.text_faint));
                        }
                        for message in &self.messages {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(&message.when)
                                        .monospace()
                                        .color(t_.text_faint),
                                );
                                let color = if message.error { t_.danger } else { t_.text };
                                ui.label(egui::RichText::new(&message.text).color(color));
                            });
                        }
                        if !self.failed.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(t("Files that could not be read"))
                                    .font(theme::semibold(12.5)),
                            );
                            ui.add_space(2.0);
                            for document in &self.failed {
                                theme::card_frame(ui).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&document.name).strong());
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.small_button(t("Reveal")).clicked() {
                                                    reveal = Some(document.path.clone());
                                                }
                                            },
                                        );
                                    });
                                    path_label(ui, &t_, &document.path);
                                    if let Some(error) = &document.last_error {
                                        ui.label(egui::RichText::new(error).color(t_.danger));
                                    }
                                });
                            }
                        }
                    });
                if let Some(path) = reveal {
                    self.reveal_source(&path);
                }
            });
    }

    // ---- views ---------------------------------------------------------------------------------

    fn library_view(&mut self, ui: &mut egui::Ui) {
        let t_ = Tokens::get(ui.ctx());
        let Some(snapshot) = self.snapshot.clone() else {
            ui.spinner();
            return;
        };
        let stats = &snapshot.stats;

        if snapshot.folders.is_empty() {
            self.empty_state(ui, &t_);
            return;
        }

        // The overview card: state, counts, progress.
        theme::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (label, color) = match stats.status {
                    IndexStatus::Idle => (t("Up to date"), t_.success),
                    IndexStatus::Scanning => (t("Scanning"), t_.accent),
                    IndexStatus::Indexing => (t("Indexing"), t_.accent),
                    IndexStatus::Paused => (t("Paused"), t_.warning),
                    IndexStatus::Error => (t("Error"), t_.danger),
                };
                ui.label(egui::RichText::new(label).font(theme::semibold(13.0)).color(color));
                ui.separator();
                ui.label(tf(
                    "{docs} documents, {passages} passages, {size}",
                    &[
                        ("docs", &stats.documents.to_string()),
                        ("passages", &stats.indexed_chunks.to_string()),
                        ("size", &human_bytes(stats.total_bytes)),
                    ],
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match stats.status {
                        IndexStatus::Indexing | IndexStatus::Scanning => {
                            if ui.button(t("Pause")).clicked() {
                                self.library.pause_indexing();
                            }
                        }
                        IndexStatus::Paused => {
                            if ui.button(t("Resume")).clicked() {
                                self.library.resume_indexing();
                            }
                        }
                        _ => {
                            if ui.button(t("Check now")).clicked() {
                                self.check_now();
                            }
                        }
                    }
                    if ui.button(t("Add folder")).clicked() {
                        self.add_folder_dialog();
                    }
                });
            });
            if let Some(progress) = &self.progress
                && matches!(
                    stats.status,
                    IndexStatus::Indexing | IndexStatus::Scanning | IndexStatus::Paused
                )
            {
                let fraction = if progress.total == 0 {
                    0.0
                } else {
                    progress.processed as f32 / progress.total as f32
                };
                let text = match &progress.current_file {
                    Some(file) => tf(
                        "{done} / {total}  {file}",
                        &[
                            ("done", &progress.processed.to_string()),
                            ("total", &progress.total.to_string()),
                            ("file", file),
                        ],
                    ),
                    None => progress.message.clone(),
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(text)
                        .fill(t_.accent)
                        .animate(stats.status != IndexStatus::Paused),
                );
            }
            ui.horizontal(|ui| {
                if let Some(last) = &stats.last_indexed_at {
                    ui.label(
                        egui::RichText::new(tf(
                            "Last indexed {when}",
                            &[(
                                "when",
                                &last.replace('T', " ").chars().take(19).collect::<String>(),
                            )],
                        ))
                        .color(t_.text_faint),
                    );
                }
                if stats.failed > 0 {
                    ui.label(
                        egui::RichText::new(tf(
                            "{n} files could not be read",
                            &[("n", &stats.failed.to_string())],
                        ))
                        .color(t_.warning),
                    );
                    if ui.link(t("Show")).clicked() {
                        self.messages_open = true;
                    }
                }
            });
        });
        ui.add_space(8.0);

        let busy = matches!(
            stats.status,
            IndexStatus::Indexing | IndexStatus::Scanning
        );
        let folders: Vec<_> = snapshot
            .folders
            .iter()
            .filter(|f| {
                self.selected_folder
                    .as_deref()
                    .is_none_or(|selected| selected == f.path)
            })
            .cloned()
            .collect();
        let mut remove: Option<String> = None;
        let mut reveal: Option<String> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for folder in &folders {
                    theme::card_frame(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(folder_name(&folder.path))
                                        .font(theme::semibold(13.5)),
                                );
                                path_label(ui, &t_, &folder.path);
                                let line = if folder.failed > 0 {
                                    tf(
                                        "{n} documents, {m} failed",
                                        &[
                                            ("n", &folder.files.to_string()),
                                            ("m", &folder.failed.to_string()),
                                        ],
                                    )
                                } else {
                                    tf("{n} documents", &[("n", &folder.files.to_string())])
                                };
                                ui.label(egui::RichText::new(line).color(t_.text_dim));
                                if !folder.present {
                                    ui.label(
                                        egui::RichText::new(t(
                                            "This folder is missing; its records are kept until it returns.",
                                        ))
                                        .color(t_.warning),
                                    );
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Min),
                                |ui| {
                                    if ui
                                        .add_enabled(!busy, egui::Button::new(t("Remove")))
                                        .on_disabled_hover_text(t("Wait for indexing to finish"))
                                        .clicked()
                                    {
                                        remove = Some(folder.path.clone());
                                    }
                                    if folder.present && ui.button(t("Reveal")).clicked() {
                                        reveal = Some(folder.path.clone());
                                    }
                                },
                            );
                        });
                    });
                    ui.add_space(6.0);
                }
            });
        if let Some(path) = remove {
            self.remove_folder(&path);
        }
        if let Some(path) = reveal
            && let Err(message) = services::open_path(Path::new(&path))
        {
            self.say_error(message);
        }
    }

    /// First run: one primary action and the drop hint.
    fn empty_state(&mut self, ui: &mut egui::Ui, t_: &Tokens) {
        let height = ui.available_height();
        ui.add_space((height * 0.3).max(24.0));
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(t("No folders yet")).font(theme::semibold(20.0)));
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(t(
                    "Add a folder and Recall indexes it on this machine. Nothing leaves it.",
                ))
                .color(t_.text_dim),
            );
            ui.add_space(16.0);
            let button = egui::Button::new(
                egui::RichText::new(t("Add folder"))
                    .font(theme::semibold(14.0))
                    .color(t_.primary_text),
            )
            .fill(t_.primary_bg)
            .min_size(egui::vec2(180.0, 36.0));
            if ui.add(button).clicked() {
                self.add_folder_dialog();
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(t("or drop a folder onto this window")).color(t_.text_faint),
            );
        });
    }

    fn search_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t_ = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 140.0).max(120.0);
            let response = ui.add_sized(
                [width, 30.0],
                egui::TextEdit::singleline(&mut self.search.query)
                    .font(egui::TextStyle::Body)
                    .hint_text(t(
                        "Search your files: words, \"a phrase\", -excluded, heading:word",
                    )),
            );
            if self.search.focus_box {
                response.request_focus();
                self.search.focus_box = false;
            }
            if response.changed() {
                self.schedule_search();
            }
            if ui
                .add_sized(
                    [120.0, 30.0],
                    egui::TextEdit::singleline(&mut self.search.extension_filter)
                        .hint_text(t("pdf, md")),
                )
                .on_hover_text(t("Only these file types (comma separated)"))
                .changed()
            {
                self.schedule_search();
            }
        });
        ui.add_space(4.0);
        // The line under the box: what is being searched, or how to use it.
        let scope = self
            .selected_folder
            .as_deref()
            .map(|folder| tf("in {folder}", &[("folder", &folder_name(folder))]));
        ui.horizontal(|ui| {
            if self.search.searching || self.search.pending_since.is_some() {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(egui::RichText::new(t("Searching…")).color(t_.text_faint));
            } else if let Some(error) = &self.search.error {
                ui.colored_label(t_.danger, error);
            } else if !self.search.searched_for.is_empty() {
                let text = if self.search.results.is_empty() {
                    tf("No results for {query}", &[("query", &self.search.searched_for)])
                } else {
                    tf(
                        "{n} results for {query}",
                        &[
                            ("n", &self.search.results.len().to_string()),
                            ("query", &self.search.searched_for),
                        ],
                    )
                };
                ui.label(egui::RichText::new(text).color(t_.text_faint));
            } else {
                ui.label(
                    egui::RichText::new(t(
                        "Type to search; Enter opens, Ctrl+Enter reveals, Esc clears.",
                    ))
                    .color(t_.text_faint),
                );
            }
            if let Some(scope) = scope {
                ui.label(egui::RichText::new(scope).color(t_.accent));
            }
        });
        ui.add_space(4.0);

        let results = self.search.results.clone();
        let selected = self.search.selected;
        let scroll_to = std::mem::take(&mut self.search.scroll_to_selected);
        let mut open: Option<String> = None;
        let mut reveal: Option<String> = None;
        let mut select: Option<usize> = None;
        let mut ask_about: Option<String> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, result) in results.iter().enumerate() {
                    let is_selected = selected == Some(index);
                    let frame = theme::card_frame(ui).fill(if is_selected {
                        t_.row_selected
                    } else {
                        t_.card
                    });
                    let frame = if is_selected {
                        frame.stroke(egui::Stroke::new(1.0, t_.accent_border))
                    } else {
                        frame
                    };
                    let response = frame
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&result.name).font(theme::semibold(13.5)),
                                );
                                if let Some(page) = result.page {
                                    ui.label(
                                        egui::RichText::new(tf(
                                            "p. {page}",
                                            &[("page", &page.to_string())],
                                        ))
                                        .color(t_.text_faint),
                                    );
                                }
                                if let Some(heading) = &result.heading {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(heading).color(t_.text_faint),
                                        )
                                        .truncate(),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.0}%",
                                                result.score * 100.0
                                            ))
                                            .monospace()
                                            .color(t_.text_faint),
                                        );
                                        if ui.small_button(t("Reveal")).clicked() {
                                            reveal = Some(result.path.clone());
                                        }
                                        if ui.small_button(t("Open")).clicked() {
                                            open = Some(result.path.clone());
                                        }
                                        if ui.small_button(t("Ask about this")).clicked() {
                                            ask_about = Some(self.search.searched_for.clone());
                                        }
                                    },
                                );
                            });
                            path_label(ui, &t_, &result.path);
                            snippet_label(ui, &t_, result);
                        })
                        .response;
                    if is_selected && scroll_to {
                        response.scroll_to_me(None);
                    }
                    let row = response.interact(egui::Sense::click());
                    if row.clicked() {
                        select = Some(index);
                    }
                    if row.double_clicked() {
                        open = Some(result.path.clone());
                    }
                    ui.add_space(4.0);
                }
            });
        if let Some(index) = select {
            self.search.selected = Some(index);
        }
        if let Some(path) = open {
            self.open_source(&path);
        }
        if let Some(path) = reveal {
            self.reveal_source(&path);
        }
        if let Some(query) = ask_about {
            self.ask.question = query;
            self.set_view(View::Ask);
            self.start_ask(ctx);
        }
    }

    fn ask_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t_ = Tokens::get(ui.ctx());
        match &self.account {
            Some(account) => {
                ui.label(
                    egui::RichText::new(tf(
                        "Answers come from the Redrob console as {account}; only the matching passages are sent.",
                        &[("account", &account.label())],
                    ))
                    .color(t_.text_faint),
                );
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t(
                            "Sign in with Redrob Launcher to ask; search works without it.",
                        ))
                        .color(t_.warning),
                    );
                    if ui.link(t("Get Redrob Launcher")).clicked() {
                        services::open_url(services::LAUNCHER_RELEASES);
                    }
                });
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 110.0).max(120.0);
            let response = ui.add_sized(
                [width, 30.0],
                egui::TextEdit::singleline(&mut self.ask.question)
                    .hint_text(t("Ask a question about your files")),
            );
            if self.ask.focus_box {
                response.request_focus();
                self.ask.focus_box = false;
            }
            let submitted =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let button = egui::Button::new(
                egui::RichText::new(t("Ask")).color(t_.primary_text),
            )
            .fill(t_.primary_bg)
            .min_size(egui::vec2(100.0, 30.0));
            if ui.add_enabled(!self.ask.asking, button).clicked() || submitted {
                self.start_ask(ctx);
            }
        });
        ui.add_space(8.0);
        let mut open: Option<String> = None;
        let mut reveal: Option<String> = None;
        let mut jump: Option<usize> = None;
        let jump_to = self.ask.jump_to.take();
        let highlight = jump_to.filter(|(_, when)| when.elapsed() < Duration::from_secs(2));
        // Scroll on the click frame only; the highlight stays two seconds.
        let scroll_to = jump_to
            .filter(|(_, when)| when.elapsed() < Duration::from_millis(150))
            .map(|(n, _)| n);
        if highlight.is_some() {
            self.ask.jump_to = highlight;
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.ask.asking {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(t("Reading your files and asking Redrob"))
                                .color(t_.text_faint),
                        );
                    });
                }
                if let Some(answer) = &self.ask.answer {
                    theme::card_frame(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new(&self.ask.asked).font(theme::semibold(13.5)));
                        ui.add_space(6.0);
                        if let Some(n) = answer_with_citations(ui, &t_, &answer.answer) {
                            jump = Some(n);
                        }
                        ui.add_space(6.0);
                        let secs = format!("{:.1}", answer.latency_ms as f32 / 1000.0);
                        let meta = match (answer.input_tokens, answer.output_tokens) {
                            (Some(i), Some(o)) => tf(
                                "{model} in {secs}s, {input} in / {output} out tokens",
                                &[
                                    ("model", &answer.model),
                                    ("secs", &secs),
                                    ("input", &i.to_string()),
                                    ("output", &o.to_string()),
                                ],
                            ),
                            _ => tf(
                                "{model} in {secs}s",
                                &[("model", &answer.model), ("secs", &secs)],
                            ),
                        };
                        ui.label(egui::RichText::new(meta).color(t_.text_faint));
                    });
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(t("Sources")).font(theme::semibold(12.5)));
                    sources_list(
                        ui,
                        &t_,
                        &answer.sources,
                        highlight.map(|(n, _)| n),
                        scroll_to,
                        &mut open,
                        &mut reveal,
                    );
                }
                if let Some(failure) = &self.ask.failure {
                    theme::card_frame(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.colored_label(t_.danger, &failure.message);
                        ui.label(
                            egui::RichText::new(format!("({})", failure.code))
                                .color(t_.text_faint),
                        );
                        if failure.code == "not_signed_in"
                            && ui.link(t("Get Redrob Launcher")).clicked()
                        {
                            services::open_url(services::LAUNCHER_RELEASES);
                        }
                    });
                    if !failure.sources.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(t("Sources found locally"))
                                .font(theme::semibold(12.5)),
                        );
                        sources_list(ui, &t_, &failure.sources, None, None, &mut open, &mut reveal);
                    }
                }
            });
        if let Some(n) = jump {
            // The citation jumps to its source and opens the file.
            self.ask.jump_to = Some((n, Instant::now()));
            if let Some(path) = self
                .ask
                .answer
                .as_ref()
                .and_then(|a| a.sources.iter().find(|s| s.number == n))
                .map(|s| s.path.clone())
            {
                open = Some(path);
            }
        }
        if let Some(path) = open {
            self.open_source(&path);
        }
        if let Some(path) = reveal {
            self.reveal_source(&path);
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let t_ = Tokens::get(ui.ctx());
        let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
            theme::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(title).font(theme::semibold(13.0)));
                ui.add_space(4.0);
                body(ui);
            });
            ui.add_space(8.0);
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Account.
                let account = self.account.clone();
                let mut check_again = false;
                section(ui, t("Redrob account"), &mut |ui| match &account {
                    Some(account) => {
                        ui.label(tf("Signed in as {account}", &[("account", &account.label())]));
                        ui.label(
                            egui::RichText::new(tf(
                                "Console: {url}",
                                &[("url", &account.api_base_url())],
                            ))
                            .color(t_.text_faint),
                        );
                        if account.source == account::Source::Shared {
                            ui.label(
                                egui::RichText::new(t(
                                    "Sign out in Redrob Launcher; this app never writes the shared file.",
                                ))
                                .color(t_.text_faint),
                            );
                        }
                    }
                    None => {
                        ui.label(t(
                            "Not signed in. Sign in once in Redrob Launcher and every Redrob app on this computer, Recall included, uses that account.",
                        ));
                        ui.horizontal(|ui| {
                            if ui.button(t("Open Redrob Launcher")).clicked() {
                                services::open_url(services::LAUNCHER_RELEASES);
                            }
                            if ui.button(t("Check again")).clicked() {
                                check_again = true;
                            }
                        });
                        if let Some(path) = account::shared_account_path() {
                            ui.label(
                                egui::RichText::new(tf(
                                    "Shared account file: {path}",
                                    &[("path", &path.display().to_string())],
                                ))
                                .color(t_.text_faint),
                            );
                        }
                    }
                });
                if check_again {
                    self.account = account::current();
                }

                // Ask.
                let draft = &mut self.settings_draft;
                let dirty = &mut self.settings_dirty;
                section(ui, t("Ask"), &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t("Answer model"));
                        if ui
                            .add(egui::TextEdit::singleline(&mut draft.answer_model).desired_width(200.0))
                            .changed()
                        {
                            *dirty = true;
                        }
                        ui.label(
                            egui::RichText::new(t("auto lets the console choose")).color(t_.text_faint),
                        );
                    });
                    if ui
                        .checkbox(
                            &mut draft.send_context_to_redrob,
                            t("Send the matching passages to Redrob when asking"),
                        )
                        .changed()
                    {
                        *dirty = true;
                    }
                });

                // Index.
                section(ui, t("Index"), &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t("Embedding model"));
                        ui.label(
                            egui::RichText::new(t(
                                "multilingual-e5-small (fastembed), downloaded once into the data folder and run on this machine",
                            ))
                            .color(t_.text_faint),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("File types"));
                        let mut text = draft.include_extensions.join(", ");
                        if ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(360.0))
                            .changed()
                        {
                            draft.include_extensions = text
                                .split(|c: char| c == ',' || c.is_whitespace())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.trim_start_matches('.').to_ascii_lowercase())
                                .collect();
                            *dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("Skip folders named"));
                        let mut text = draft.excluded_paths.join(", ");
                        if ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(360.0))
                            .changed()
                        {
                            draft.excluded_paths = text
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            *dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("Largest file (MB)"));
                        if ui
                            .add(egui::DragValue::new(&mut draft.max_file_size_mb).range(1..=250))
                            .changed()
                        {
                            *dirty = true;
                        }
                        ui.add_space(12.0);
                        ui.label(t("Passage size"));
                        if ui
                            .add(egui::DragValue::new(&mut draft.chunk_size).range(256..=8192))
                            .changed()
                        {
                            *dirty = true;
                        }
                        ui.add_space(12.0);
                        ui.label(t("Overlap"));
                        if ui
                            .add(egui::DragValue::new(&mut draft.chunk_overlap).range(0..=2048))
                            .changed()
                        {
                            *dirty = true;
                        }
                    });
                });

                // Library location.
                let data_dir = self
                    .snapshot
                    .as_ref()
                    .map(|s| s.data_directory.clone())
                    .unwrap_or_default();
                let mut reveal_data_dir = false;
                section(ui, t("Library location"), &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&data_dir).monospace());
                        if ui.small_button(t("Reveal")).clicked() {
                            reveal_data_dir = true;
                        }
                    });
                    ui.label(
                        egui::RichText::new(t(
                            "Where the index, the embedding model and the local API token live. REDROB_RECALL_DATA_DIR moves it.",
                        ))
                        .color(t_.text_faint),
                    );
                });
                if reveal_data_dir
                    && let Err(message) = services::open_path(Path::new(&data_dir))
                {
                    self.say_error(message);
                }

                // Local API.
                let draft = &mut self.settings_draft;
                let dirty = &mut self.settings_dirty;
                let data_dir_for_token = data_dir.clone();
                section(ui, t("Local API"), &mut |ui| {
                    ui.label(
                        egui::RichText::new(t(
                            "What the other Redrob apps call to search this library: 127.0.0.1 only, one bearer token (docs/API.md).",
                        ))
                        .color(t_.text_faint),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut draft.local_api_enabled, t("Serve the local API"))
                            .changed()
                        {
                            *dirty = true;
                        }
                        ui.add_space(12.0);
                        ui.label(t("Port"));
                        if ui
                            .add(egui::DragValue::new(&mut draft.local_api_port).range(1024..=65535))
                            .changed()
                        {
                            *dirty = true;
                        }
                    });
                    ui.label(
                        egui::RichText::new(tf(
                            "Token: {path} (never shown here). Changes to the API take effect at the next start.",
                            &[("path", &format!("{data_dir_for_token}/local-api-token"))],
                        ))
                        .color(t_.text_faint),
                    );
                });

                // Appearance.
                let prefs_draft = &mut self.prefs_draft;
                let fallback = self.language.fallback();
                section(ui, t("Appearance"), &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t("Language"));
                        let current = if prefs_draft.language.eq_ignore_ascii_case(i18n::AUTO) {
                            tf("System ({name})", &[("name", fallback.name)])
                        } else {
                            i18n::by_code(&prefs_draft.language)
                                .map_or_else(|| prefs_draft.language.clone(), |l| l.name.to_owned())
                        };
                        egui::ComboBox::from_id_salt("recall_language")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        prefs_draft.language.eq_ignore_ascii_case(i18n::AUTO),
                                        tf("System ({name})", &[("name", fallback.name)]),
                                    )
                                    .clicked()
                                {
                                    prefs_draft.language = i18n::AUTO.to_owned();
                                }
                                for lang in i18n::LANGUAGES {
                                    if ui
                                        .selectable_label(prefs_draft.language == lang.code, lang.name)
                                        .clicked()
                                    {
                                        prefs_draft.language = lang.code.to_owned();
                                    }
                                }
                            });
                        ui.add_space(12.0);
                        ui.label(t("Theme"));
                        egui::ComboBox::from_id_salt("recall_theme")
                            .selected_text(t(prefs_draft.theme.label()))
                            .show_ui(ui, |ui| {
                                for pref in ThemePref::ALL {
                                    if ui
                                        .selectable_label(prefs_draft.theme == pref, t(pref.label()))
                                        .clicked()
                                    {
                                        prefs_draft.theme = pref;
                                    }
                                }
                            });
                    });
                });

                // Apply / Reset.
                let changed = self.settings_changed();
                ui.horizontal(|ui| {
                    let apply = egui::Button::new(
                        egui::RichText::new(t("Apply")).color(t_.primary_text),
                    )
                    .fill(t_.primary_bg)
                    .min_size(egui::vec2(100.0, 28.0));
                    if ui.add_enabled(changed, apply).clicked() {
                        self.apply_settings(ctx);
                    }
                    if ui
                        .add_enabled(changed, egui::Button::new(t("Reset")).min_size(egui::vec2(100.0, 28.0)))
                        .clicked()
                    {
                        self.reset_settings();
                    }
                });
                ui.add_space(16.0);

                // Maintenance.
                let mut action: Option<&'static str> = None;
                section(ui, t("Maintenance"), &mut |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(t("Check health")).clicked() {
                            action = Some("health");
                        }
                        if ui.button(t("Back up metadata")).clicked() {
                            action = Some("backup");
                        }
                        if ui
                            .button(egui::RichText::new(t("Clear the index")).color(t_.danger))
                            .on_hover_text(t(
                                "Forgets every indexed document and vector; the folders stay and are re-read on the next check",
                            ))
                            .clicked()
                        {
                            action = Some("clear");
                        }
                    });
                });
                match action {
                    Some("health") => match self.library.check_health() {
                        Ok(message) => self.say(message),
                        Err(message) => self.say_error(message),
                    },
                    Some("backup") => match self.library.create_backup() {
                        Ok(path) => self.say(tf(
                            "Backup written to {path}",
                            &[("path", &path.display().to_string())],
                        )),
                        Err(message) => self.say_error(message),
                    },
                    Some("clear") => match self.library.clear() {
                        Ok(()) => self.say(t("Index cleared")),
                        Err(message) => self.say_error(message),
                    },
                    _ => {}
                }
            });
    }

    // ---- windows -------------------------------------------------------------------------------

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let t_ = Tokens::get(ctx);
        let mut open = self.about_open;
        egui::Window::new(tf("About {app}", &[("app", services::APP_NAME)]))
            .id(egui::Id::new("recall_about"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);
                    let lockup = if t_.dark() { LOCKUP_WHITE } else { LOCKUP };
                    ui.add(
                        egui::Image::new(lockup)
                            .max_height(40.0)
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(8.0);
                    ui.label(tf(
                        "Version {version}",
                        &[("version", env!("CARGO_PKG_VERSION"))],
                    ));
                    ui.add_space(8.0);
                    ui.label(t(
                        "A local memory and embedding-index layer for the Redrob apps, with a thin window.",
                    ));
                    ui.label(
                        egui::RichText::new(t(
                            "Apache-2.0. Third-party notices: NOTICE, THIRD_PARTY_NOTICES.md, UPSTREAM_NOTICES.md.",
                        ))
                        .color(t_.text_faint),
                    );
                    if ui.link(services::REPOSITORY).clicked() {
                        services::open_url(services::REPOSITORY);
                    }
                    ui.add_space(12.0);
                });
            });
        self.about_open = open;
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let mut open = self.shortcuts_open;
        egui::Window::new(t("Shortcuts"))
            .id(egui::Id::new("recall_shortcuts"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(420.0);
                let rows: Vec<(String, String)> = View::ALL
                    .iter()
                    .map(|view| {
                        (
                            ctx.format_shortcut(&view.shortcut()),
                            tf("Switch to {view}", &[("view", view.name())]),
                        )
                    })
                    .chain([
                        (
                            ctx.format_shortcut(&FOCUS_SEARCH_SHORTCUT),
                            t("Focus the search box").to_owned(),
                        ),
                        (
                            ctx.format_shortcut(&ADD_FOLDER_SHORTCUT),
                            t("Add folder").to_owned(),
                        ),
                        (
                            ctx.format_shortcut(&MESSAGES_SHORTCUT),
                            t("Toggle the messages panel").to_owned(),
                        ),
                        ("↑ / ↓".to_owned(), t("Move between results").to_owned()),
                        ("Enter".to_owned(), t("Open the selected result").to_owned()),
                        (
                            ctx.format_shortcut(&egui::KeyboardShortcut::new(
                                egui::Modifiers::COMMAND,
                                egui::Key::Enter,
                            )),
                            t("Reveal the selected result in the file manager").to_owned(),
                        ),
                        ("Esc".to_owned(), t("Clear the search").to_owned()),
                    ])
                    .collect();
                egui::Grid::new("recall_shortcuts_grid")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for (keys, what) in rows {
                            ui.label(egui::RichText::new(keys).monospace());
                            ui.label(what);
                            ui.end_row();
                        }
                    });
            });
        self.shortcuts_open = open;
    }
}

impl eframe::App for ShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        i18n::set_current(self.language.effective());
        self.sync_theme(&ctx);
        self.refresh();
        self.drain_jobs();
        self.poll_search_debounce(&ctx);
        // The shared sign-in can change under us (a sign-in or sign-out in Redrob Launcher): the
        // watcher says when; a slow timer covers a directory that did not exist when it started,
        // and re-reads the launcher's language file for a window drawn in its language.
        if self.account_watcher.take() {
            self.account = account::current();
            self.account_checked = Instant::now();
            if self.language.refresh_fallback() {
                i18n::set_current(self.language.effective());
            }
        } else if self.account_checked.elapsed() > Duration::from_secs(30) {
            self.account_watcher.rearm();
            self.account = account::current();
            self.account_checked = Instant::now();
        }
        self.handle_shortcuts(&ctx);
        self.handle_drops(&ctx);

        self.top_bar(ui);
        self.status_line(ui);
        self.library_panel(ui);
        self.messages_dock(ui);
        let t_ = Tokens::get(&ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(t_.canvas)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| match self.view {
                View::Library => self.library_view(ui),
                View::Search => self.search_view(ui, &ctx),
                View::Ask => self.ask_view(ui, &ctx),
                View::Settings => self.settings_view(ui, &ctx),
            });
        self.about_window(&ctx);
        self.shortcuts_window(&ctx);
        self.drop_overlay(&ctx);
    }
}

// ---- widgets ------------------------------------------------------------------------------------

/// One row of the library tree: a state dot, the name, the detail line; the full path on hover.
fn tree_row(
    ui: &mut egui::Ui,
    t_: &Tokens,
    selected: bool,
    name: &str,
    detail: &str,
    dot: egui::Color32,
    full_path: Option<&str>,
) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), 38.0);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(
                rect,
                egui::CornerRadius::same(t_.radius_sm as u8),
                t_.row_selected,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, egui::CornerRadius::same(t_.radius_sm as u8), t_.hover);
        }
        painter.circle_filled(egui::pos2(rect.left() + 12.0, rect.center().y), 4.0, dot);
        let text_left = rect.left() + 24.0;
        let width = (rect.right() - 8.0 - text_left).max(10.0);
        let mut name_job = egui::text::LayoutJob::simple_singleline(
            name.to_owned(),
            theme::medium(12.5),
            t_.text,
        );
        name_job.wrap.max_width = width;
        name_job.wrap.max_rows = 1;
        name_job.wrap.break_anywhere = true;
        let name_galley = painter.layout_job(name_job);
        painter.galley(egui::pos2(text_left, rect.top() + 4.0), name_galley, t_.text);
        let mut detail_job = egui::text::LayoutJob::simple_singleline(
            detail.to_owned(),
            egui::FontId::proportional(10.5),
            t_.text_faint,
        );
        detail_job.wrap.max_width = width;
        detail_job.wrap.max_rows = 1;
        detail_job.wrap.break_anywhere = true;
        let detail_galley = painter.layout_job(detail_job);
        painter.galley(
            egui::pos2(text_left, rect.top() + 21.0),
            detail_galley,
            t_.text_faint,
        );
    }
    match full_path {
        Some(path) => response.on_hover_text(path),
        None => response,
    }
}

/// The dot colour for a folder's (or the library's) state.
fn folder_dot(t_: &Tokens, status: IndexStatus, failed: u64, present: bool) -> egui::Color32 {
    if !present {
        t_.danger
    } else {
        match status {
            IndexStatus::Indexing | IndexStatus::Scanning => t_.accent,
            IndexStatus::Paused => t_.warning,
            IndexStatus::Error => t_.danger,
            IndexStatus::Idle if failed > 0 => t_.warning,
            IndexStatus::Idle => t_.success,
        }
    }
}

/// The last component of a path, or the path itself (a drive root).
fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.to_owned())
}

/// A path on one line, ellipsized to the width, the whole path on hover.
fn path_label(ui: &mut egui::Ui, t_: &Tokens, path: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(path)
                .font(egui::FontId::monospace(11.0))
                .color(t_.text_faint),
        )
        .truncate(),
    )
    .on_hover_text(path);
}

/// The keyword snippet with the matched words in the accent colour, or the head of the chunk.
fn snippet_label(ui: &mut egui::Ui, t_: &Tokens, result: &SearchResult) {
    let Some(snippet) = &result.snippet else {
        ui.label(egui::RichText::new(work::plain_snippet(result)).color(t_.text_dim));
        return;
    };
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat {
        color: t_.text_dim,
        ..Default::default()
    };
    let bold = egui::TextFormat {
        font_id: theme::semibold(12.5),
        color: t_.accent,
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

/// The answer with its `[n]` citations as buttons that flow with the text. Returns the citation
/// clicked this frame.
fn answer_with_citations(ui: &mut egui::Ui, t_: &Tokens, answer: &str) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for piece in split_citations(answer) {
            match piece {
                Piece::Text(text) => {
                    ui.label(egui::RichText::new(text).color(t_.text));
                }
                Piece::Citation(n) => {
                    let button = egui::Button::new(
                        egui::RichText::new(format!("[{n}]"))
                            .font(theme::semibold(11.0))
                            .color(t_.accent),
                    )
                    .frame(false);
                    if ui
                        .add(button)
                        .on_hover_text(tf(
                            "Jump to source {n} and open the file",
                            &[("n", &n.to_string())],
                        ))
                        .clicked()
                    {
                        clicked = Some(n);
                    }
                }
            }
        }
    });
    clicked
}

enum Piece<'a> {
    Text(&'a str),
    Citation(usize),
}

/// Split an answer at its `[n]` and `[n, m]` citations.
fn split_citations(answer: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = answer;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let inside = &after[..end];
        let numbers: Vec<usize> = inside
            .split(',')
            .map(str::trim)
            .filter_map(|n| n.parse::<usize>().ok())
            .collect();
        if numbers.is_empty()
            || numbers.len() != inside.split(',').count()
            || numbers.iter().any(|n| *n == 0 || *n > 99)
        {
            // Not a citation: keep the bracket as text and move past it.
            pieces.push(Piece::Text(&rest[..start + 1]));
            rest = after;
            continue;
        }
        if start > 0 {
            pieces.push(Piece::Text(&rest[..start]));
        }
        for n in numbers {
            pieces.push(Piece::Citation(n));
        }
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    pieces
}

fn sources_list(
    ui: &mut egui::Ui,
    t_: &Tokens,
    sources: &[AnswerSource],
    highlight: Option<usize>,
    scroll_to: Option<usize>,
    open: &mut Option<String>,
    reveal: &mut Option<String>,
) {
    for source in sources {
        let lit = highlight == Some(source.number);
        let frame = theme::card_frame(ui);
        let frame = if lit {
            frame
                .fill(t_.row_selected)
                .stroke(egui::Stroke::new(1.0, t_.accent_border))
        } else {
            frame
        };
        let response = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("[{}]", source.number))
                            .font(theme::semibold(12.5))
                            .color(t_.accent),
                    );
                    ui.label(egui::RichText::new(&source.name).font(theme::semibold(13.0)));
                    if let Some(page) = source.page {
                        ui.label(
                            egui::RichText::new(tf("p. {page}", &[("page", &page.to_string())]))
                                .color(t_.text_faint),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t("Reveal")).clicked() {
                            *reveal = Some(source.path.clone());
                        }
                        if ui.small_button(t("Open")).clicked() {
                            *open = Some(source.path.clone());
                        }
                    });
                });
                path_label(ui, t_, &source.path);
                ui.label(
                    egui::RichText::new(
                        work::strip_bold(&source.excerpt)
                            .chars()
                            .take(400)
                            .collect::<String>(),
                    )
                    .color(t_.text_dim),
                );
            })
            .response;
        if scroll_to == Some(source.number) {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        ui.add_space(4.0);
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
