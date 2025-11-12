import { Database } from '@tursodatabase/database';
import { KvStore } from './kvstore';
import { Filesystem } from './filesystem';
import { ToolCalls } from './toolcalls';

export class AgentFS {
  private db: Database;

  public readonly kv: KvStore;
  public readonly fs: Filesystem;
  public readonly tools: ToolCalls;

  /**
   * Private constructor - use AgentFS.create() instead
   */
  private constructor(db: Database, kv: KvStore, fs: Filesystem, tools: ToolCalls) {
    this.db = db;
    this.kv = kv;
    this.fs = fs;
    this.tools = tools;
  }

  /**
   * Create a new AgentFS instance (async factory method)
   * @param dbPath Path to the database file (defaults to ':memory:')
   * @returns Fully initialized AgentFS instance
   */
  static async create(dbPath: string = ':memory:'): Promise<AgentFS> {
    const db = new Database(dbPath);

    // Connect to the database to ensure it's created
    await db.connect();

    // Create subsystems
    const kv = new KvStore(db);
    const fs = new Filesystem(db);
    const tools = new ToolCalls(db);

    // Wait for all subsystems to initialize
    await kv.ready();
    await fs.ready();
    await tools.ready();

    // Return fully initialized instance
    return new AgentFS(db, kv, fs, tools);
  }

  /**
   * Get the underlying Database instance
   */
  getDatabase(): Database {
    return this.db;
  }

  /**
   * Close the database connection
   */
  async close(): Promise<void> {
    await this.db.close();
  }
}

export { KvStore } from './kvstore';
export { Filesystem } from './filesystem';
export type { Stats } from './filesystem';
export { ToolCalls } from './toolcalls';
export type { ToolCall, ToolCallStats } from './toolcalls';
