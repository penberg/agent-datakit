/**
 * Sync status information
 */
export interface SyncStatus {
  state: 'idle' | 'syncing' | 'error';
  lastSync?: Date;
  lastError?: string;
}

/**
 * Interface for sync providers
 * Implement this to create custom sync backends (S3, PostgreSQL, etc.)
 */
export interface SyncProvider {
  /**
   * Initialize the sync provider
   */
  initialize(): Promise<void>;

  /**
   * Pull changes from remote to local
   */
  pull(): Promise<void>;

  /**
   * Push changes from local to remote
   */
  push(): Promise<void>;

  /**
   * Bidirectional sync (pull + push)
   */
  sync(): Promise<void>;

  /**
   * Get current sync status
   */
  getStatus(): SyncStatus;

  /**
   * Cleanup resources (stop auto-sync, close connections, etc.)
   */
  cleanup(): Promise<void>;
}

/**
 * Configuration for Turso Cloud sync
 * All fields are optional with smart defaults from environment variables
 */
export interface TursoSyncConfig {
  /**
   * Turso organization name
   * Default: process.env.TURSO_API_ORG
   */
  org?: string;

  /**
   * Turso API token for database management
   * Default: process.env.TURSO_API_TOKEN
   * Required: At least one of these must be set
   */
  apiToken?: string;

  /**
   * Database URL or name
   * - If URL (starts with http): Connect to existing database
   * - If name: Create database with this name if it doesn't exist
   * Default: process.env.TURSO_DATABASE_URL || agent id
   */
  databaseUrl?: string;

  /**
   * Enable automatic background sync
   * Default: process.env.TURSO_AUTO_SYNC !== 'false' (true)
   */
  autoSync?: boolean;

  /**
   * Auto-sync interval in milliseconds
   * Default: process.env.TURSO_SYNC_INTERVAL || 60000 (60s)
   */
  interval?: number;
}

/**
 * Internal factory type for Turso sync
 * @internal
 */
export interface TursoSyncFactory {
  __type: 'turso-sync-factory';
  config: TursoSyncConfig;
}

/**
 * Create a Turso Cloud sync provider
 *
 * @param config Optional configuration (uses env vars and defaults if not provided)
 * @returns Sync provider factory
 *
 * @example
 * ```typescript
 * // Minimal - uses environment variables
 * const agent = await AgentFS.open({
 *   id: 'my-agent',
 *   sync: tursoSync()
 * });
 *
 * // With custom config
 * const agent = await AgentFS.open({
 *   id: 'my-agent',
 *   sync: tursoSync({
 *     org: 'my-org',
 *     databaseUrl: 'https://existing.turso.io',
 *     autoSync: false
 *   })
 * });
 * ```
 */
export function tursoSync(config: TursoSyncConfig = {}): TursoSyncFactory {
  return {
    __type: 'turso-sync-factory',
    config
  };
}
