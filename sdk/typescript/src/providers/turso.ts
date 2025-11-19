import type { Database } from '@tursodatabase/database';
import type { SyncProvider, SyncStatus, TursoSyncConfig } from '../sync';

/**
 * Turso Cloud sync provider
 * Handles database provisioning via @tursodatabase/api and sync via @tursodatabase/sync
 */
export class TursoSyncProvider implements SyncProvider {
  private apiClient: any;
  private syncDb: any;
  private status: SyncStatus = { state: 'idle' };
  private autoSyncTimer?: NodeJS.Timeout;
  private resolvedConfig: Required<TursoSyncConfig>;
  private dbUrl?: string;
  private dbAuthToken?: string;

  constructor(
    private db: Database,
    private agentId: string,
    config: TursoSyncConfig
  ) {
    // Resolve config with precedence: explicit > env > defaults
    this.resolvedConfig = {
      org: config.org || process.env.TURSO_API_ORG || '',
      apiToken: config.apiToken || process.env.TURSO_API_TOKEN || '',
      databaseUrl:
        config.databaseUrl ||
        process.env.TURSO_DATABASE_URL ||
        agentId,
      autoSync:
        config.autoSync ??
        (process.env.TURSO_AUTO_SYNC === 'false' ? false : true),
      interval:
        config.interval ||
        parseInt(process.env.TURSO_SYNC_INTERVAL || '60000', 10),
    };
  }

  async initialize(): Promise<void> {
    try {
      // 1. Validate required config
      if (!this.resolvedConfig.apiToken) {
        throw new Error(
          'Turso API token required. Set TURSO_API_TOKEN environment variable or pass apiToken in config.'
        );
      }

      // 2. Check if optional dependencies are available
      let createClient: any;
      let connect: any;

      try {
        const apiModule = await import('@tursodatabase/api');
        createClient = apiModule.createClient;
      } catch (error) {
        throw new Error(
          '@tursodatabase/api is required for Turso sync. Install it with: npm install @tursodatabase/api'
        );
      }

      try {
        const syncModule = await import('@tursodatabase/sync');
        connect = syncModule.connect;
      } catch (error) {
        throw new Error(
          '@tursodatabase/sync is required for Turso sync. Install it with: npm install @tursodatabase/sync'
        );
      }

      // 3. Setup API client
      this.apiClient = createClient({
        org: this.resolvedConfig.org,
        token: this.resolvedConfig.apiToken,
      });

      // 4. Determine if we need to provision or connect to existing database
      const databaseUrl = this.resolvedConfig.databaseUrl;

      if (databaseUrl.startsWith('http://') || databaseUrl.startsWith('https://')) {
        // Existing database URL provided
        this.dbUrl = databaseUrl;
        // For existing DB, user should provide token via env or config
        // We'll try to get it, but may need user to provide it
        console.warn(
          'Using existing database URL. Ensure database auth token is available via TURSO_DB_TOKEN environment variable if needed.'
        );
      } else {
        // Database name - provision it
        await this.provisionDatabase(databaseUrl);
      }

      // 5. Setup sync connection
      // Get the database file path
      const dbPath = (this.db as any).path || '.agentfs/' + this.agentId + '.db';

      this.syncDb = await connect({
        path: dbPath,
        url: this.dbUrl!,
        authToken: this.dbAuthToken || process.env.TURSO_DB_TOKEN || '',
        clientName: `agentfs-${this.agentId}`,
      });

      // 6. Start auto-sync if enabled
      if (this.resolvedConfig.autoSync) {
        this.startAutoSync();
      }

      this.status = { state: 'idle' };
    } catch (error) {
      this.status = {
        state: 'error',
        lastError: error instanceof Error ? error.message : String(error),
      };
      throw error;
    }
  }

  private async provisionDatabase(dbName: string): Promise<void> {
    try {
      // Check if database exists
      try {
        const dbInfo = await this.apiClient.databases.get(dbName);
        this.dbUrl = dbInfo.hostname;
        console.log(`Using existing Turso database: ${dbName}`);
      } catch (error) {
        // Database doesn't exist, create it
        console.log(`Creating Turso database: ${dbName}...`);
        const newDb = await this.apiClient.databases.create(dbName);
        this.dbUrl = newDb.hostname;
        console.log(`Created Turso database: ${dbName}`);
      }

      // Create/get auth token for the database
      const tokenResponse = await this.apiClient.databases.createToken(dbName, {
        expiration: 'never',
        authorization: 'full-access',
      });

      this.dbAuthToken = tokenResponse.jwt;
    } catch (error) {
      throw new Error(
        `Failed to provision Turso database '${dbName}': ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  async pull(): Promise<void> {
    if (!this.syncDb) {
      throw new Error('Sync not initialized. Call initialize() first.');
    }

    this.status = { state: 'syncing' };
    try {
      await this.syncDb.pull();
      this.status = { state: 'idle', lastSync: new Date() };
    } catch (error) {
      this.status = {
        state: 'error',
        lastError: error instanceof Error ? error.message : String(error),
      };
      throw error;
    }
  }

  async push(): Promise<void> {
    if (!this.syncDb) {
      throw new Error('Sync not initialized. Call initialize() first.');
    }

    this.status = { state: 'syncing' };
    try {
      await this.syncDb.push();
      this.status = { state: 'idle', lastSync: new Date() };
    } catch (error) {
      this.status = {
        state: 'error',
        lastError: error instanceof Error ? error.message : String(error),
      };
      throw error;
    }
  }

  async sync(): Promise<void> {
    if (!this.syncDb) {
      throw new Error('Sync not initialized. Call initialize() first.');
    }

    this.status = { state: 'syncing' };
    try {
      await this.syncDb.sync();
      this.status = { state: 'idle', lastSync: new Date() };
    } catch (error) {
      this.status = {
        state: 'error',
        lastError: error instanceof Error ? error.message : String(error),
      };
      throw error;
    }
  }

  getStatus(): SyncStatus {
    return { ...this.status };
  }

  private startAutoSync(): void {
    this.autoSyncTimer = setInterval(async () => {
      // Only sync if not currently syncing
      if (this.status.state !== 'syncing') {
        try {
          await this.sync();
        } catch (error) {
          console.error('Auto-sync failed:', error);
        }
      }
    }, this.resolvedConfig.interval);

    // Ensure timer doesn't keep process alive
    if (this.autoSyncTimer.unref) {
      this.autoSyncTimer.unref();
    }
  }

  async cleanup(): Promise<void> {
    // Stop auto-sync
    if (this.autoSyncTimer) {
      clearInterval(this.autoSyncTimer);
      this.autoSyncTimer = undefined;
    }

    // Close sync connection if available
    // Note: @tursodatabase/sync may not have explicit close method
    // The connection will be cleaned up when the process exits
  }
}
