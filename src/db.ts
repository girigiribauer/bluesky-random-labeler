import Database from "better-sqlite3";
import path from "node:path";
import fs from "node:fs";

const DB_PATH = process.env.DB_PATH || "data/labels.db";
const dataDir = path.dirname(DB_PATH);

if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
}

export const db: Database.Database = new Database(DB_PATH);

// Initialize DB schema
export function initDB() {
    db.exec(`
    CREATE TABLE IF NOT EXISTS labels (
      uri TEXT NOT NULL,
      val TEXT NOT NULL,
      cts TEXT NOT NULL,
      neg INTEGER DEFAULT 0,
      PRIMARY KEY (uri, val)
    )
  `);
}
