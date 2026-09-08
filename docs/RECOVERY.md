# Backup and Recovery

Redrob VectorDB stores derived data in `~/.redrob/vectordb`. Original files remain authoritative and are never modified by backup, recovery, or index-clear operations.

## Automatic safeguards

- SQLite uses WAL mode, foreign keys, a 10-second busy timeout, and startup integrity checks.
- Schema changes are ordered with SQLite `user_version`.
- Before a schema migration, the app creates a consistent SQLite backup using `VACUUM INTO`.
- The three newest automatic or manual metadata backups are retained under `~/.redrob/vectordb/backups`.
- Each document is marked vector-pending until all semantic vectors are committed. Interrupted work is reprocessed on the next scan.
- If a Qdrant Edge index cannot be loaded, it is renamed to a timestamped `qdrant-edge-corrupt-*` folder and rebuilt from SQLite and the original files.
- If SQLite fails integrity or migration checks, the database is preserved as `metadata-corrupt-*.db` and the app starts a clean library. A database created by a newer app version is never automatically replaced.
- Unavailable watched roots are not interpreted as deleted files, protecting records for disconnected external drives.

## Create a backup

Open **Settings → Storage → Create backup**. Indexing must be idle. The backup contains paths, metadata, and extracted passages, so protect it as carefully as the original documents and use full-disk encryption.

A metadata backup does not include the vector index or downloaded model; both are rebuildable.

## Restore a backup

Restoration is intentionally an offline administrator operation so the live database cannot be replaced while writers are active:

1. Quit Redrob VectorDB completely.
2. Copy `~/.redrob/vectordb` to a separate safe location.
3. Choose a backup from `~/.redrob/vectordb/backups`.
4. Move `metadata.db`, `metadata.db-wal`, and `metadata.db-shm` out of the data directory if present.
5. Copy the chosen backup to `~/.redrob/vectordb/metadata.db`.
6. Remove or rename `~/.redrob/vectordb/qdrant-edge` so semantic vectors are rebuilt.
7. Start Redrob VectorDB and select **Check now**.

Never restore a database from a newer app version into an older application. Keep the quarantined copy until the restored library has been inspected.

## Full rebuild

Use **Settings → Clear local index**, then **Check now**. Clearing removes extracted passages and vectors but does not remove selected folder settings, the embedding model, API credentials, or original files.

## Disk-full or interrupted updates

Free disk space before retrying. Do not manually delete `metadata.db-wal` while the app is running. If an update is interrupted, restart the existing installed version; signed updates are installed only after the complete artifact has downloaded and verified.
