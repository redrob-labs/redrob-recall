import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import type {
  AppSettings,
  AppSnapshot,
  AskResponse,
  ConnectionStatus,
  DocumentRecord,
  IndexProgress,
  SearchRequest,
  SearchResult,
} from "../types";

const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const defaultSettings: AppSettings = {
  libraryPaths: [],
  excludedPaths: [".git", "node_modules", ".cache", "target", "dist"],
  includeExtensions: [
    "pdf",
    "docx",
    "txt",
    "md",
    "markdown",
    "rst",
    "csv",
    "json",
    "html",
    "htm",
    "log",
    "xml",
  ],
  maxFileSizeMb: 50,
  chunkSize: 1200,
  chunkOverlap: 180,
  answerModel: "auto",
  sendContextToRedrob: true,
  localApiEnabled: true,
  localApiPort: 47331,
  redrobBaseUrl: "https://console.redrob.ai/api/backend/v1",
};

const demoEnabled =
  !isDesktop &&
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("demo");
let mockConnected = demoEnabled;
let pendingUpdate: Update | null = null;
let mockSettings: AppSettings = demoEnabled
  ? {
      ...defaultSettings,
      libraryPaths: ["/Users/you/Documents", "/Users/you/Projects"],
    }
  : defaultSettings;

const demoResults: SearchResult[] = [
  {
    chunkId: 101,
    documentId: 1,
    path: "/Users/you/Documents/Strategy/india-hiring-plan-2026.pdf",
    name: "india-hiring-plan-2026.pdf",
    extension: "pdf",
    page: 7,
    content:
      "Bengaluru should be the first engineering hiring hub. The plan calls for a local recruiting lead in Q2, followed by the initial platform team and customer operations hires.",
    score: 1,
    vectorScore: 0.86,
    keywordScore: 0.72,
    modifiedAt: "2026-08-28T10:20:00Z",
  },
  {
    chunkId: 102,
    documentId: 2,
    path: "/Users/you/Documents/Board/q4-board-notes.docx",
    name: "q4-board-notes.docx",
    extension: "docx",
    page: 18,
    content:
      "The board approved a phased India expansion, with engineering recruitment beginning in Bengaluru before a broader commercial rollout.",
    score: 0.87,
    vectorScore: 0.81,
    keywordScore: 0.44,
    modifiedAt: "2026-08-19T08:00:00Z",
  },
  {
    chunkId: 103,
    documentId: 3,
    path: "/Users/you/Projects/market-research/india-notes.md",
    name: "india-notes.md",
    extension: "md",
    content:
      "Candidate availability is strongest around Bengaluru and Hyderabad. Compensation benchmarks and notice periods are included below.",
    score: 0.71,
    vectorScore: 0.74,
    keywordScore: 0.3,
    modifiedAt: "2026-08-05T14:10:00Z",
  },
];

function mockSnapshot(): AppSnapshot {
  return {
    settings: mockSettings,
    stats: {
      documents: demoEnabled ? 12481 : 0,
      indexedChunks: demoEnabled ? 48207 : 0,
      pending: 0,
      failed: demoEnabled ? 3 : 0,
      totalBytes: demoEnabled ? 18_920_000_000 : 0,
      status: "idle",
      lastIndexedAt: demoEnabled ? new Date().toISOString() : undefined,
    },
    connection: {
      connected: mockConnected,
      keyLabel: mockConnected ? "This device" : undefined,
      endpoint: defaultSettings.redrobBaseUrl,
    },
    dataDirectory: "~/.redrob/recall",
    appVersion: "0.1.0",
  };
}

export const bridge = {
  isDesktop,
  async snapshot(): Promise<AppSnapshot> {
    return isDesktop ? invoke("get_snapshot") : mockSnapshot();
  },
  async chooseFolder(): Promise<string | null> {
    if (isDesktop) return invoke("choose_library_folder");
    return "/Users/you/Documents";
  },
  async addLibraryPath(path: string): Promise<void> {
    if (isDesktop) return invoke("add_library_path", { path });
    if (!mockSettings.libraryPaths.includes(path))
      mockSettings = {
        ...mockSettings,
        libraryPaths: [...mockSettings.libraryPaths, path],
      };
  },
  async removeLibraryPath(path: string): Promise<void> {
    if (isDesktop) return invoke("remove_library_path", { path });
    mockSettings = {
      ...mockSettings,
      libraryPaths: mockSettings.libraryPaths.filter((item) => item !== path),
    };
  },
  async startIndexing(): Promise<boolean> {
    return isDesktop ? invoke("start_indexing") : true;
  },
  async pauseIndexing(): Promise<void> {
    if (isDesktop) return invoke("pause_indexing");
  },
  async search(request: SearchRequest): Promise<SearchResult[]> {
    if (isDesktop) return invoke("search_library", { request });
    await new Promise((resolve) => setTimeout(resolve, 350));
    if (!request.query.trim() || !demoEnabled) return [];
    const extensions = request.filters.extensions;
    return extensions.length
      ? demoResults.filter((result) => extensions.includes(result.extension))
      : demoResults;
  },
  async ask(query: string, sourceIds: number[]): Promise<AskResponse> {
    if (isDesktop)
      return invoke("ask_library", {
        request: { query, sourceIds, maxSources: 6 },
      });
    await new Promise((resolve) => setTimeout(resolve, 650));
    return {
      answer:
        "The available sources recommend beginning the India expansion in Bengaluru. Engineering recruitment comes first, with a local recruiting lead planned for Q2 [1]. The board approved a phased rollout before broader commercial hiring [2].",
      sources: demoResults.slice(0, 2).map((result, index) => ({
        number: index + 1,
        chunkId: result.chunkId,
        path: result.path,
        name: result.name,
        page: result.page,
        excerpt: result.content,
      })),
      model: "auto",
      inputTokens: 842,
      outputTokens: 68,
      latencyMs: 1240,
    };
  },
  async documents(): Promise<DocumentRecord[]> {
    if (isDesktop) return invoke("get_documents", { limit: 250, offset: 0 });
    if (!demoEnabled) return [];
    return demoResults.map((result) => ({
      id: result.documentId,
      path: result.path,
      name: result.name,
      extension: result.extension,
      mimeType: "text/plain",
      modifiedAt: result.modifiedAt,
      sizeBytes: 1_420_000,
      status: "indexed",
      chunkCount: 18,
    }));
  },
  async openSource(path: string): Promise<void> {
    if (isDesktop) return invoke("open_source", { path });
  },
  async revealSource(path: string): Promise<void> {
    if (isDesktop) return invoke("reveal_source", { path });
  },
  async saveSettings(settings: AppSettings): Promise<void> {
    if (isDesktop) return invoke("save_settings", { settings });
    mockSettings = settings;
  },
  async connect(apiKey: string): Promise<ConnectionStatus> {
    if (isDesktop) return invoke("connect_with_api_key", { apiKey });
    mockConnected = true;
    return {
      connected: true,
      keyLabel: "This device",
      endpoint: defaultSettings.redrobBaseUrl,
    };
  },
  async disconnect(): Promise<ConnectionStatus> {
    if (isDesktop) return invoke("disconnect_redrob");
    mockConnected = false;
    return { connected: false, endpoint: defaultSettings.redrobBaseUrl };
  },
  async openExternal(url: string): Promise<void> {
    if (isDesktop) return openUrl(url);
    window.open(url, "_blank", "noopener,noreferrer");
  },
  async createBackup(): Promise<string> {
    if (isDesktop) return invoke("create_library_backup");
    return "~/.redrob/recall/backups/demo-backup.db";
  },
  async checkLibraryHealth(): Promise<string> {
    if (isDesktop) return invoke("check_library_health");
    return "Local metadata and document references passed their integrity checks";
  },
  async checkForUpdate(): Promise<{
    version: string;
    notes?: string;
  } | null> {
    if (!isDesktop) return null;
    pendingUpdate = await check({ timeout: 30_000 });
    return pendingUpdate
      ? { version: pendingUpdate.version, notes: pendingUpdate.body }
      : null;
  },
  async installPendingUpdate(
    onProgress?: (downloaded: number, total?: number) => void,
  ): Promise<void> {
    if (!pendingUpdate) throw new Error("Check for an update first.");
    let downloaded = 0;
    let total: number | undefined;
    await pendingUpdate.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength;
      if (event.event === "Progress") downloaded += event.data.chunkLength;
      onProgress?.(downloaded, total);
    });
    await relaunch();
  },
  async clearLibrary(): Promise<void> {
    if (isDesktop) return invoke("clear_library");
  },
  async onSnapshot(
    callback: (snapshot: AppSnapshot) => void,
  ): Promise<UnlistenFn> {
    if (!isDesktop) return () => undefined;
    return listen<AppSnapshot>("snapshot-changed", (event) =>
      callback(event.payload),
    );
  },
  async onProgress(
    callback: (progress: IndexProgress) => void,
  ): Promise<UnlistenFn> {
    if (!isDesktop) return () => undefined;
    return listen<IndexProgress>("index-progress", (event) =>
      callback(event.payload),
    );
  },
};
