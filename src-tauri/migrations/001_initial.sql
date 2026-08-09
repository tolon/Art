-- ART initial schema (migration v1)
--
-- Phase 0 ships only the foundational tables. Each later phase adds its own
-- migration files; never edit this file after release.

-- Key/value application settings (mirrors a few things the JSON store also
-- holds, but lets SQL-backed features query settings atomically).
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Recently opened files, surfaced on the Dashboard. `kind` is the detected
-- format hint (adf, lha, hdf, rom, directory, ...).
CREATE TABLE IF NOT EXISTS recent_files (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    opened_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_recent_files_opened_at ON recent_files (opened_at DESC);

-- Background job queue (Phase 0: schema only; the job runner arrives later).
CREATE TABLE IF NOT EXISTS jobs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,
    state      TEXT NOT NULL DEFAULT 'pending',  -- pending | running | done | failed | cancelled
    payload    TEXT,                               -- JSON blob
    result     TEXT,
    progress   REAL NOT NULL DEFAULT 0.0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs (state);
