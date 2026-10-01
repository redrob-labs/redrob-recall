//! The indexer, driven the way the app drives it: real files on disk, the real `run_full_index`, the
//! real `search`. Nothing is mocked except the window (there is none to emit to).
//!
//! Each fixture carries a phrase that appears in no other fixture, so a hit can only come from that
//! file having been extracted, chunked and stored.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::models::{SearchFilters, SearchRequest};
use crate::state::AppState;

/// Held by every test that loads the real embedding model. The app builds one embedder per process;
/// these tests built up to four at once, and on CI runners (never locally, 12 runs) one of them left
/// a file or two without vectors. Loading one at a time is what the app does anyway.
static MODEL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn model_lock() -> std::sync::MutexGuard<'static, ()> {
    MODEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// (file, a word only that file contains)
const FIXTURES: &[(&str, &str)] = &[
    ("notes.md", "marmalade"),
    ("memo.txt", "quartzite"),
    ("table.csv", "hydrangea"),
    ("page.html", "tortoise"),
    ("minutes.docx", "saffron"),
    ("report.pdf", "juniper"),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library")
}

/// A data directory of its own, so a test never reads or writes the user's `~/.redrob/recall`.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "redrob-recall-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn state_indexing(scratch: &Scratch, library: &Path) -> AppState {
    let state = AppState::open_headless(scratch.0.join("data")).unwrap();
    let mut settings = state.settings();
    settings.library_paths = vec![library.to_string_lossy().into_owned()];
    state.update_settings(settings).unwrap();
    state
}

fn hits(state: &AppState, word: &str) -> Vec<String> {
    crate::search::search(
        state,
        SearchRequest {
            query: word.into(),
            limit: 10,
            filters: SearchFilters::default(),
        },
    )
    .unwrap()
    .into_iter()
    .map(|result| result.name)
    .collect()
}

/// Every supported format is found by its own phrase after a full index.
///
/// Needs the embedding model, which fastembed downloads on first use; the download is cached under
/// `target/` so it is paid once per checkout, not once per run.
#[test]
fn every_format_is_searchable_after_a_full_index() {
    let _model = model_lock();
    let scratch = Scratch::new("formats");
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-model-cache");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&models, scratch.0.join("data/models")).unwrap();

    let state = state_indexing(&scratch, &fixtures());
    super::run_full_index(&state).unwrap();

    let mut missing = Vec::new();
    for (file, word) in FIXTURES {
        let found = hits(&state, word);
        if !found.iter().any(|name| name == file) {
            missing.push(format!("{file} (searched {word:?}, got {found:?})"));
        }
    }
    assert!(
        missing.is_empty(),
        "not searchable after indexing: {missing:#?}"
    );
}

/// The first run with no embedding model -- offline, or the model host blocked -- must still leave
/// the library keyword-searchable. Search already falls back to keywords when vectors are
/// unavailable; indexing has to keep the keywords for that fallback to have anything to find.
#[test]
fn library_stays_keyword_searchable_when_the_embedding_model_is_unavailable() {
    let _model = model_lock();
    let scratch = Scratch::new("offline");
    // A file where the model cache directory should be: fastembed cannot create its cache, so
    // loading the model fails the same way an unreachable download does, with no network involved.
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    std::fs::write(scratch.0.join("data/models"), b"not a directory").unwrap();

    let state = state_indexing(&scratch, &fixtures());
    super::run_full_index(&state).unwrap();

    let mut missing = Vec::new();
    for (file, word) in FIXTURES {
        let found = hits(&state, word);
        if !found.iter().any(|name| name == file) {
            missing.push(format!("{file} (searched {word:?}, got {found:?})"));
        }
    }
    assert!(
        missing.is_empty(),
        "lost to a missing embedding model: {missing:#?}"
    );

    // Leaving vectors `pending` is only half the fix: the next run, with the model reachable, has to
    // pick those files up again rather than treat them as done.
    let stale_files = |state: &AppState| {
        FIXTURES
            .iter()
            .filter(|(file, _)| {
                let path = fixtures().join(file);
                let metadata = std::fs::metadata(&path).unwrap();
                let modified = super::system_time_string(metadata.modified().unwrap());
                !state
                    .storage()
                    .document_is_current(&path.to_string_lossy(), &modified, metadata.len())
                    .unwrap()
            })
            .map(|(file, _)| *file)
            .collect::<Vec<_>>()
    };
    let stale = |state: &AppState| stale_files(state).len();
    assert_eq!(
        stale(&state),
        FIXTURES.len(),
        "files without vectors must stay due for re-indexing"
    );
    drop(state);

    std::fs::remove_file(scratch.0.join("data/models")).unwrap();
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-model-cache");
    std::fs::create_dir_all(&models).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&models, scratch.0.join("data/models")).unwrap();
    let state = AppState::open_headless(scratch.0.join("data")).unwrap();
    super::run_full_index(&state).unwrap();
    // Failed once on a CI runner and never locally (12 runs, including pinned to one CPU), so when
    // it fails it says which file and whether the model can be loaded at all at that moment.
    let left = stale_files(&state);
    assert!(
        left.is_empty(),
        "the run after the model became available left {left:?} without vectors; \
         the model {}",
        match state.embedder().embed_query("probe") {
            Ok(_) => "loads now".to_string(),
            Err(error) => format!("does not load: {error:#}"),
        }
    );
}

/// A folder added while a pass is running gets indexed by that run, not left until the next manual
/// check. This was the shape of "I added a folder and nothing got indexed": the second request was
/// answered "already indexing" and dropped.
#[test]
fn a_folder_added_during_a_pass_is_indexed_when_the_pass_finishes() {
    let _model = model_lock();
    let scratch = Scratch::new("midpass");
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-model-cache");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&models, scratch.0.join("data/models")).unwrap();

    // Second folder, with a word the fixtures do not contain.
    let late = scratch.0.join("late");
    std::fs::create_dir_all(&late).unwrap();
    std::fs::write(
        late.join("late.md"),
        "The periwinkle zeppelin docked at noon.\n",
    )
    .unwrap();

    let state = state_indexing(&scratch, &fixtures());
    assert!(
        super::request_index(&state),
        "nothing was running, so a pass must start"
    );

    // Wait until the pass is past its settings read -- it is on a file -- so the folder below
    // cannot be picked up by this pass and only the queued re-run can find it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while state.snapshot().unwrap().stats.current_file.is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first pass never reached a file"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut settings = state.settings();
    settings
        .library_paths
        .push(late.to_string_lossy().into_owned());
    state.update_settings(settings).unwrap();
    assert!(
        !super::request_index(&state),
        "a pass is running, so this request must be queued"
    );

    while state.is_indexing() {
        assert!(
            std::time::Instant::now() < deadline,
            "indexing never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        hits(&state, "periwinkle")
            .iter()
            .any(|name| name == "late.md"),
        "the folder added mid-pass was never indexed"
    );
}

/// Each way Add folder can refuse says why, in words the user can act on -- and refuses before
/// anything is saved, so a refused path never shows up in the library.
#[test]
fn add_folder_refuses_bad_paths_with_a_reason_and_saves_nothing() {
    let scratch = Scratch::new("refusals");
    // Adding a folder starts indexing; keep it off the network by making the model unloadable.
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    std::fs::write(scratch.0.join("data/models"), b"not a directory").unwrap();
    let state = AppState::open_headless(scratch.0.join("data")).unwrap();
    let add = |path: &Path| crate::commands::add_library_folder(&state, &path.to_string_lossy());

    let missing = scratch.0.join("no-such-folder");
    assert!(add(&missing).unwrap_err().ends_with("does not exist"));

    let file = fixtures().join("memo.txt");
    assert!(add(&file).unwrap_err().contains("is a file"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = scratch.0.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads anything, so this case only means something for an ordinary user.
        if std::fs::read_dir(&locked).is_err() {
            assert!(add(&locked).unwrap_err().contains("not allowed to read"));
        }
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    assert!(
        state.settings().library_paths.is_empty(),
        "a refused path was saved"
    );

    add(&fixtures()).unwrap();
    assert!(add(&fixtures())
        .unwrap_err()
        .ends_with("is already in your library"));
    assert_eq!(state.settings().library_paths.len(), 1);
    while state.is_indexing() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A folder the user added is still there after the app restarts.
#[test]
fn library_folders_survive_a_restart() {
    let scratch = Scratch::new("restart");
    // Adding a folder starts indexing; keep it off the network by making the model unloadable.
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    std::fs::write(scratch.0.join("data/models"), b"not a directory").unwrap();
    let state = AppState::open_headless(scratch.0.join("data")).unwrap();
    crate::commands::add_library_folder(&state, &fixtures().to_string_lossy()).unwrap();
    while state.is_indexing() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    state.shutdown();
    drop(state);

    let reopened = AppState::open_headless(scratch.0.join("data")).unwrap();
    let expected = std::fs::canonicalize(fixtures()).unwrap();
    assert_eq!(
        reopened.settings().library_paths,
        vec![expected.to_string_lossy().into_owned()]
    );
}

/// Check now re-reads what changed and nothing else, and forgets what was deleted. Observed through
/// chunk ids: an unchanged file keeps its ids, a re-read file gets new ones.
#[test]
fn a_second_pass_rereads_only_changed_files_and_drops_deleted_ones() {
    let _model = model_lock();
    let scratch = Scratch::new("incremental");
    // The real model: without vectors every file stays due for re-indexing by design (see the
    // offline test), which would make "unchanged" unobservable.
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-model-cache");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::create_dir_all(scratch.0.join("data")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&models, scratch.0.join("data/models")).unwrap();
    let library = scratch.0.join("library");
    std::fs::create_dir_all(&library).unwrap();
    let kept = library.join("kept.md");
    let edited = library.join("edited.md");
    let deleted = library.join("deleted.md");
    std::fs::write(&kept, "The obsidian kettle stays the same.\n").unwrap();
    std::fs::write(&edited, "The first draft mentions a tangerine.\n").unwrap();
    std::fs::write(&deleted, "The vermilion compass will be thrown away.\n").unwrap();

    let state = state_indexing(&scratch, &library);
    super::run_full_index(&state).unwrap();
    let ids = |path: &Path| {
        state
            .storage()
            .chunk_ids_for_path(&path.to_string_lossy())
            .unwrap()
    };
    let kept_before = ids(&kept);
    let edited_before = ids(&edited);
    assert!(!kept_before.is_empty() && !edited_before.is_empty());

    // Different length as well as content, so the change is visible even on a filesystem whose
    // modification times are coarser than this test.
    std::fs::write(
        &edited,
        "The second draft replaces it with a pomegranate entirely.\n",
    )
    .unwrap();
    std::fs::remove_file(&deleted).unwrap();
    super::run_full_index(&state).unwrap();

    assert_eq!(ids(&kept), kept_before, "an unchanged file was re-read");
    assert_ne!(
        ids(&edited),
        edited_before,
        "an edited file was not re-read"
    );
    assert!(
        ids(&deleted).is_empty(),
        "a deleted file is still in the library"
    );
    assert!(hits(&state, "pomegranate").iter().any(|n| n == "edited.md"));
    // Absence is checked against the stored text, not ranked results: semantic search always
    // returns the nearest files, so "tangerine" still ranks edited.md even though the word is gone.
    let stored = |word: &str| state.storage().keyword_search(word, 10).unwrap().len();
    assert_eq!(
        stored("tangerine"),
        0,
        "the edited file's old text is still stored"
    );
    assert_eq!(
        stored("vermilion"),
        0,
        "the deleted file's text is still stored"
    );
    assert_eq!(stored("pomegranate"), 1);
}
