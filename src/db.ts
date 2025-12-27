import Database from "better-sqlite3";
import path from "node:path";
import dotenv from "dotenv";

dotenv.config();

const DB_PATH = path.resolve(process.env.DB_PATH || "labels.db");

console.log(`[DB] Initializing... Path: ${DB_PATH}`);

export const db: Database.Database = new Database(DB_PATH);

// Initialize DB schema
export function initDB() {
  db.exec(`
    CREATE TABLE IF NOT EXISTS labels (
      uri TEXT NOT NULL,
      val TEXT NOT NULL,
      cts TEXT NOT NULL,
      neg INTEGER DEFAULT 0,
      src TEXT,
      PRIMARY KEY (uri, val)
    )
  `);
}
