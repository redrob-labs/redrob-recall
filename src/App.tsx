import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import {
  Archive,
  ArrowRight,
  Bot,
  Check,
  ChevronRight,
  CircleHelp,
  Database,
  ExternalLink,
  File,
  FileCode2,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  KeyRound,
  Library,
  LoaderCircle,
  LockKeyhole,
  MessageSquareText,
  Plus,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Trash2,
  Unplug,
  X,
} from "lucide-react";
import { bridge } from "./lib/bridge";
import type {
  AppSettings,
  AppSnapshot,
  AskResponse,
  DocumentRecord,
  IndexProgress,
  SearchResult,
  View,
} from "./types";

type Toast = {
  id: number;
  message: string;
  tone: "success" | "error" | "info";
};

export default function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [progress, setProgress] = useState<IndexProgress | null>(null);
  const [view, setView] = useState<View>("search");
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [fatalError, setFatalError] = useState<string | null>(null);
  const toastCounter = useRef(0);

  const notify = useCallback(
    (message: string, tone: Toast["tone"] = "info") => {
      const id = ++toastCounter.current;
      setToasts((current) => [...current, { id, message, tone }]);
      window.setTimeout(
        () =>
          setToasts((current) => current.filter((toast) => toast.id !== id)),
        3500,
      );
    },
    [],
  );

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await bridge.snapshot());
      setFatalError(null);
    } catch (error) {
      setFatalError(readError(error));
    }
  }, []);

  useEffect(() => {
    void refresh();
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const register = async () => {
      try {
        const [unlistenSnapshot, unlistenProgress] = await Promise.all([
          bridge.onSnapshot(setSnapshot),
          bridge.onProgress(setProgress),
        ]);
        if (cancelled) {
          unlistenSnapshot();
          unlistenProgress();
        } else {
          unlisteners.push(unlistenSnapshot, unlistenProgress);
        }
      } catch (error) {
        if (!cancelled) setFatalError(readError(error));
      }
    };
    void register();
    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [refresh]);

  if (fatalError) {
    return <FatalState error={fatalError} retry={refresh} />;
  }
  if (!snapshot) {
    return <BootState />;
  }
  if (snapshot.settings.libraryPaths.length === 0) {
    return <Onboarding snapshot={snapshot} refresh={refresh} notify={notify} />;
  }

  return (
    <div className="app-shell">
      <Sidebar view={view} setView={setView} snapshot={snapshot} />
      <main className="workspace">
        {view === "search" && (
          <SearchView snapshot={snapshot} notify={notify} setView={setView} />
        )}
        {view === "ask" && <AskView snapshot={snapshot} notify={notify} />}
        {view === "sources" && (
          <SourcesView snapshot={snapshot} refresh={refresh} notify={notify} />
        )}
        {view === "settings" && (
          <SettingsView snapshot={snapshot} refresh={refresh} notify={notify} />
        )}
      </main>
      {progress && progress.status !== "idle" && (
        <ProgressBar progress={progress} />
      )}
      <ToastStack toasts={toasts} />
    </div>
  );
}

function Sidebar({
  view,
  setView,
  snapshot,
}: {
  view: View;
  setView: (view: View) => void;
  snapshot: AppSnapshot;
}) {
  const nav: { id: View; label: string; icon: typeof Search }[] = [
    { id: "search", label: "Search", icon: Search },
    { id: "ask", label: "Ask", icon: MessageSquareText },
    { id: "sources", label: "Sources", icon: Library },
  ];
  return (
    <aside className="sidebar">
      <div className="wordmark">
        <Logo />
        <span>VectorDB</span>
      </div>
      <nav className="nav-list" aria-label="Main navigation">
        {nav.map((item) => {
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              className={view === item.id ? "nav-item active" : "nav-item"}
              onClick={() => setView(item.id)}
            >
              <Icon size={18} />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>
      <div className="sidebar-spacer" />
      <div className="library-mini">
        <div className="library-mini-top">
          <span
            className={
              snapshot.stats.status === "indexing" ? "pulse-dot" : "status-dot"
            }
          />
          <span>{statusLabel(snapshot.stats.status)}</span>
        </div>
        <strong>{formatNumber(snapshot.stats.documents)} files</strong>
        <span>{formatBytes(snapshot.stats.totalBytes)} indexed locally</span>
      </div>
      <button
        className={view === "settings" ? "nav-item active" : "nav-item"}
        onClick={() => setView("settings")}
      >
        <Settings size={18} />
        <span>Settings</span>
      </button>
      <div className="privacy-foot">
        <ShieldCheck size={14} />
        <span>Files stay on this device</span>
      </div>
    </aside>
  );
}

function Onboarding({
  snapshot,
  refresh,
  notify,
}: {
  snapshot: AppSnapshot;
  refresh: () => Promise<void>;
  notify: Notify;
}) {
  const [adding, setAdding] = useState(false);
  const addFolder = async () => {
    try {
      setAdding(true);
      const path = await bridge.chooseFolder();
      if (!path) return;
      await bridge.addLibraryPath(path);
      await bridge.startIndexing();
      await refresh();
      notify("Folder added. Local indexing has started.", "success");
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setAdding(false);
    }
  };
  return (
    <div className="onboarding">
      <header className="onboarding-header">
        <div className="wordmark">
          <Logo />
          <span>VectorDB</span>
        </div>
        <span className="local-badge">
          <LockKeyhole size={14} /> Private by default
        </span>
      </header>
      <div className="onboarding-grid">
        <section className="onboarding-copy">
          <span className="eyebrow">YOUR FILES, FINALLY FINDABLE</span>
          <h1>Everything on your computer, searchable.</h1>
          <p>
            Choose the folders you want to remember. Redrob reads and indexes
            them on this device—your library is never uploaded.
          </p>
          <button
            className="primary large"
            onClick={() => void addFolder()}
            disabled={adding}
          >
            {adding ? (
              <LoaderCircle className="spin" size={19} />
            ) : (
              <FolderOpen size={19} />
            )}{" "}
            Choose folders
          </button>
          <div className="format-list">
            <span>PDF</span>
            <span>DOCX</span>
            <span>TXT</span>
            <span>Markdown</span>
            <span>HTML</span>
            <span>CSV</span>
          </div>
        </section>
        <section
          className="onboarding-visual"
          aria-label="How local search works"
        >
          <div className="visual-orbit orbit-one" />
          <div className="visual-orbit orbit-two" />
          <div className="visual-card source-card one">
            <FileText size={20} />
            <div>
              <strong>Quarterly plan.pdf</strong>
              <span>Page 14</span>
            </div>
          </div>
          <div className="visual-card source-card two">
            <FileCode2 size={20} />
            <div>
              <strong>project-notes.md</strong>
              <span>Documents</span>
            </div>
          </div>
          <div className="visual-card search-card">
            <Search size={22} />
            <span>the plan we discussed last winter</span>
          </div>
          <div className="local-core">
            <Database size={25} />
            <span>Local index</span>
            <small>~/.redrob/vectordb</small>
          </div>
        </section>
      </div>
      <footer className="onboarding-footer">
        <span>
          <HardDrive size={15} /> Stored at {snapshot.dataDirectory}
        </span>
        <span>Redrob VectorDB {snapshot.appVersion}</span>
      </footer>
    </div>
  );
}

function SearchView({
  snapshot,
  notify,
  setView,
}: {
  snapshot: AppSnapshot;
  notify: Notify;
  setView: (view: View) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selected, setSelected] = useState<SearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [hasSearched, setHasSearched] = useState(false);
  const [extension, setExtension] = useState("all");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener("keydown", focus);
    return () => window.removeEventListener("keydown", focus);
  }, []);

  const runSearch = async (
    searchQuery = query,
    selectedExtension = extension,
  ) => {
    if (!searchQuery.trim()) return;
    try {
      setSearching(true);
      setHasSearched(true);
      const found = await bridge.search({
        query: searchQuery,
        limit: 16,
        filters: {
          extensions: selectedExtension === "all" ? [] : [selectedExtension],
        },
      });
      setResults(found);
      setSelected(found[0] ?? null);
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setSearching(false);
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    void runSearch();
  };
  const extensions = useMemo(
    () => [
      "all",
      ...Array.from(new Set(results.map((result) => result.extension))),
    ],
    [results],
  );

  return (
    <div className="view search-view">
      <header className="view-header">
        <div>
          <span className="eyebrow">LOCAL SEARCH</span>
          <h1>Find anything.</h1>
        </div>
        <div className="header-meta">
          <span>
            <Database size={15} /> {formatNumber(snapshot.stats.indexedChunks)}{" "}
            passages
          </span>
          <span className={snapshot.stats.status === "idle" ? "ready" : "busy"}>
            {snapshot.stats.status === "idle" ? (
              <Check size={14} />
            ) : (
              <LoaderCircle className="spin" size={14} />
            )}{" "}
            {statusLabel(snapshot.stats.status)}
          </span>
        </div>
      </header>
      <form className="search-box" onSubmit={submit}>
        <Search size={22} />
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Describe what you remember…"
          aria-label="Search your library"
        />
        <kbd>⌘ K</kbd>
        {query && (
          <button
            type="button"
            className="icon-button"
            aria-label="Clear search"
            onClick={() => {
              setQuery("");
              setResults([]);
              setSelected(null);
              setExtension("all");
              setHasSearched(false);
            }}
          >
            <X size={18} />
          </button>
        )}
        <button
          className="search-submit"
          aria-label={searching ? "Searching" : "Search"}
          disabled={searching || !query.trim()}
        >
          {searching ? (
            <LoaderCircle className="spin" size={18} />
          ) : (
            <ArrowRight size={18} />
          )}
        </button>
      </form>
      {!hasSearched ? (
        <div className="search-empty">
          <div className="search-empty-icon">
            <Sparkles size={26} />
          </div>
          <h2>Search by meaning, not filenames.</h2>
          <p>Try a phrase, an idea, or the part you remember.</p>
          <div className="suggestion-grid">
            {[
              "the contract with the 60-day notice clause",
              "notes from our India expansion discussion",
              "the design brief with the red gradient",
            ].map((suggestion) => (
              <button
                key={suggestion}
                onClick={() => {
                  setQuery(suggestion);
                  void runSearch(suggestion);
                }}
              >
                <span>“{suggestion}”</span>
                <ChevronRight size={16} />
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="results-layout">
          <section className="results-column">
            <div className="results-toolbar">
              <strong>
                {searching
                  ? "Searching locally…"
                  : `${results.length} relevant passages`}
              </strong>
              <div className="extension-tabs">
                {extensions.map((item) => (
                  <button
                    key={item}
                    className={extension === item ? "active" : ""}
                    onClick={() => {
                      setExtension(item);
                      void runSearch(query, item);
                    }}
                  >
                    {item === "all" ? "All" : item.toUpperCase()}
                  </button>
                ))}
              </div>
            </div>
            {results.length === 0 && !searching ? (
              <NoResults />
            ) : (
              <div className="result-list">
                {results.map((result) => (
                  <ResultCard
                    key={result.chunkId}
                    result={result}
                    active={selected?.chunkId === result.chunkId}
                    onClick={() => setSelected(result)}
                  />
                ))}
              </div>
            )}
          </section>
          <aside className="preview-panel">
            {selected ? (
              <ResultPreview
                result={selected}
                notify={notify}
                ask={() => setView("ask")}
              />
            ) : (
              <div className="preview-placeholder">
                <File size={26} />
                <span>Select a result to preview it</span>
              </div>
            )}
          </aside>
        </div>
      )}
    </div>
  );
}

function ResultCard({
  result,
  active,
  onClick,
}: {
  result: SearchResult;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={active ? "result-card active" : "result-card"}
      onClick={onClick}
    >
      <div className={`file-icon ${result.extension}`}>
        <FileTypeIcon extension={result.extension} />
      </div>
      <div className="result-main">
        <div className="result-title">
          <strong>{result.name}</strong>
          <span>{relevanceLabel(result.score)}</span>
        </div>
        <div className="result-path">
          {shortenPath(result.path)}
          {result.page ? ` · Page ${result.page}` : ""}
        </div>
        <p>{result.content}</p>
      </div>
    </button>
  );
}

function ResultPreview({
  result,
  notify,
  ask,
}: {
  result: SearchResult;
  notify: Notify;
  ask: () => void;
}) {
  const open = async () => {
    try {
      await bridge.openSource(result.path);
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  const reveal = async () => {
    try {
      await bridge.revealSource(result.path);
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  return (
    <div className="preview-content">
      <div className="preview-actions">
        <button className="secondary" onClick={() => void open()}>
          <ExternalLink size={15} /> Open file
        </button>
        <button
          className="icon-button"
          title="Show in folder"
          onClick={() => void reveal()}
        >
          <FolderOpen size={17} />
        </button>
      </div>
      <div className={`preview-file-icon ${result.extension}`}>
        <FileTypeIcon extension={result.extension} />
      </div>
      <h2>{result.name}</h2>
      <span className="preview-location">{result.path}</span>
      <div className="match-meter">
        <span>
          <i style={{ width: `${Math.round(result.score * 100)}%` }} />
        </span>
        <strong>{relevanceLabel(result.score)}</strong>
        <small>search relevance</small>
      </div>
      <div className="passage-label">
        <span>RELEVANT PASSAGE</span>
        {result.page && <span>PAGE {result.page}</span>}
      </div>
      <blockquote>{result.content}</blockquote>
      <button className="ask-about" onClick={ask}>
        <Sparkles size={17} />
        <span>Ask about this and related files</span>
        <ArrowRight size={17} />
      </button>
    </div>
  );
}

function AskView({
  snapshot,
  notify,
}: {
  snapshot: AppSnapshot;
  notify: Notify;
}) {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState<AskResponse | null>(null);
  const [askError, setAskError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [showConnect, setShowConnect] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!question.trim()) return;
    if (!snapshot.connection.connected) {
      setShowConnect(true);
      return;
    }
    try {
      setAsking(true);
      setAskError(null);
      setAnswer(await bridge.ask(question, []));
    } catch (error) {
      setAskError(readError(error));
    } finally {
      setAsking(false);
    }
  };
  return (
    <div className="view ask-view">
      <header className="view-header">
        <div>
          <span className="eyebrow">ASK YOUR LIBRARY</span>
          <h1>Answers with receipts.</h1>
        </div>
        <span className="privacy-chip">
          <LockKeyhole size={14} /> Only relevant excerpts are sent
        </span>
      </header>
      <div className="ask-stage">
        {!answer && !asking ? (
          <div className="ask-intro">
            <div className="ask-orb">
              <Bot size={28} />
            </div>
            <h2>What do you want to know?</h2>
            <p>
              Redrob finds the evidence locally, then uses only those passages
              to answer.
            </p>
            <div className="ask-examples">
              {[
                "What did we decide about the India launch?",
                "Summarise every refund exception",
                "Which proposals mention a February deadline?",
              ].map((item) => (
                <button key={item} onClick={() => setQuestion(item)}>
                  {item}
                  <ChevronRight size={15} />
                </button>
              ))}
            </div>
          </div>
        ) : asking ? (
          <div className="thinking">
            <div className="thinking-mark">
              <Sparkles size={24} />
            </div>
            <h2>Looking through your library…</h2>
            <div className="thinking-steps">
              <span className="done">
                <Check size={14} /> Searching{" "}
                {formatNumber(snapshot.stats.indexedChunks)} passages
              </span>
              <span>
                <LoaderCircle className="spin" size={14} /> Reading the
                strongest evidence
              </span>
              <span>Composing an answer with citations</span>
            </div>
          </div>
        ) : (
          answer && <AnswerCard answer={answer} notify={notify} />
        )}
      </div>
      <form className="ask-composer" onSubmit={(event) => void submit(event)}>
        <textarea
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          placeholder="Ask a question about your files…"
          rows={2}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
        />
        <div className="composer-bottom">
          <span>
            <Database size={14} /> {formatNumber(snapshot.stats.documents)}{" "}
            local files
          </span>
          <button className="primary" disabled={asking || !question.trim()}>
            {asking ? (
              <LoaderCircle className="spin" size={17} />
            ) : (
              <Sparkles size={17} />
            )}{" "}
            Ask Redrob
          </button>
        </div>
      </form>
      {askError && (
        <div className="ask-error" role="alert">
          <CircleHelp size={17} />
          <span>{askError}</span>
          <button onClick={() => setAskError(null)} aria-label="Dismiss error">
            <X size={15} />
          </button>
        </div>
      )}
      {showConnect && (
        <ConnectModal
          close={() => setShowConnect(false)}
          connected={() => setShowConnect(false)}
          notify={notify}
        />
      )}
    </div>
  );
}

function AnswerCard({
  answer,
  notify,
}: {
  answer: AskResponse;
  notify: Notify;
}) {
  return (
    <div className="answer-layout">
      <article className="answer-card">
        <div className="answer-label">
          <Sparkles size={16} /> REDROB ANSWER
        </div>
        <div className="answer-text">{answer.answer}</div>
        <div className="answer-meta">
          <span>{answer.model}</span>
          <span>{(answer.latencyMs / 1000).toFixed(1)}s</span>
          {answer.inputTokens && (
            <span>
              {formatNumber(answer.inputTokens + (answer.outputTokens ?? 0))}{" "}
              tokens
            </span>
          )}
        </div>
      </article>
      <aside className="answer-sources">
        <div className="sources-title">
          <strong>Sources</strong>
          <span>{answer.sources.length} used</span>
        </div>
        {answer.sources.map((source) => (
          <button
            key={source.chunkId}
            onClick={() =>
              void bridge
                .openSource(source.path)
                .catch((error) => notify(readError(error), "error"))
            }
          >
            <span className="source-number">{source.number}</span>
            <div>
              <strong>{source.name}</strong>
              <span>
                {source.page ? `Page ${source.page}` : shortenPath(source.path)}
              </span>
              <p>{source.excerpt}</p>
            </div>
            <ExternalLink size={14} />
          </button>
        ))}
      </aside>
    </div>
  );
}

function SourcesView({
  snapshot,
  refresh,
  notify,
}: {
  snapshot: AppSnapshot;
  refresh: () => Promise<void>;
  notify: Notify;
}) {
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const load = useCallback(async () => {
    try {
      setLoading(true);
      setDocuments(await bridge.documents());
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setLoading(false);
    }
  }, [notify]);
  useEffect(() => {
    void load();
  }, [load]);
  const addFolder = async () => {
    try {
      const path = await bridge.chooseFolder();
      if (!path) return;
      await bridge.addLibraryPath(path);
      await bridge.startIndexing();
      await refresh();
      notify("Source folder added.", "success");
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  const removeFolder = async (path: string) => {
    try {
      await bridge.removeLibraryPath(path);
      await refresh();
      notify("Folder removed from future indexing.", "success");
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  return (
    <div className="view sources-view">
      <header className="view-header">
        <div>
          <span className="eyebrow">LOCAL LIBRARY</span>
          <h1>Sources</h1>
        </div>
        <button className="primary" onClick={() => void addFolder()}>
          <Plus size={17} /> Add folder
        </button>
      </header>
      <div className="stat-grid">
        <Stat
          icon={<FileText />}
          label="Files"
          value={formatNumber(snapshot.stats.documents)}
        />
        <Stat
          icon={<Archive />}
          label="Passages"
          value={formatNumber(snapshot.stats.indexedChunks)}
        />
        <Stat
          icon={<HardDrive />}
          label="On this device"
          value={formatBytes(snapshot.stats.totalBytes)}
        />
        <Stat
          icon={<CircleHelp />}
          label="Needs attention"
          value={formatNumber(snapshot.stats.failed)}
          tone={snapshot.stats.failed ? "warn" : undefined}
        />
      </div>
      <section className="source-section">
        <div className="section-heading">
          <div>
            <h2>Watched folders</h2>
            <p>Changes here are picked up automatically.</p>
          </div>
          <button
            className="secondary"
            onClick={() =>
              void bridge
                .startIndexing()
                .then(refresh)
                .catch((error) => notify(readError(error), "error"))
            }
          >
            <RefreshCw size={15} /> Check now
          </button>
        </div>
        <div className="folder-list">
          {snapshot.settings.libraryPaths.map((path) => (
            <div className="folder-row" key={path}>
              <span className="folder-icon">
                <Folder size={19} />
              </span>
              <div>
                <strong>{lastPathPart(path)}</strong>
                <span>{path}</span>
              </div>
              <span className="watching">
                <span className="status-dot" /> Watching
              </span>
              <button
                className="icon-button danger"
                title="Remove folder"
                onClick={() => void removeFolder(path)}
              >
                <Trash2 size={16} />
              </button>
            </div>
          ))}
        </div>
      </section>
      <section className="source-section">
        <div className="section-heading">
          <div>
            <h2>Recently indexed</h2>
            <p>The newest files in your searchable library.</p>
          </div>
        </div>
        {loading ? (
          <div className="table-loading">
            <LoaderCircle className="spin" /> Loading files…
          </div>
        ) : (
          <div className="document-table">
            <div className="table-head">
              <span>Name</span>
              <span>Type</span>
              <span>Passages</span>
              <span>Modified</span>
              <span>Status</span>
            </div>
            {documents.slice(0, 12).map((document) => (
              <button
                className="table-row"
                key={document.id}
                onClick={() =>
                  void bridge
                    .openSource(document.path)
                    .catch((error) => notify(readError(error), "error"))
                }
              >
                <span className="document-name">
                  <span className={`tiny-file ${document.extension}`}>
                    <FileTypeIcon extension={document.extension} />
                  </span>
                  <span>
                    <strong>{document.name}</strong>
                    <small>{shortenPath(document.path)}</small>
                  </span>
                </span>
                <span>{document.extension.toUpperCase()}</span>
                <span>{document.chunkCount}</span>
                <span>{formatDate(document.modifiedAt)}</span>
                <span
                  className={
                    document.status === "indexed"
                      ? "doc-status good"
                      : "doc-status bad"
                  }
                >
                  {document.status === "indexed" ? "Ready" : "Failed"}
                </span>
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function SettingsView({
  snapshot,
  refresh,
  notify,
}: {
  snapshot: AppSnapshot;
  refresh: () => Promise<void>;
  notify: Notify;
}) {
  const [settings, setSettings] = useState(snapshot.settings);
  const [saving, setSaving] = useState(false);
  const [showConnect, setShowConnect] = useState(false);
  const [maintenanceBusy, setMaintenanceBusy] = useState(false);
  const [compactLayout, setCompactLayout] = useState(
    () => window.matchMedia("(max-width: 1000px)").matches,
  );
  useEffect(() => setSettings(snapshot.settings), [snapshot.settings]);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 1000px)");
    const update = () => setCompactLayout(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  const save = async (next: AppSettings) => {
    try {
      setSaving(true);
      setSettings(next);
      await bridge.saveSettings(next);
      await refresh();
      notify("Settings saved.", "success");
    } catch (error) {
      setSettings(snapshot.settings);
      notify(readError(error), "error");
    } finally {
      setSaving(false);
    }
  };
  const clear = async () => {
    if (
      !window.confirm(
        "Delete the local search index? Your original files will not be touched.",
      )
    )
      return;
    try {
      await bridge.clearLibrary();
      await refresh();
      notify("Local index cleared. Your files were not changed.", "success");
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  const disconnect = async () => {
    try {
      await bridge.disconnect();
      await refresh();
      document.querySelector<HTMLElement>(".workspace")?.scrollTo({ top: 0 });
      notify("Redrob disconnected from this device.", "success");
    } catch (error) {
      notify(readError(error), "error");
    }
  };
  const createBackup = async () => {
    try {
      setMaintenanceBusy(true);
      const path = await bridge.createBackup();
      notify(`Local backup created: ${lastPathPart(path)}`, "success");
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setMaintenanceBusy(false);
    }
  };
  const checkHealth = async () => {
    try {
      setMaintenanceBusy(true);
      notify(await bridge.checkLibraryHealth(), "success");
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setMaintenanceBusy(false);
    }
  };
  const checkForUpdates = async () => {
    try {
      setMaintenanceBusy(true);
      const update = await bridge.checkForUpdate();
      if (!update) {
        notify("Redrob VectorDB is up to date.", "success");
        return;
      }
      if (
        window.confirm(
          `Redrob VectorDB ${update.version} is available. Download, install, and restart now?`,
        )
      ) {
        notify(`Downloading Redrob VectorDB ${update.version}…`, "info");
        await bridge.installPendingUpdate();
      }
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setMaintenanceBusy(false);
    }
  };
  return (
    <div className="view settings-view">
      <header className="view-header">
        <div>
          <span className="eyebrow">PREFERENCES</span>
          <h1>Settings</h1>
        </div>
        {compactLayout && (
          <button
            className="secondary compact-connection-action"
            onClick={() =>
              snapshot.connection.connected
                ? void disconnect()
                : setShowConnect(true)
            }
          >
            {snapshot.connection.connected ? "Disconnect" : "Connect Redrob"}
          </button>
        )}
        {saving && (
          <span className="saving">
            <LoaderCircle className="spin" size={15} /> Saving
          </span>
        )}
      </header>
      <div className="settings-stack">
        <SettingsSection
          icon={<KeyRound />}
          title="Redrob connection"
          description="AI answers use your existing Redrob workspace and balance."
        >
          <div className="connection-card">
            <div
              className={
                snapshot.connection.connected
                  ? "connection-icon connected"
                  : "connection-icon"
              }
            >
              {snapshot.connection.connected ? (
                <Check size={19} />
              ) : (
                <Unplug size={19} />
              )}
            </div>
            <div>
              <strong>
                {snapshot.connection.connected
                  ? "Connected to Redrob"
                  : "Not connected"}
              </strong>
              <span>
                {snapshot.connection.connected
                  ? "The key is verified by Redrob when you send your first question."
                  : "Local indexing and search work without an account."}
              </span>
            </div>
            {!compactLayout &&
              (snapshot.connection.connected ? (
                <button className="secondary" onClick={() => void disconnect()}>
                  Disconnect
                </button>
              ) : (
                <button
                  className="primary"
                  onClick={() => setShowConnect(true)}
                >
                  Connect Redrob
                </button>
              ))}
          </div>
        </SettingsSection>
        <SettingsSection
          icon={<ShieldCheck />}
          title="Privacy"
          description="Control exactly when text can leave this computer."
        >
          <SettingToggle
            label="Allow Ask to send relevant excerpts"
            description="Only the selected passages—not your files or complete index—are sent to Redrob."
            checked={settings.sendContextToRedrob}
            onChange={(checked) =>
              void save({ ...settings, sendContextToRedrob: checked })
            }
          />
          <div className="privacy-detail">
            <LockKeyhole size={16} />
            <div>
              <strong>Always local</strong>
              <span>
                Original files, filenames, embeddings, search history, and the
                full index remain on this device.
              </span>
            </div>
          </div>
        </SettingsSection>
        <SettingsSection
          icon={<Database />}
          title="Indexing"
          description="Tune what is searchable and how much space it can use."
        >
          <div className="setting-row">
            <div>
              <strong>Maximum file size</strong>
              <span>Files larger than this are skipped.</span>
            </div>
            <select
              aria-label="Maximum file size"
              value={settings.maxFileSizeMb}
              onChange={(event) =>
                void save({
                  ...settings,
                  maxFileSizeMb: Number(event.target.value),
                })
              }
            >
              <option value={10}>10 MB</option>
              <option value={25}>25 MB</option>
              <option value={50}>50 MB</option>
              <option value={100}>100 MB</option>
              <option value={250}>250 MB</option>
            </select>
          </div>
          <div className="setting-row">
            <div>
              <strong>Supported file types</strong>
              <span>
                {settings.includeExtensions
                  .map((item) => item.toUpperCase())
                  .join(", ")}
              </span>
            </div>
            <span className="fixed-value">
              {settings.includeExtensions.length} types
            </span>
          </div>
          <div className="setting-row">
            <div>
              <strong>Local API</strong>
              <span>
                Available to Redrob apps on 127.0.0.1. Changes apply after
                restart.
              </span>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                aria-label="Enable local API"
                checked={settings.localApiEnabled}
                onChange={(event) =>
                  void save({
                    ...settings,
                    localApiEnabled: event.target.checked,
                  })
                }
              />
              <span />
            </label>
          </div>
        </SettingsSection>
        <SettingsSection
          icon={<HardDrive />}
          title="Storage"
          description="The index can be rebuilt from your original files at any time."
        >
          <div className="storage-summary">
            <div className="storage-facts">
              <div>
                <strong>{formatBytes(snapshot.stats.totalBytes)}</strong>
                <span>source files represented</span>
              </div>
              <div>
                <strong>{snapshot.dataDirectory}</strong>
                <span>local data directory</span>
              </div>
            </div>
            <div className="storage-actions">
              <button
                className="secondary"
                disabled={maintenanceBusy}
                onClick={() => void checkHealth()}
              >
                <ShieldCheck size={16} /> Check health
              </button>
              <button
                className="secondary"
                disabled={maintenanceBusy}
                onClick={() => void createBackup()}
              >
                <Archive size={16} /> Create backup
              </button>
              <button className="danger-button" onClick={() => void clear()}>
                <Trash2 size={16} /> Clear local index
              </button>
            </div>
          </div>
        </SettingsSection>
        <SettingsSection
          icon={<RefreshCw />}
          title="Updates"
          description="Updates are verified with Redrob's release signature before installation."
        >
          <div className="setting-row">
            <div>
              <strong>Redrob VectorDB {snapshot.appVersion}</strong>
              <span>Check the signed stable release channel.</span>
            </div>
            <button
              className="secondary"
              disabled={maintenanceBusy}
              onClick={() => void checkForUpdates()}
            >
              <RefreshCw size={16} /> Check for updates
            </button>
          </div>
        </SettingsSection>
        <div className="about-line">
          <span>Redrob VectorDB {snapshot.appVersion}</span>
          <span>Qdrant Edge · FastEmbed · Tauri</span>
          <span>© 2026 Redrob</span>
        </div>
      </div>
      {showConnect && (
        <ConnectModal
          close={() => setShowConnect(false)}
          connected={() => {
            setShowConnect(false);
            void refresh();
          }}
          notify={notify}
        />
      )}
    </div>
  );
}

function ConnectModal({
  close,
  connected,
  notify,
}: {
  close: () => void;
  connected: () => void;
  notify: Notify;
}) {
  const [apiKey, setApiKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
        return;
      }
      if (event.key !== "Tab" || !modalRef.current) return;
      const focusable = Array.from(
        modalRef.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      } else if (!modalRef.current.contains(document.activeElement)) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      previouslyFocused?.focus();
    };
  }, [close]);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      setSubmitting(true);
      await bridge.connect(apiKey);
      connected();
      notify("Redrob key saved to this device.", "success");
    } catch (error) {
      notify(readError(error), "error");
    } finally {
      setSubmitting(false);
    }
  };
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => event.target === event.currentTarget && close()}
    >
      <div
        ref={modalRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="connect-title"
      >
        <button
          className="modal-close"
          onClick={close}
          aria-label="Close dialog"
        >
          <X size={18} />
        </button>
        <div className="modal-brand">
          <Logo />
        </div>
        <h2 id="connect-title">Connect Redrob</h2>
        <p>
          Paste a workspace API key. It is saved in your operating system’s
          secure credential store.
        </p>
        <form onSubmit={(event) => void submit(event)}>
          <label>
            Workspace API key
            <input
              type="password"
              autoFocus
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder="rr_••••••••••••••••"
            />
          </label>
          <button
            className="primary full"
            disabled={submitting || apiKey.trim().length < 16}
          >
            {submitting ? (
              <LoaderCircle className="spin" size={17} />
            ) : (
              <KeyRound size={17} />
            )}{" "}
            Connect this device
          </button>
        </form>
        <button
          type="button"
          className="modal-link"
          onClick={() =>
            void bridge
              .openExternal("https://console.redrob.ai/api-keys")
              .catch((error) => notify(readError(error), "error"))
          }
        >
          Create or manage API keys <ExternalLink size={14} />
        </button>
        <div className="modal-privacy">
          <ShieldCheck size={15} /> Search remains local whether or not you
          connect.
        </div>
      </div>
    </div>
  );
}

function SettingsSection({
  icon,
  title,
  description,
  children,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-section">
      <div className="settings-section-title">
        <span>{icon}</span>
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </div>
      <div className="settings-section-body">{children}</div>
    </section>
  );
}
function SettingToggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{label}</strong>
        <span>{description}</span>
      </div>
      <label className="switch">
        <input
          type="checkbox"
          aria-label={label}
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span />
      </label>
    </div>
  );
}
function Stat({
  icon,
  label,
  value,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  tone?: string;
}) {
  return (
    <div className={`stat-card ${tone ?? ""}`}>
      <span>{icon}</span>
      <div>
        <strong>{value}</strong>
        <small>{label}</small>
      </div>
    </div>
  );
}
function ProgressBar({ progress }: { progress: IndexProgress }) {
  const percent = progress.total
    ? Math.round((progress.processed / progress.total) * 100)
    : 0;
  return (
    <div className="progress-toast">
      <div className="progress-copy">
        <LoaderCircle className="spin" size={17} />
        <div>
          <strong>{progress.message}</strong>
          <span>{progress.currentFile ?? "Scanning folders"}</span>
        </div>
        <b>{percent}%</b>
      </div>
      <div className="progress-track">
        <i style={{ width: `${percent}%` }} />
      </div>
    </div>
  );
}
function ToastStack({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="toast-stack">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.tone}`}>
          {toast.tone === "success" ? (
            <Check size={16} />
          ) : toast.tone === "error" ? (
            <X size={16} />
          ) : (
            <Sparkles size={16} />
          )}
          <span>{toast.message}</span>
        </div>
      ))}
    </div>
  );
}
function Logo() {
  return (
    <span className="logo">
      R<span />
    </span>
  );
}
function FileTypeIcon({ extension }: { extension: string }) {
  return extension === "md" || extension === "json" || extension === "html" ? (
    <FileCode2 size={20} />
  ) : (
    <FileText size={20} />
  );
}
function NoResults() {
  return (
    <div className="no-results">
      <Search size={25} />
      <h3>Nothing matched that yet.</h3>
      <p>Try fewer details, another phrase, or check your watched folders.</p>
    </div>
  );
}
function BootState() {
  return (
    <main className="boot-screen">
      <Logo />
      <h1>Redrob VectorDB</h1>
      <p>
        <LoaderCircle className="spin" size={16} /> Opening your local library…
      </p>
    </main>
  );
}
function FatalState({
  error,
  retry,
}: {
  error: string;
  retry: () => Promise<void>;
}) {
  return (
    <main className="fatal-state">
      <div className="fatal-icon">
        <Database size={25} />
      </div>
      <h1>Your local library could not open.</h1>
      <p>{error}</p>
      <button className="primary" onClick={() => void retry()}>
        <RefreshCw size={17} /> Try again
      </button>
    </main>
  );
}

type Notify = (message: string, tone?: Toast["tone"]) => void;
const formatNumber = (value: number) => new Intl.NumberFormat().format(value);
const formatBytes = (value: number) => {
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(
    Math.floor(Math.log(value) / Math.log(1024)),
    units.length - 1,
  );
  return `${(value / 1024 ** index).toFixed(index > 2 ? 1 : 0)} ${units[index]}`;
};
const formatDate = (value: string) => {
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? "—"
    : new Intl.DateTimeFormat(undefined, {
        month: "short",
        day: "numeric",
      }).format(date);
};
const shortenPath = (path: string) => {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 3
    ? `…/${parts.slice(-3, -1).join("/")}`
    : parts.slice(0, -1).join("/");
};
const lastPathPart = (path: string) =>
  path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
const relevanceLabel = (score: number) =>
  score >= 0.99
    ? "Best match"
    : score >= 0.8
      ? "Very relevant"
      : score >= 0.6
        ? "Relevant"
        : "Related";
const statusLabel = (status: string) =>
  ({
    idle: "Library ready",
    indexing: "Indexing locally",
    scanning: "Scanning folders",
    paused: "Indexing paused",
    error: "Needs attention",
  })[status] ?? "Library ready";
const readError = (error: unknown) => {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message;
  return "Something unexpected happened.";
};
