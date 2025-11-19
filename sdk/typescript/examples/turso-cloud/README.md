# Turso Cloud Sync Examples

These examples demonstrate how to use AgentFS with Turso Cloud for automatic synchronization between local and cloud databases.

## Prerequisites

1. **Turso Account**: Sign up at [turso.tech](https://turso.tech)
2. **API Token**: Get your API token from the [Turso dashboard](https://turso.tech/app)
3. **Install Dependencies**:
   ```bash
   npm install agentfs-sdk @tursodatabase/api @tursodatabase/sync
   ```

## Environment Setup

Set your Turso API token:

```bash
export TURSO_API_TOKEN="your-token-here"
```

Optional environment variables:

```bash
export TURSO_API_ORG="your-org-name"        # Default: your primary org
export TURSO_DATABASE_URL="database-name"   # Default: agent id
export TURSO_AUTO_SYNC="true"               # Default: true
export TURSO_SYNC_INTERVAL="60000"          # Default: 60000 (60s)
```

## Examples

### 1. Simple Example (`simple.ts`)

Minimal setup using environment variables and defaults.

```bash
# Run the example
npx tsx simple.ts
```

**What it does:**
- Creates a database named `simple-agent` in Turso Cloud
- Stores data locally
- Syncs to cloud
- Enables auto-sync every 60 seconds

**Code:**
```typescript
const agent = await AgentFS.open({
  id: 'simple-agent',
  sync: tursoSync()
});

await agent.kv.set('key', 'value');
await agent.sync.push();
```

### 2. Custom Configuration (`custom.ts`)

Advanced configuration with custom options.

```bash
# Run the example
npx tsx custom.ts
```

**What it does:**
- Demonstrates manual sync control
- Shows custom sync intervals
- Explains all sync operations (pull/push/sync)
- Displays sync status

**Code:**
```typescript
const agent = await AgentFS.open({
  id: 'agent-custom',
  sync: tursoSync({
    org: 'my-org',
    databaseUrl: 'custom-db-name',
    autoSync: false,
    interval: 10000
  })
});

await agent.sync.pull();  // Pull from cloud
await agent.sync.push();  // Push to cloud
await agent.sync.sync();  // Bidirectional
```

## Sync Operations

### `agent.sync.push()`
Push local changes to the cloud database.

### `agent.sync.pull()`
Pull remote changes from the cloud database to local.

### `agent.sync.sync()`
Bidirectional sync (pull + push).

### `agent.sync.getStatus()`
Get current sync status:
```typescript
{
  state: 'idle' | 'syncing' | 'error',
  lastSync?: Date,
  lastError?: string
}
```

## Configuration Options

All options are optional with smart defaults:

| Option | Environment Variable | Default | Description |
|--------|---------------------|---------|-------------|
| `org` | `TURSO_API_ORG` | - | Turso organization name |
| `apiToken` | `TURSO_API_TOKEN` | - | **Required**: API token |
| `databaseUrl` | `TURSO_DATABASE_URL` | agent id | Database name or URL |
| `autoSync` | `TURSO_AUTO_SYNC` | `true` | Enable automatic sync |
| `interval` | `TURSO_SYNC_INTERVAL` | `60000` | Auto-sync interval (ms) |

## Use Cases

### Local-First Development
```typescript
const agent = await AgentFS.open({
  id: 'dev-agent',
  sync: tursoSync({ autoSync: false })
});

// Work locally, sync manually when ready
await agent.sync.push();
```

### Real-Time Collaboration
```typescript
const agent = await AgentFS.open({
  id: 'collab-agent',
  sync: tursoSync({ interval: 5000 }) // Sync every 5s
});
```

### Backup & Recovery
```typescript
const agent = await AgentFS.open({
  id: 'backup-agent',
  sync: tursoSync({ interval: 300000 }) // Sync every 5 minutes
});
```

## Important Notes

⚠️ **@tursodatabase/sync is ALPHA**: The sync package is in alpha stage. While functional, it's recommended for development and testing only. Do not use for critical production data yet.

### Sync Behavior
- **Remote is source of truth**: Conflicts are resolved with Last-Push-Wins strategy
- **Eventually consistent**: Local replica becomes byte-identical to remote
- **Automatic provisioning**: Databases are created automatically if they don't exist

## Troubleshooting

### Error: Missing TURSO_API_TOKEN
Set the environment variable:
```bash
export TURSO_API_TOKEN="your-token"
```

### Error: @tursodatabase/api not found
Install optional dependencies:
```bash
npm install @tursodatabase/api @tursodatabase/sync
```

### Sync Errors
Check sync status:
```typescript
const status = agent.sync.getStatus();
console.log(status.lastError);
```

## Next Steps

- Read the [Turso Documentation](https://docs.turso.tech)
- Explore [AgentFS API Reference](../../README.md)
- Join the [Turso Discord](https://discord.gg/turso)
