// SPDX-License-Identifier: Apache-2.0
//! One interface language for the window: English (the source) and Korean.
//!
//! Precedence, as redrob-launcher's `docs/shared-language.md` asks of every app: the person's own
//! choice in this app (Help > Language or Settings, kept in `ui.json` beside the library), else the
//! launcher's shared `language.json` (`apps["redrob-recall"]`, then `language`), else the system's
//! preferred languages (`REDROB_RECALL_LOCALE` overrides them for screenshots). The shell sets the
//! language once per frame ([`set_current`]) and draws every string through [`t`] / [`tf`].

use std::cell::Cell;

/// `REDROB_RECALL_LOCALE=ko-KR`: the system language, for tests and screenshots.
pub const LOCALE_ENV: &str = "REDROB_RECALL_LOCALE";
/// The preference value that follows the launcher's file and the system.
pub const AUTO: &str = "auto";
/// This app's id in the shared file's `apps` map.
pub const APP_ID: &str = "redrob-recall";

#[derive(Debug, PartialEq, Eq)]
pub struct LangInfo {
    pub code: &'static str,
    /// The name in that language, for the menu.
    pub name: &'static str,
}

pub const LANGUAGES: &[LangInfo] = &[
    LangInfo {
        code: "en",
        name: "English",
    },
    LangInfo {
        code: "ko",
        name: "한국어",
    },
];

pub fn by_code(code: &str) -> Option<&'static LangInfo> {
    LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code))
}

pub fn english() -> &'static LangInfo {
    &LANGUAGES[0]
}

/// The language for a locale tag such as `ko_KR.UTF-8`, `ko-KR` or `en`; `None` for one this app
/// has no catalog for, and for `C` / `POSIX`, which state no preference.
pub fn from_tag(tag: &str) -> Option<&'static LangInfo> {
    let base = tag
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .trim()
        .replace('_', "-")
        .to_ascii_lowercase();
    let primary = base.split('-').next().unwrap_or("");
    if primary.is_empty() || matches!(primary, "c" | "posix") {
        return None;
    }
    by_code(primary)
}

/// The first supported language among `tags` (most preferred first); English when none is.
pub fn from_tags<S: AsRef<str>>(tags: &[S]) -> &'static LangInfo {
    tags.iter()
        .filter_map(|t| from_tag(t.as_ref()))
        .next()
        .unwrap_or_else(english)
}

/// The system's language: `REDROB_RECALL_LOCALE`, else the OS preferred-languages list.
pub fn system() -> &'static LangInfo {
    if let Ok(tag) = std::env::var(LOCALE_ENV)
        && !tag.trim().is_empty()
    {
        return from_tags(&[tag]);
    }
    let tags: Vec<String> = std::panic::catch_unwind(|| sys_locale::get_locales().take(64).collect())
        .unwrap_or_default();
    from_tags(&tags)
}

/// The launcher's shared `language.json` beside its `account.json`: `apps["redrob-recall"]`, else
/// `language`; `None` when the file is missing, unreadable or names a language this app lacks.
pub fn shared() -> Option<&'static LangInfo> {
    let path = crate::account::shared_account_path()?.with_file_name("language.json");
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let per_app = value
        .get("apps")
        .and_then(|apps| apps.get(APP_ID))
        .and_then(serde_json::Value::as_str)
        .and_then(by_code);
    per_app.or_else(|| {
        value
            .get("language")
            .and_then(serde_json::Value::as_str)
            .and_then(by_code)
    })
}

/// The language setting: the person's own preference (`auto` or a code) and what `auto` resolves to.
#[derive(Debug)]
pub struct Language {
    pref: String,
    fallback: &'static LangInfo,
}

impl Language {
    /// From the preference (`auto` or a code) and the launcher's file / the system.
    pub fn new(pref: &str) -> Self {
        let pref = if pref.eq_ignore_ascii_case(AUTO) || by_code(pref).is_some() {
            pref.to_ascii_lowercase()
        } else {
            AUTO.to_owned()
        };
        Self {
            pref,
            fallback: shared().unwrap_or_else(system),
        }
    }

    /// The language the window is drawn in.
    pub fn effective(&self) -> &'static LangInfo {
        if self.is_auto() {
            self.fallback
        } else {
            by_code(&self.pref).unwrap_or(self.fallback)
        }
    }

    pub fn pref(&self) -> &str {
        &self.pref
    }

    pub fn is_auto(&self) -> bool {
        self.pref.eq_ignore_ascii_case(AUTO)
    }

    /// What `auto` means right now (the launcher's choice or the system's).
    pub fn fallback(&self) -> &'static LangInfo {
        self.fallback
    }

    /// Set the preference; returns whether the effective language changed.
    pub fn set(&mut self, pref: &str) -> bool {
        let before = self.effective();
        self.pref = if pref.eq_ignore_ascii_case(AUTO) || by_code(pref).is_some() {
            pref.to_ascii_lowercase()
        } else {
            AUTO.to_owned()
        };
        before != self.effective()
    }

    /// Re-read the launcher's file and the system (the window regained focus).
    pub fn refresh_fallback(&mut self) -> bool {
        let before = self.effective();
        self.fallback = shared().unwrap_or_else(system);
        before != self.effective()
    }
}

// ---- the strings -------------------------------------------------------------------------------

thread_local! {
    static CURRENT: Cell<&'static LangInfo> = const { Cell::new(&LANGUAGES[0]) };
}

/// Set the language the strings are drawn in (once per frame, by the shell).
pub fn set_current(lang: &'static LangInfo) {
    CURRENT.set(lang);
}

pub fn current() -> &'static LangInfo {
    CURRENT.get()
}

/// Translate an English string into the current language; unknown strings come back unchanged.
pub fn t(s: &str) -> &str {
    tr(current().code, s)
}

pub fn tr<'a>(code: &str, s: &'a str) -> &'a str {
    match code {
        "ko" => KO.iter().find(|(en, _)| *en == s).map_or(s, |(_, ko)| ko),
        _ => s,
    }
}

/// `{name}` placeholders in a translated template.
pub fn fmt(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// [`t`] then [`fmt`].
pub fn tf(s: &str, args: &[(&str, &str)]) -> String {
    fmt(t(s), args)
}

/// The Korean strings (English is the source).
const KO: &[(&str, &str)] = &[
    // Views.
    ("Library", "라이브러리"),
    ("Search", "검색"),
    ("Ask", "질문"),
    ("Settings", "설정"),
    // Top bar and menus.
    ("Help", "도움말"),
    ("Add folder…", "폴더 추가…"),
    ("Add folder", "폴더 추가"),
    ("Check now", "지금 확인"),
    ("Pause indexing", "색인 일시 중지"),
    ("Resume indexing", "색인 다시 시작"),
    ("Pause", "일시 중지"),
    ("Resume", "다시 시작"),
    ("Language", "언어"),
    ("Theme", "테마"),
    ("System", "시스템"),
    ("Light", "밝게"),
    ("Dark", "어둡게"),
    ("System ({name})", "시스템 ({name})"),
    (
        "Follow Redrob Launcher's language, else the operating system's",
        "Redrob Launcher의 언어를, 없으면 운영 체제의 언어를 따릅니다",
    ),
    ("Follow the operating system's appearance", "운영 체제의 모양을 따릅니다"),
    ("Shortcuts", "단축키"),
    ("About {app}", "{app} 정보"),
    ("Open the repository", "저장소 열기"),
    ("Messages", "메시지"),
    ("Show messages", "메시지 표시"),
    ("Hide messages", "메시지 숨기기"),
    ("No messages", "메시지가 없습니다"),
    ("Clear", "지우기"),
    ("Hide", "숨기기"),
    ("Signed in with Redrob Launcher", "Redrob Launcher로 로그인됨"),
    ("Not signed in", "로그인되지 않음"),
    ("Connect", "연결"),
    (
        "Sign in once in Redrob Launcher; every Redrob app on this computer uses that account",
        "Redrob Launcher에서 한 번 로그인하면 이 컴퓨터의 모든 Redrob 앱이 그 계정을 사용합니다",
    ),
    ("Version {version}", "버전 {version}"),
    (
        "A local memory and embedding-index layer for the Redrob apps, with a thin window.",
        "Redrob 앱을 위한 로컬 메모리와 임베딩 색인 계층, 그리고 얇은 창.",
    ),
    (
        "Apache-2.0. Third-party notices: NOTICE, THIRD_PARTY_NOTICES.md, UPSTREAM_NOTICES.md.",
        "Apache-2.0. 서드파티 고지: NOTICE, THIRD_PARTY_NOTICES.md, UPSTREAM_NOTICES.md.",
    ),
    // Shortcuts window.
    ("Switch to {view}", "{view}로 전환"),
    ("Focus the search box", "검색창에 포커스"),
    ("Toggle the messages panel", "메시지 패널 전환"),
    ("Open the selected result", "선택한 결과 열기"),
    ("Reveal the selected result in the file manager", "선택한 결과를 파일 관리자에서 표시"),
    ("Clear the search", "검색 지우기"),
    ("Move between results", "결과 사이 이동"),
    // Library panel.
    ("All folders", "모든 폴더"),
    ("{n} documents", "문서 {n}개"),
    ("{n} documents, {m} failed", "문서 {n}개, 실패 {m}개"),
    ("Missing", "없음"),
    ("Indexing", "색인 중"),
    ("Scanning", "스캔 중"),
    ("Paused", "일시 중지됨"),
    ("Up to date", "최신 상태"),
    ("Error", "오류"),
    ("Ready", "준비됨"),
    ("No folders yet", "아직 폴더가 없습니다"),
    (
        "Add a folder and Recall indexes it on this machine. Nothing leaves it.",
        "폴더를 추가하면 Recall이 이 컴퓨터에서 색인합니다. 아무것도 밖으로 나가지 않습니다.",
    ),
    ("or drop a folder onto this window", "또는 폴더를 이 창에 끌어다 놓으세요"),
    ("Drop to add the folder", "놓으면 폴더가 추가됩니다"),
    ("Added {path}", "{path} 추가됨"),
    ("Removed {path}", "{path} 제거됨"),
    ("Remove", "제거"),
    ("Reveal", "표시"),
    ("Open", "열기"),
    ("Wait for indexing to finish", "색인이 끝날 때까지 기다리세요"),
    ("Re-reading your library", "라이브러리를 다시 읽는 중"),
    ("Queued behind the running pass", "실행 중인 작업 뒤에 예약됨"),
    ("Last indexed {when}", "마지막 색인 {when}"),
    ("{docs} documents, {passages} passages, {size}", "문서 {docs}개, 구절 {passages}개, {size}"),
    ("{n} files could not be read", "파일 {n}개를 읽을 수 없습니다"),
    ("Show", "보기"),
    ("Folder", "폴더"),
    (
        "This folder is missing; its records are kept until it returns.",
        "이 폴더가 없습니다. 돌아올 때까지 기록은 보존됩니다.",
    ),
    ("{done} / {total}", "{done} / {total}"),
    ("{done} / {total}  {file}", "{done} / {total}  {file}"),
    ("Indexing paused", "색인 일시 중지됨"),
    ("The library could not be read: {error}", "라이브러리를 읽을 수 없습니다: {error}"),
    ("{path} is not a folder", "{path}은(는) 폴더가 아닙니다"),
    ("Files that could not be read", "읽을 수 없는 파일"),
    // Search.
    (
        "Search your files: words, \"a phrase\", -excluded, heading:word",
        "파일 검색: 단어, \"구절\", -제외어, heading:단어",
    ),
    ("pdf, md", "pdf, md"),
    ("Only these file types (comma separated)", "이 파일 형식만 (쉼표로 구분)"),
    ("Searching…", "검색 중…"),
    ("{n} results for {query}", "{query} 검색 결과 {n}개"),
    ("No results for {query}", "{query} 검색 결과가 없습니다"),
    ("in {folder}", "{folder} 안에서"),
    ("Ask about this", "이것에 대해 질문"),
    ("p. {page}", "{page}쪽"),
    ("Type to search; Enter opens, Ctrl+Enter reveals, Esc clears.", "입력하면 검색됩니다. Enter 열기, Ctrl+Enter 표시, Esc 지우기."),
    // Ask.
    ("Ask a question about your files", "파일에 대해 질문하세요"),
    (
        "Answers come from the Redrob console as {account}; only the matching passages are sent.",
        "답변은 {account}로 Redrob 콘솔에서 옵니다. 일치하는 구절만 전송됩니다.",
    ),
    (
        "Sign in with Redrob Launcher to ask; search works without it.",
        "질문하려면 Redrob Launcher로 로그인하세요. 검색은 로그인 없이도 됩니다.",
    ),
    ("Get Redrob Launcher", "Redrob Launcher 받기"),
    ("Reading your files and asking Redrob", "파일을 읽고 Redrob에 묻는 중"),
    ("Sources", "출처"),
    ("Sources found locally", "로컬에서 찾은 출처"),
    ("{model} in {secs}s", "{model}, {secs}초"),
    ("{model} in {secs}s, {input} in / {output} out tokens", "{model}, {secs}초, 토큰 입력 {input} / 출력 {output}"),
    ("Jump to source {n} and open the file", "출처 {n}으로 이동하고 파일 열기"),
    // Settings.
    ("Redrob account", "Redrob 계정"),
    ("Signed in as {account}", "{account}로 로그인됨"),
    ("Console: {url}", "콘솔: {url}"),
    (
        "Sign out in Redrob Launcher; this app never writes the shared file.",
        "로그아웃은 Redrob Launcher에서 하세요. 이 앱은 공유 파일을 쓰지 않습니다.",
    ),
    (
        "Not signed in. Sign in once in Redrob Launcher and every Redrob app on this computer, Recall included, uses that account.",
        "로그인되지 않았습니다. Redrob Launcher에서 한 번 로그인하면 Recall을 포함한 이 컴퓨터의 모든 Redrob 앱이 그 계정을 사용합니다.",
    ),
    ("Open Redrob Launcher", "Redrob Launcher 열기"),
    ("Check again", "다시 확인"),
    ("Shared account file: {path}", "공유 계정 파일: {path}"),
    ("Answer model", "답변 모델"),
    ("auto lets the console choose", "auto는 콘솔이 고릅니다"),
    (
        "Send the matching passages to Redrob when asking",
        "질문할 때 일치하는 구절을 Redrob에 전송",
    ),
    ("Index", "색인"),
    ("Embedding model", "임베딩 모델"),
    (
        "multilingual-e5-small (fastembed), downloaded once into the data folder and run on this machine",
        "multilingual-e5-small (fastembed). 데이터 폴더에 한 번 내려받아 이 컴퓨터에서 실행합니다",
    ),
    ("File types", "파일 형식"),
    ("Skip folders named", "건너뛸 폴더 이름"),
    ("Largest file (MB)", "최대 파일 크기 (MB)"),
    ("Passage size", "구절 크기"),
    ("Overlap", "겹침"),
    ("Library location", "라이브러리 위치"),
    (
        "Where the index, the embedding model and the local API token live. REDROB_RECALL_DATA_DIR moves it.",
        "색인, 임베딩 모델, 로컬 API 토큰이 있는 곳. REDROB_RECALL_DATA_DIR로 옮길 수 있습니다.",
    ),
    ("Local API", "로컬 API"),
    (
        "What the other Redrob apps call to search this library: 127.0.0.1 only, one bearer token (docs/API.md).",
        "다른 Redrob 앱이 이 라이브러리를 검색할 때 호출하는 것: 127.0.0.1 전용, 베어러 토큰 하나 (docs/API.md).",
    ),
    ("Serve the local API", "로컬 API 제공"),
    ("Port", "포트"),
    (
        "Token: {path} (never shown here). Changes to the API take effect at the next start.",
        "토큰: {path} (여기에 표시되지 않음). API 변경은 다음 시작 때 적용됩니다.",
    ),
    ("Appearance", "모양"),
    ("Apply", "적용"),
    ("Reset", "되돌리기"),
    ("Settings saved", "설정이 저장되었습니다"),
    ("Maintenance", "유지 관리"),
    ("Check health", "상태 점검"),
    ("Back up metadata", "메타데이터 백업"),
    ("Backup written to {path}", "백업을 {path}에 썼습니다"),
    ("Clear the index", "색인 지우기"),
    (
        "Forgets every indexed document and vector; the folders stay and are re-read on the next check",
        "색인된 모든 문서와 벡터를 잊습니다. 폴더는 남고 다음 확인 때 다시 읽습니다",
    ),
    ("Index cleared", "색인을 지웠습니다"),
    ("Add a folder to your library", "라이브러리에 폴더 추가"),
];
