import { Database } from '@tursodatabase/database';
import { KvStore } from './kvstore';
import { Filesystem } from './filesystem';
import { ToolCalls } from './toolcalls';

export class AgentOS {
  private db: Database;
  private initialized: Promise<void>;

  public readonly kv: KvStore;
  public readonly fs: Filesystem;
  public readonly tools: ToolCalls;

  constructor(dbPath: string = ':memory:') {
    this.db = new Database(dbPath);
    this.initialized = this.initialize();
    // Create KvStore, Filesystem, and ToolCalls after initialization starts
    // They will wait for the database to be connected
    this.kv = new KvStore(this.db);
    this.fs = new Filesystem(this.db);
    this.tools = new ToolCalls(this.db);
  }

  private async initialize(): Promise<void> {
    // Connect to the database to ensure it's created
    await this.db.connect();

    // Wait for KvStore, Filesystem, and ToolCalls to initialize
    await this.kv.ready();
    await this.fs.ready();
    await this.tools.ready();
  }

  /**
   * Get the underlying Database instance
   */
  getDatabase(): Database {
    return this.db;
  }

  /**
   * Wait for initialization to complete
   */
  async ready(): Promise<void> {
    await this.initialized;
  }

  /**
   * Close the database connection
   */
  async close(): Promise<void> {
    await this.initialized;
    await this.db.close();
  }
}

export { KvStore } from './kvstore';
export { Filesystem } from './filesystem';
export type { Stats } from './filesystem';
export { ToolCalls } from './toolcalls';
export type { ToolCall, ToolCallStats } from './toolcalls';
