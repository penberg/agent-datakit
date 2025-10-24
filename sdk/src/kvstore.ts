import { Database } from '@tursodatabase/database';

export class KvStore {
  private db: Database;
  private initialized: Promise<void>;

  constructor(db: Database) {
    this.db = db;
    this.initialized = this.initialize();
  }

  private async initialize(): Promise<void> {
    // Ensure database is connected
    try {
      await this.db.connect();
    } catch (error: any) {
      // Ignore "already connected" errors
      if (!error.message?.includes('already')) {
        throw error;
      }
    }

    // Create the key-value store table if it doesn't exist
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS kv_store (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL,
        created_at INTEGER DEFAULT (unixepoch()),
        updated_at INTEGER DEFAULT (unixepoch())
      )
    `);

    // Create index on created_at for potential queries
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_kv_store_created_at
      ON kv_store(created_at)
    `);
  }

  async set(key: string, value: any): Promise<void> {
    await this.initialized;

    // Serialize the value to JSON
    const serializedValue = JSON.stringify(value);

    // Use prepared statement to insert or update
    const stmt = this.db.prepare(`
      INSERT INTO kv_store (key, value, updated_at)
      VALUES (?, ?, unixepoch())
      ON CONFLICT(key) DO UPDATE SET
        value = excluded.value,
        updated_at = unixepoch()
    `);

    await stmt.run(key, serializedValue);
  }

  async get(key: string): Promise<any> {
    await this.initialized;

    const stmt = this.db.prepare(`SELECT value FROM kv_store WHERE key = ?`);
    const row = await stmt.get(key) as { value: string } | undefined;

    if (!row) {
      return undefined;
    }

    // Deserialize the JSON value
    return JSON.parse(row.value);
  }

  async delete(key: string): Promise<void> {
    await this.initialized;

    const stmt = this.db.prepare(`DELETE FROM kv_store WHERE key = ?`);
    await stmt.run(key);
  }

  /**
   * Wait for initialization to complete
   */
  async ready(): Promise<void> {
    await this.initialized;
  }
}
