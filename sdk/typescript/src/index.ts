import { Database } from '@tursodatabase/database';
import { existsSync, mkdirSync } from 'fs';
import { KvStore } from './kvstore';
import { Filesystem } from './filesystem';
import { ToolCalls } from './toolcalls';
import type { SyncProvider, TursoSyncFactory } from './sync';
import { TursoSyncProvider } from './providers/turso';

/**
 * Configuration options for opening an AgentFS instance
 */
export interface AgentFSOptions {
  /**
   * Optional unique identifier for the agent.
   * - If provided: Creates persistent storage at `.agentfs/{id}.db`
   * - If omitted: Uses ephemeral in-memory database
   */
  id?: string;

  /**
   * Optional sync configuration
   * - Pass a TursoSyncFactory from tursoSync() for Turso Cloud sync
   * - Pass a custom SyncProvider for other backends
   */
  sync?: TursoSyncFactory | SyncProvider;
}

export class AgentFS {
  private db: Database;

  public readonly kv: KvStore;
  public readonly fs: Filesystem;
  public readonly tools: ToolCalls;
  public readonly sync?: SyncProvider;

  /**
   * Private constructor - use AgentFS.open() instead
   */
  private constructor(
    db: Database,
    kv: KvStore,
    fs: Filesystem,
    tools: ToolCalls,
    sync?: SyncProvider
  ) {
    this.db = db;
    this.kv = kv;
    this.fs = fs;
    this.tools = tools;
    this.sync = sync;
  }

  /**
   * Open an agent filesystem
   * @param options Configuration options (optional id for persistent storage)
   * @returns Fully initialized AgentFS instance
   * @example
   * ```typescript
   * // Persistent storage
   * const agent = await AgentFS.open({ id: 'my-agent' });
   * // Creates: .agentfs/my-agent.db
   *
   * // Ephemeral in-memory database
   * const agent = await AgentFS.open();
   *
   * // With Turso Cloud sync
   * import { tursoSync } from 'agentfs-sdk';
   * const agent = await AgentFS.open({
   *   id: 'my-agent',
   *   sync: tursoSync()
   * });
   * ```
   */
  static async open(options?: AgentFSOptions): Promise<AgentFS> {
    // Error handling for old API usage
    if (typeof options === 'string') {
      throw new Error(
        `AgentFS.open() no longer accepts string paths. ` +
        `Please use: AgentFS.open({ id: 'your-id' }) for persistent storage, ` +
        `or AgentFS.open({}) for ephemeral in-memory database. ` +
        `See migration guide: https://github.com/tursodatabase/agentfs#migration-from-01x-to-02x`
      );
    }

    const { id, sync } = options || {};

    // Determine database path based on id
    let dbPath: string;
    if (!id) {
      // No id = ephemeral in-memory database
      dbPath = ':memory:';
    } else {
      // Ensure .agentfs directory exists
      const dir = '.agentfs';
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      dbPath = `${dir}/${id}.db`;
    }

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

    // Initialize sync if configured
    let syncProvider: SyncProvider | undefined;
    if (sync) {
      if ('__type' in sync && sync.__type === 'turso-sync-factory') {
        // Turso sync factory
        syncProvider = new TursoSyncProvider(db, id || 'default', sync.config);
      } else {
        // Custom sync provider
        syncProvider = sync as SyncProvider;
      }

      await syncProvider.initialize();
    }

    // Return fully initialized instance
    return new AgentFS(db, kv, fs, tools, syncProvider);
  }

  /**
   * Get the underlying Database instance
   */
  getDatabase(): Database {
    return this.db;
  }

  /**
   * Close the database connection and cleanup sync
   */
  async close(): Promise<void> {
    if (this.sync) {
      await this.sync.cleanup();
    }
    await this.db.close();
  }
}

export { KvStore } from './kvstore';
export { Filesystem } from './filesystem';
export type { Stats } from './filesystem';
export { ToolCalls } from './toolcalls';
export type { ToolCall, ToolCallStats } from './toolcalls';
export { tursoSync } from './sync';
export type { SyncProvider, TursoSyncConfig, SyncStatus } from './sync';
