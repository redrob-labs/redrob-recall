# Security Policy

## Supported versions

Redrob VectorDB is currently in private preview. Security fixes are applied to the latest version on the `main` branch.

## Reporting a vulnerability

Do not open a public issue containing exploit details, credentials, private file content, local API tokens, or Redrob API keys. Report vulnerabilities privately to the Redrob maintainers through the repository's private security reporting channel or the established Redrob security contact.

Include:

- the affected version and operating system;
- reproduction steps with synthetic files and placeholder credentials;
- the expected and observed behavior;
- impact and prerequisites; and
- any suggested mitigation.

## Security model

- The desktop process can read only folders the user selects, plus its application data directory.
- Original files are not copied into application storage.
- Extracted passages, SQLite metadata, embeddings, and Qdrant Edge data remain under `~/.redrob/vectordb`.
- The Redrob key is stored in the OS credential manager.
- Ask sends the question and selected excerpt text only. Filenames, paths, full files, and vectors are excluded.
- The local API listens on loopback and authenticates every endpoint with the token at `~/.redrob/vectordb/local-api-token`.
- The app contains no analytics, advertising, or behavioral telemetry SDK.

## Development precautions

- Never commit `.env` files, application databases, model caches, local API tokens, signing certificates, or platform notarization credentials.
- Treat indexed files as untrusted parser input.
- Review dependency advisories before releases with `npm audit` and the appropriate Rust advisory tooling.
- Sign production installers using credentials supplied outside the repository.
