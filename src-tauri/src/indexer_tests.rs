//! The indexer, driven the way the app drives it: real files on disk, the real `run_full_index`, the
//! real `search`. Nothing is mocked except the window (there is none to emit to).
//!
//! Each fixture carries a phrase that appears in no other fixture, so a hit can only come from that
//! file having been extracted, chunked and stored.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::models::{SearchFilters, SearchRequest};
use crate::state::AppState;

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
    let stale = |state: &AppState| {
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
            .count()
    };
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
    assert_eq!(
        stale(&state),
        0,
        "the run after the model became available must embed every file"
    );
}

/// A folder added while a pass is running gets indexed by that run, not left until the next manual
/// check. This was the shape of "I added a folder and nothing got indexed": the second request was
/// answered "already indexing" and dropped.
#[test]
fn a_folder_added_during_a_pass_is_indexed_when_the_pass_finishes() {
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
