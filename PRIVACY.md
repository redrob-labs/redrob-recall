# Redrob VectorDB Privacy

_Last updated: September 7, 2026_

Redrob VectorDB is designed as a local-first application. This document describes the implemented data flow; it is not a substitute for Redrob’s organization-wide privacy policy or the terms that apply to the Redrob API.

## Data kept on your device

The app stores the following under `~/.redrob/vectordb`:

- paths and metadata for folders and files you selected;
- text passages extracted from supported files;
- keyword-search data and local vector embeddings;
- the downloaded embedding model;
- application settings; and
- a random token used to authenticate the loopback API.

The Redrob workspace API key is stored in the operating system credential manager, not in the metadata database. Original files remain in their existing locations and are not copied into application storage.

## Network activity

### Local Search

Local Search performs extraction, embedding, keyword retrieval, semantic retrieval, ranking, and result preview on the device. Search queries and results are not sent to Redrob.

### Ask

Ask is disabled unless the privacy setting **Allow Ask to send relevant excerpts** is enabled and a Redrob API key is available. For each Ask request, the app sends:

- the question you entered;
- up to ten locally selected relevant text excerpts;
- numeric citation labels; and
- the configured model and completion parameters.

The app does not include file paths, filenames, complete files, unrelated passages, embeddings, or the full index in that request. Returned answers, usage counts, model name, and latency are displayed in the current session.

The request is sent to the configured Redrob API base URL, which defaults to `https://console.redrob.ai/api/backend/v1`. Processing by Redrob is governed by the terms and privacy commitments for your Redrob workspace.

### Model download

FastEmbed downloads the multilingual embedding model when it is needed for the first time. The model is then cached under the local application data directory. This download reveals normal network metadata such as your IP address to the model host, but does not include your files or extracted content.

## Local API

When enabled, the API listens on `127.0.0.1` only. `/health`, `/v1/search`, and `/v1/ask` all require the bearer token in `~/.redrob/vectordb/local-api-token`. Other software running under your user account may be able to read files that account can read, so install only trusted local applications. API setting changes take effect after app restart.

## Telemetry

The application does not include an analytics, advertising, crash-reporting, or behavioral telemetry SDK. Diagnostic log messages may contain local paths when run from a development terminal; review logs before sharing them.

## Delete local data

- **Settings → Clear local index** removes searchable metadata, passages, and vectors but does not modify original files or watched-folder settings.
- **Disconnect Redrob** removes the stored API key from the operating system credential manager.
- To remove all app data, quit Redrob VectorDB and delete `~/.redrob/vectordb`.

Removing a watched folder stops future indexing of it. Run **Check now** or clear/rebuild the local index to immediately reconcile files that were previously indexed.

## Security boundaries

The app does not make untrusted files safe. Its parsers process files selected through watched folders. Keep the app updated, avoid indexing folders containing files from untrusted sources, and use normal operating-system account and disk-encryption protections.
