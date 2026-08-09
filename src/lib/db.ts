// SQLite access via tauri-plugin-sql.
// Migrations are owned by the Rust side; the frontend only reads/writes.

import Database from "@tauri-apps/plugin-sql";
import type { RecentFile } from "@/types";

let _db: Database | null = null;

export const DB_URI = "sqlite:art.db";

export async function db(): Promise<Database> {
  if (_db) return _db;
  // `load` triggers the Rust-side migrations on first call.
  _db = await Database.load(DB_URI);
  return _db;
}

export async function addRecentFile(path: string, name: string, kind: string): Promise<void> {
  const d = await db();
  const now = Date.now();
  await d.execute(
    `INSERT INTO recent_files (path, name, kind, opened_at) VALUES ($1, $2, $3, $4)
     ON CONFLICT(path) DO UPDATE SET opened_at = $4`,
    [path, name, kind, now],
  );
}

export async function getRecentFiles(limit = 10): Promise<RecentFile[]> {
  const d = await db();
  return d.select<RecentFile[]>(
    `SELECT id, path, name, kind, opened_at FROM recent_files
     ORDER BY opened_at DESC LIMIT $1`,
    [limit],
  );
}

export async function clearRecentFiles(): Promise<void> {
  const d = await db();
  await d.execute(`DELETE FROM recent_files`);
}
