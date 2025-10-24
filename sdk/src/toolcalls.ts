import { Database } from '@tursodatabase/database';

export interface ToolCall {
  id: number;
  name: string;
  parameters?: any;
  result?: any;
  error?: string;
  started_at: number;
  completed_at: number;
  duration_ms: number;
}

export interface ToolCallStats {
  name: string;
  total_calls: number;
  successful: number;
  failed: number;
  avg_duration_ms: number;
}

export class ToolCalls {
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

    // Create the tool_calls table
    await this.db.exec(`
      CREATE TABLE IF NOT EXISTS tool_calls (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        parameters TEXT,
        result TEXT,
        error TEXT,
        started_at INTEGER NOT NULL,
        completed_at INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL
      )
    `);

    // Create indexes for efficient queries
    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_tool_calls_name
      ON tool_calls(name)
    `);

    await this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_tool_calls_started_at
      ON tool_calls(started_at)
    `);
  }

  /**
   * Record a completed tool call
   * Either result or error should be provided, not both
   * Returns the ID of the created tool call record
   */
  async record(
    name: string,
    started_at: number,
    completed_at: number,
    parameters?: any,
    result?: any,
    error?: string
  ): Promise<number> {
    await this.initialized;

    const serializedParams = parameters !== undefined ? JSON.stringify(parameters) : null;
    const serializedResult = result !== undefined ? JSON.stringify(result) : null;
    const duration_ms = (completed_at - started_at) * 1000;

    const stmt = this.db.prepare(`
      INSERT INTO tool_calls (name, parameters, result, error, started_at, completed_at, duration_ms)
      VALUES (?, ?, ?, ?, ?, ?, ?)
      RETURNING id
    `);

    const row = await stmt.get(name, serializedParams, serializedResult, error || null, started_at, completed_at, duration_ms) as { id: number };
    return row.id;
  }

  /**
   * Get a specific tool call by ID
   */
  async get(id: number): Promise<ToolCall | undefined> {
    await this.initialized;

    const stmt = this.db.prepare(`
      SELECT * FROM tool_calls WHERE id = ?
    `);

    const row = await stmt.get(id) as any;
    if (!row) {
      return undefined;
    }

    return this.rowToToolCall(row);
  }

  /**
   * Query tool calls by name
   */
  async getByName(name: string, limit?: number): Promise<ToolCall[]> {
    await this.initialized;

    const limitClause = limit !== undefined ? `LIMIT ${limit}` : '';
    const stmt = this.db.prepare(`
      SELECT * FROM tool_calls
      WHERE name = ?
      ORDER BY started_at DESC
      ${limitClause}
    `);

    const rows = await stmt.all(name) as any[];
    return rows.map(row => this.rowToToolCall(row));
  }

  /**
   * Query recent tool calls
   */
  async getRecent(since: number, limit?: number): Promise<ToolCall[]> {
    await this.initialized;

    const limitClause = limit !== undefined ? `LIMIT ${limit}` : '';
    const stmt = this.db.prepare(`
      SELECT * FROM tool_calls
      WHERE started_at > ?
      ORDER BY started_at DESC
      ${limitClause}
    `);

    const rows = await stmt.all(since) as any[];
    return rows.map(row => this.rowToToolCall(row));
  }

  /**
   * Get performance statistics for all tools
   */
  async getStats(): Promise<ToolCallStats[]> {
    await this.initialized;

    const stmt = this.db.prepare(`
      SELECT
        name,
        COUNT(*) as total_calls,
        SUM(CASE WHEN error IS NULL THEN 1 ELSE 0 END) as successful,
        SUM(CASE WHEN error IS NOT NULL THEN 1 ELSE 0 END) as failed,
        AVG(duration_ms) as avg_duration_ms
      FROM tool_calls
      GROUP BY name
      ORDER BY total_calls DESC
    `);

    const rows = await stmt.all() as any[];
    return rows.map(row => ({
      name: row.name,
      total_calls: row.total_calls,
      successful: row.successful,
      failed: row.failed,
      avg_duration_ms: row.avg_duration_ms || 0,
    }));
  }

  /**
   * Helper to convert database row to ToolCall object
   */
  private rowToToolCall(row: any): ToolCall {
    return {
      id: row.id,
      name: row.name,
      parameters: row.parameters ? JSON.parse(row.parameters) : undefined,
      result: row.result ? JSON.parse(row.result) : undefined,
      error: row.error || undefined,
      started_at: row.started_at,
      completed_at: row.completed_at,
      duration_ms: row.duration_ms,
    };
  }

  /**
   * Wait for initialization to complete
   */
  async ready(): Promise<void> {
    await this.initialized;
  }
}
