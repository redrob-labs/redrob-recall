export type IndexStatus = "idle" | "scanning" | "indexing" | "paused" | "error";

export interface AppSettings {
  libraryPaths: string[];
  excludedPaths: string[];
  includeExtensions: string[];
  maxFileSizeMb: number;
  chunkSize: number;
  chunkOverlap: number;
  answerModel: string;
  sendContextToRedrob: boolean;
  localApiEnabled: boolean;
  localApiPort: number;
  redrobBaseUrl: string;
}

export interface IndexStats {
  documents: number;
  indexedChunks: number;
  pending: number;
  failed: number;
  totalBytes: number;
  status: IndexStatus;
  currentFile?: string;
  lastIndexedAt?: string;
}

export interface ConnectionStatus {
  connected: boolean;
  keyLabel?: string;
  endpoint: string;
  error?: string;
}

export interface AppSnapshot {
  settings: AppSettings;
  stats: IndexStats;
  connection: ConnectionStatus;
  dataDirectory: string;
  appVersion: string;
  /** One entry per library folder, in settings order. */
  folders: FolderStatus[];
}

/** `present` is false when the folder is gone: the watcher skips it, so it is not watched. */
export interface FolderStatus {
  path: string;
  present: boolean;
  files: number;
  failed: number;
}

export interface SearchFilters {
  extensions: string[];
  pathPrefix?: string;
}

export interface SearchRequest {
  query: string;
  limit: number;
  filters: SearchFilters;
}

export interface SearchResult {
  chunkId: number;
  documentId: number;
  path: string;
  name: string;
  extension: string;
  page?: number;
  heading?: string;
  content: string;
  /** The window around the matching terms, when the keyword half found this chunk. */
  snippet?: string;
  score: number;
  vectorScore: number;
  keywordScore: number;
  /** How well the document's name or path matched. Zero when only its text did. */
  documentNameScore?: number;
  modifiedAt: string;
}

export interface DocumentRecord {
  id: number;
  path: string;
  name: string;
  extension: string;
  mimeType: string;
  modifiedAt: string;
  sizeBytes: number;
  status: string;
  chunkCount: number;
  lastError?: string;
}

export interface AnswerSource {
  number: number;
  chunkId: number;
  path: string;
  name: string;
  page?: number;
  excerpt: string;
}

export interface AskResponse {
  answer: string;
  sources: AnswerSource[];
  model: string;
  inputTokens?: number;
  outputTokens?: number;
  latencyMs: number;
}

export interface IndexProgress {
  processed: number;
  total: number;
  currentFile?: string;
  status: IndexStatus;
  message: string;
}

export type View = "search" | "ask" | "sources" | "settings";
