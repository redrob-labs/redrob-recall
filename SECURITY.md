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
- The local API listens on loopback, authenticates every endpoint with the token at `~/.redrob/vectordb/local-api-token`, and limits concurrent credit-consuming Ask requests.
- Settings are validated in Rust; remote HTTP endpoints are rejected except loopback development endpoints.
- File open/reveal commands accept only canonical files under selected library roots.
- Production updates and installers are signed; updater verification cannot be disabled.
- The app contains no analytics, advertising, or behavioral telemetry SDK.

## Development precautions

- Never commit `.env` files, application databases, model caches, local API tokens, signing certificates, updater private keys, generated release configuration, or platform notarization credentials.
- Metadata backups contain paths and extracted passages. Keep them on encrypted storage and inspect them before sharing.
- Treat indexed files as untrusted parser input.
- CI workflows use least-privilege permissions and pin external Actions to immutable commit SHAs.
- Review dependency advisories before releases with `npm audit` and the RustSec CI check.
- Production release jobs fail when signing material is absent; do not distribute unsigned fallback builds.
