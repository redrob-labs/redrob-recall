# The Redrob Recall local API

Redrob Recall is first of all a memory layer for the other Redrob apps on the same machine: it
indexes the folders a person chose, keeps that index current, and answers searches by keyword and by
meaning. This API is how another app uses it. It is served by `redrob-recall serve` and by the
desktop app while it is open, on the loopback interface only, and every call carries one bearer
token.

The Rust implementation is `crates/redrob-recall-api` (`server` feature: the axum router; `client`
feature: `RecallClient`, a typed client with these same shapes). Any language can call it with plain
HTTP and JSON.

## Reaching it

| | |
| --- | --- |
| Address | `http://127.0.0.1:47331` by default (`localApiPort` in the settings; `redrob-recall serve --port N`). The bound base URL is written to `<data dir>/local-api-address` at every start. |
| Token | `<data dir>/local-api-token`, one line, mode 0600, created on first start. Send it as `Authorization: Bearer <token>`. Every token starts with `rrv_local_`. |
| Data dir | `~/.redrob/recall` (`REDROB_RECALL_DATA_DIR` overrides it). |
| Switch | `localApiEnabled` in the settings (on by default). |

A client on the same machine therefore needs no configuration: read the token file, read the address
file (fall back to the default port when it is missing), and call. `RecallClient::discover()` does
exactly that. A missing or foreign token file means Recall has never run here.

Only a process running as the same user can read the token, and the server never listens on another
interface. Do not copy the token anywhere else.

## Shapes

JSON, camelCase. Errors are `{ "error": "<sentence for a person>", "code": "<stable code>" }` with an
HTTP status; `code` is present when the server has a stable one (`invalid_request`, `no_sources`,
`local_error`), absent otherwise. A missing or wrong token answers `401 { "error": "invalid local API
token" }` on every route.

### `GET /health`

```json
{ "status": "ok", "product": "Redrob Recall", "version": "0.4.0", "documents": 1240, "chunks": 18233 }
```

### `GET /v1/status`

The whole snapshot: settings, counts, folders and the running pass.

```json
{
  "settings": { "libraryPaths": ["/home/me/Documents"], "includeExtensions": ["pdf", "docx", "md", "..."], "...": "..." },
  "stats": { "documents": 1240, "indexedChunks": 18233, "pending": 0, "failed": 3, "totalBytes": 912345678,
             "status": "idle", "currentFile": null, "lastIndexedAt": "2026-10-16T06:00:00Z" },
  "folders": [ { "path": "/home/me/Documents", "present": true, "files": 1240, "failed": 3 } ],
  "progress": { "processed": 1240, "total": 1240, "currentFile": null, "status": "idle", "message": "Your library is up to date" },
  "dataDirectory": "/home/me/.redrob/recall",
  "version": "0.4.0"
}
```

`stats.status` and `progress.status` are one of `idle`, `scanning`, `indexing`, `paused`, `error`.
`progress` is `null` before the first pass of this process.

### `POST /v1/search`

Hybrid search: the vector arm (local multilingual embeddings in Qdrant Edge), the keyword arm (SQLite
FTS5, with bloop's query grammar: quoted phrases, `-excluded`, `heading:word`, `prefix*`) and the
document-name arm, fused by reciprocal rank and normalised to `score` in 0..1.

Request:

```json
{ "query": "quarterly budget review", "limit": 12, "filters": { "extensions": ["pdf", "docx"], "pathPrefix": "/home/me/Documents/Finance" } }
```

`limit` defaults to 12 and is clamped to 1..50; `filters` is optional. Response:

```json
{ "results": [ {
  "chunkId": 48213, "documentId": 311, "path": "/home/me/Documents/Finance/Q3.pdf", "name": "Q3.pdf", "extension": "pdf",
  "page": 4, "heading": null,
  "content": "<the whole chunk>",
  "snippet": "...the <b>quarterly</b> <b>budget</b> <b>review</b> moved to...",
  "score": 1.0, "vectorScore": 0.83, "keywordScore": 12.4, "documentNameScore": 0.0,
  "modifiedAt": "2026-09-30T11:02:44+00:00"
} ] }
```

`snippet` is present when the keyword arm found the chunk (the window FTS5 chose around the match,
with `<b>`/`</b>` around matched terms); a vector-only hit has none, because a semantic match is a
whole-passage judgement with no single place to point at. `documentNameScore` is non-zero when the
file's name or path matched. At most 4 searches run at once; a fifth answers `429`.

### `POST /v1/ask-context`

The local half of "ask my files": the best chunks for a question as numbered sources, plus the two
prompts a chat model answers from. Recall does not call a model here; the caller sends `systemPrompt`
and `userPrompt` to whatever it talks to (the Redrob desktop app sends them to the Redrob console
with the shared account) and shows `sources` beside the answer so `[1]`, `[2]` can be opened.

Request:

```json
{ "query": "What did we decide about the Q3 budget?", "maxSources": 6, "sourceIds": [] }
```

`maxSources` defaults to 6 and is clamped to 1..10. With `sourceIds` (chunk ids from a search), those
chunks are the evidence instead of a new search. Response:

```json
{
  "query": "What did we decide about the Q3 budget?",
  "sources": [ { "number": 1, "chunkId": 48213, "path": "/home/me/Documents/Finance/Q3.pdf", "name": "Q3.pdf", "page": 4,
                 "excerpt": "...the <b>quarterly</b> <b>budget</b> <b>review</b> moved to..." } ],
  "context": "[1]\n...the quarterly budget review moved to...\n\n[2]\n...",
  "systemPrompt": "Answer only from the supplied local sources. Cite factual statements with [1], [2], ...",
  "userPrompt": "LOCAL SOURCES\n[1]\n...\n\nQUESTION\nWhat did we decide about the Q3 budget?"
}
```

Errors: `400 invalid_request` (empty or over 4,000 characters), `404 no_sources` (nothing relevant
indexed), `500 local_error`. The excerpt is the keyword snippet when the search produced one and the
whole chunk otherwise; the context is capped at 60,000 characters.

### `POST /v1/folders` and `DELETE /v1/folders`

Add a folder to the library (a pass starts, or is queued behind the running one) or remove it (what
was indexed under it is dropped). Body: `{ "path": "/home/me/Notes" }`. Both answer the folder list:

```json
{ "folders": [ { "path": "/home/me/Documents", "present": true, "files": 1240, "failed": 3 },
               { "path": "/home/me/Notes", "present": true, "files": 0, "failed": 0 } ] }
```

Add refuses with `400` and a sentence naming the path: it does not exist, it is a file, Recall is not
allowed to read it, it is already in the library. Remove refuses with `409` while a pass is running
("Wait for indexing to finish before removing a library folder"). The path is canonicalised on add,
so the list carries the canonical form; remove takes that form.

### `POST /v1/index` and `POST /v1/index/pause`

Start a pass now (`202 { "started": true, "queued": false }`) or queue one behind the running pass
(`{ "started": false, "queued": true }` -- never dropped). Pause stops the running pass at the next
file and answers `{ "status": "paused" }`; the next `POST /v1/index` resumes.

## What is indexed

The settings' `includeExtensions` (default: pdf, docx, txt, md, markdown, rst, csv, json, html, htm,
log, xml), under `maxFileSizeMb` (default 50), skipping `excludedPaths` components (`.git`,
`node_modules`, `.cache`, `target`, `dist`) and anything `.gitignore`/hidden rules hide. Text is
chunked at `chunkSize` characters (default 1,200) with `chunkOverlap` (180). Everything stays on the
machine: the embedding model (`multilingual-e5-small`, downloaded once into the data directory) runs
locally, and the only network call Recall ever makes on its own is that download.

## Versioning

The API is `v1`. Fields are added, never renamed or removed within `v1`; a client should ignore
fields it does not know. `GET /health` carries the product version.
