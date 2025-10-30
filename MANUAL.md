# AgentOS User Manual

AgentOS provides a secure sandboxed environment for running AI agents with SQLite-backed filesystem storage. It enables agents to interact with files while maintaining complete isolation, auditability, and control.

## Overview

AgentOS consists of three main components:

1. **[Agent Filesystem Specification](SPEC.md)** - Defines the SQLite-based filesystem format, including:
   - Virtual filesystem with Unix-like inode design
   - Key-value store for agent context and state
   - Tool call audit trail for debugging and analysis

2. **AgentOS Tool** - Command-line tool for managing and running agents in sandboxed environments

3. **AgentOS SDK** - TypeScript/JavaScript SDK for building agents that work with AgentOS

## Quick Start

### 1. Initialize an Agent Filesystem

Create a new SQLite-based agent filesystem:

```bash
$ agentos init
Created agent filesystem: agent.db
```

You can specify a custom filename:

```bash
$ agentos init myagent.db
Created agent filesystem: myagent.db
```

Use `--force` to overwrite an existing file:

```bash
$ agentos init --force agent.db
Created agent filesystem: agent.db
```

### 2. Run Programs in the Sandbox

Start any program with the agent filesystem mounted at `/agent`:

```bash
$ agentos run /bin/bash
Welcome to AgentOS!

The following mount points are sandboxed:
 - /agent -> agent.db (sqlite)

$ echo "hello, agent" > /agent/hello.txt
$ cat /agent/hello.txt
hello, agent
$ exit
```

### 3. Inspect the Agent Filesystem

List files in the agent filesystem:

```bash
$ agentos fs ls
f hello.txt
```

Display file contents:

```bash
$ agentos fs cat hello.txt
hello, agent
```

## AgentOS Tool Reference

### `agentos init`

Initialize a new agent filesystem.

**Usage:**
```bash
agentos init [OPTIONS] [FILENAME]
```

**Arguments:**
- `[FILENAME]` - SQLite file to create (default: `agent.db`)

**Options:**
- `--force` - Overwrite existing file if it exists
- `-h, --help` - Print help

**Examples:**
```bash
# Create agent.db in current directory
agentos init

# Create with custom name
agentos init production-agent.db

# Overwrite existing file
agentos init --force agent.db
```

**What it does:**
Creates a new SQLite database with the [Agent Filesystem schema](SPEC.md), including:
- Root directory (inode 1)
- File metadata tables (`fs_inode`, `fs_dentry`, `fs_data`, `fs_symlink`)
- Key-value store table (`kv_store`)
- Tool call tracking table (`tool_calls`)

### `agentos run`

Execute a program in the sandboxed environment.

**Usage:**
```bash
agentos run [OPTIONS] <COMMAND> [ARGS]...
```

**Arguments:**
- `<COMMAND>` - Command to execute
- `[ARGS]...` - Arguments for the command

**Options:**
- `--mount <MOUNT_SPEC>` - Mount configuration (format: `type=bind,src=<host_path>,dst=<sandbox_path>`)
- `--strace` - Enable strace-like output for system calls
- `-h, --help` - Print help

**Examples:**

Basic shell access:
```bash
agentos run /bin/bash
```

Run a Python script:
```bash
agentos run python3 agent.py
```

Run with custom mount points:
```bash
agentos run --mount type=bind,src=/tmp/data,dst=/data /bin/bash
```

Debug system calls with strace output:
```bash
agentos run --strace python3 agent.py
```

### `agentos fs`

Perform filesystem operations on the agent database from outside the sandbox.

**Usage:**
```bash
agentos fs <COMMAND>
```

**Commands:**
- `ls` - List files in the filesystem
- `cat` - Display file contents

#### `agentos fs ls`

List files and directories in the agent filesystem.

**Usage:**
```bash
agentos fs ls [PATH]
```

**Examples:**
```bash
# List root directory
agentos fs ls

# List subdirectory
agentos fs ls /artifacts
```

**Output format:**
- `f <name>` - Regular file
- `d <name>` - Directory

#### `agentos fs cat`

Display the contents of a file in the agent filesystem.

**Usage:**
```bash
agentos fs cat <PATH>
```

**Examples:**
```bash
# Display file contents
agentos fs cat hello.txt

# Display file in subdirectory
agentos fs cat /artifacts/report.txt
```

## How AgentOS Works

### Architecture

```
┌─────────────────────────────────────────┐
│         Agent Application               │
├─────────────────────────────────────────┤
│      AgentOS Sandbox (Hermit)           │
│   Filesystem Interception Layer         │
├─────────────────────────────────────────┤
│       /agent mount point                │
├─────────────────────────────────────────┤
│    Agent Filesystem (agent.db)          │
│         SQLite Database                 │
└─────────────────────────────────────────┘
```

### Filesystem Interception

AgentOS uses [Hermit](https://github.com/facebookexperimental/hermit), a deterministic execution sandbox that intercepts all system calls. When a program running inside AgentOS attempts filesystem operations on `/agent/*`, AgentOS:

1. **Intercepts** the system call (open, read, write, etc.)
2. **Translates** the path to a SQLite query
3. **Executes** the operation on the agent database
4. **Returns** results to the program

This is completely transparent to the application - it sees `/agent` as a normal POSIX filesystem.

### SQLite as a Filesystem

The agent filesystem uses a Unix-like inode design implemented in SQLite. See the [Agent Filesystem Specification](SPEC.md) for complete details.

**Key features:**
- **Inodes** - Each file/directory has a unique inode number and metadata
- **Directory entries** - Map names to inodes (enables hard links)
- **Data chunks** - File contents stored as BLOBs
- **Metadata** - Unix-style permissions, timestamps, ownership

**Benefits:**
- **Single file** - Entire filesystem in one `.db` file
- **Snapshotting** - Copy the file to snapshot complete state
- **Auditability** - Query filesystem history with SQL
- **ACID transactions** - Consistency guarantees
- **Portability** - Works everywhere SQLite works

## AgentOS SDK

The AgentOS SDK provides a TypeScript/JavaScript interface for building agents that use the agent filesystem. It offers three main APIs for working with the agent database:

- **Key-Value Store** - Simple storage for agent context, preferences, and state
- **Filesystem** - POSIX-like file operations for reading/writing files
- **Tool Calls** - Track and analyze agent tool invocations

### Installation

```bash
npm install agentos-sdk
```

### Quick Start

```typescript
import { AgentOS } from 'agentos-sdk';

// Initialize the agent store
const agent = new AgentOS('./agent.db');

// Wait for initialization (optional, operations will auto-wait)
await agent.ready();

// Key-value operations
await agent.kv.set('user:name', 'Alice');
const name = await agent.kv.get('user:name');

// Filesystem operations
await agent.fs.writeFile('/output/report.txt', 'Hello, world!');
const content = await agent.fs.readFile('/output/report.txt');
const files = await agent.fs.readdir('/output');

// Tool call tracking
await agent.tools.record(
  'web_search',
  Date.now() / 1000,
  Date.now() / 1000 + 1.5,
  { query: 'AI agents' },
  { results: ['result1', 'result2'] }
);

// Get performance statistics
const stats = await agent.tools.getStats();

// Close when done
await agent.close();
```

### API Reference

#### AgentOS Class

The main class for interacting with the agent database.

**Constructor:**
```typescript
new AgentOS(dbPath?: string)
```
- `dbPath` - Path to the SQLite database file (default: `:memory:`)

**Properties:**
- `kv: KvStore` - Key-value store interface
- `fs: Filesystem` - Filesystem interface
- `tools: ToolCalls` - Tool call tracking interface

**Methods:**
- `ready(): Promise<void>` - Wait for initialization to complete
- `getDatabase(): Database` - Get the underlying Database instance
- `close(): Promise<void>` - Close the database connection

#### Key-Value Store API

Simple key-value storage for agent context and preferences.

**set(key: string, value: any): Promise<void>**

Store a value with the given key. The value is automatically serialized to JSON.

```typescript
await agent.kv.set('config', { theme: 'dark', lang: 'en' });
await agent.kv.set('counter', 42);
await agent.kv.set('items', ['apple', 'banana', 'cherry']);
```

**get(key: string): Promise<any>**

Retrieve a value by key. Returns `undefined` if the key doesn't exist. The value is automatically deserialized from JSON.

```typescript
const config = await agent.kv.get('config');
const counter = await agent.kv.get('counter');
const missing = await agent.kv.get('nonexistent'); // undefined
```

**delete(key: string): Promise<void>**

Delete a key-value pair.

```typescript
await agent.kv.delete('counter');
```

**ready(): Promise<void>**

Wait for initialization to complete.

#### Filesystem API

POSIX-like filesystem operations for managing files and directories.

**writeFile(path: string, content: string | Buffer): Promise<void>**

Write content to a file. Creates parent directories automatically. Overwrites existing files.

```typescript
// Write text
await agent.fs.writeFile('/notes/todo.txt', 'Buy groceries');

// Write binary data
const pdfBuffer = Buffer.from(pdfData);
await agent.fs.writeFile('/reports/summary.pdf', pdfBuffer);
```

**readFile(path: string): Promise<string>**

Read file contents as a UTF-8 string. Throws `ENOENT` error if the file doesn't exist.

```typescript
const content = await agent.fs.readFile('/notes/todo.txt');
console.log(content); // 'Buy groceries'
```

**readdir(path: string): Promise<string[]>**

List files and directories in a directory. Returns file/directory names (not full paths).

```typescript
const files = await agent.fs.readdir('/notes');
console.log(files); // ['todo.txt', 'ideas.txt']
```

**deleteFile(path: string): Promise<void>**

Delete a file. Throws `ENOENT` error if the file doesn't exist.

```typescript
await agent.fs.deleteFile('/notes/todo.txt');
```

**stat(path: string): Promise<Stats>**

Get file/directory metadata.

```typescript
const stats = await agent.fs.stat('/notes/todo.txt');
console.log(stats.size);      // File size in bytes
console.log(stats.mtime);     // Modification time (Unix timestamp)
console.log(stats.isFile());  // true
console.log(stats.isDirectory()); // false
```

**Stats Interface:**
```typescript
interface Stats {
  ino: number;           // Inode number
  mode: number;          // File mode (type + permissions)
  nlink: number;         // Number of hard links
  uid: number;           // User ID
  gid: number;           // Group ID
  size: number;          // File size in bytes
  atime: number;         // Access time (Unix timestamp)
  mtime: number;         // Modification time (Unix timestamp)
  ctime: number;         // Change time (Unix timestamp)
  isFile(): boolean;
  isDirectory(): boolean;
  isSymbolicLink(): boolean;
}
```

**ready(): Promise<void>**

Wait for initialization to complete.

#### Tool Calls API

Track and analyze agent tool invocations for debugging and performance monitoring.

**record(name: string, started_at: number, completed_at: number, parameters?: any, result?: any, error?: string): Promise<number>**

Record a completed tool call. Either `result` or `error` should be provided (not both). Returns the ID of the created record.

Timestamps should be Unix timestamps (seconds since epoch).

```typescript
const started = Date.now() / 1000;
// ... perform the tool call ...
const completed = Date.now() / 1000;

// Successful call
const id = await agent.tools.record(
  'web_search',
  started,
  completed,
  { query: 'AgentOS' },
  { results: ['result1', 'result2'] }
);

// Failed call
await agent.tools.record(
  'database_query',
  started,
  completed,
  { sql: 'SELECT * FROM users' },
  undefined,
  'Connection timeout'
);
```

**get(id: number): Promise<ToolCall | undefined>**

Get a specific tool call by ID.

```typescript
const call = await agent.tools.get(42);
console.log(call.name);         // 'web_search'
console.log(call.duration_ms);  // 1500
```

**getByName(name: string, limit?: number): Promise<ToolCall[]>**

Query tool calls by name, most recent first.

```typescript
// Get all web_search calls
const searches = await agent.tools.getByName('web_search');

// Get last 10 web_search calls
const recent = await agent.tools.getByName('web_search', 10);
```

**getRecent(since: number, limit?: number): Promise<ToolCall[]>**

Query recent tool calls since a given timestamp, most recent first.

```typescript
const oneHourAgo = Date.now() / 1000 - 3600;
const recentCalls = await agent.tools.getRecent(oneHourAgo);

// Last 5 calls in the past hour
const latest = await agent.tools.getRecent(oneHourAgo, 5);
```

**getStats(): Promise<ToolCallStats[]>**

Get performance statistics for all tools, ordered by total call count.

```typescript
const stats = await agent.tools.getStats();
for (const stat of stats) {
  console.log(`${stat.name}:`);
  console.log(`  Total calls: ${stat.total_calls}`);
  console.log(`  Success rate: ${stat.successful / stat.total_calls * 100}%`);
  console.log(`  Avg duration: ${stat.avg_duration_ms}ms`);
}
```

**ToolCall Interface:**
```typescript
interface ToolCall {
  id: number;
  name: string;
  parameters?: any;
  result?: any;
  error?: string;
  started_at: number;      // Unix timestamp (seconds)
  completed_at: number;    // Unix timestamp (seconds)
  duration_ms: number;
}
```

**ToolCallStats Interface:**
```typescript
interface ToolCallStats {
  name: string;
  total_calls: number;
  successful: number;
  failed: number;
  avg_duration_ms: number;
}
```

**ready(): Promise<void>**

Wait for initialization to complete.

### Examples

The SDK includes working examples in the `sdk/examples/` directory:

- **Key-Value Store** (`sdk/examples/kvstore/`) - Basic key-value operations
- **Filesystem** (`sdk/examples/filesystem/`) - File and directory operations
- **Tool Calls** (`sdk/examples/toolcalls/`) - Tool call tracking and analytics

Run examples:
```bash
cd sdk/examples/kvstore
npm install
npm start
```

### TypeScript Support

The SDK is written in TypeScript and includes full type definitions. TypeScript users get autocomplete, type checking, and inline documentation:

```typescript
import { AgentOS, Stats, ToolCall, ToolCallStats } from 'agentos-sdk';

const agent = new AgentOS('./agent.db');

// Type-safe operations
const stats: Stats = await agent.fs.stat('/file.txt');
const calls: ToolCall[] = await agent.tools.getByName('search');
```

### Error Handling

The SDK throws standard Node.js-style errors with descriptive messages:

```typescript
try {
  await agent.fs.readFile('/nonexistent.txt');
} catch (error) {
  console.error(error.message); // "ENOENT: no such file or directory, open '/nonexistent.txt'"
}

try {
  await agent.fs.deleteFile('/missing.txt');
} catch (error) {
  console.error(error.message); // "ENOENT: no such file or directory, unlink '/missing.txt'"
}
```

### Using with Turso

The SDK uses [@tursodatabase/database](https://www.npmjs.com/package/@tursodatabase/database) under the hood, which supports both local SQLite files and remote Turso databases.

For local SQLite:
```typescript
const agent = new AgentOS('./agent.db');
```

For Turso (requires additional configuration):
```typescript
import { Database } from '@tursodatabase/database';

const db = new Database('libsql://your-database.turso.io', {
  authToken: process.env.TURSO_AUTH_TOKEN
});

// Use db directly or pass it to AgentOS components
```

See the [Turso documentation](https://docs.turso.tech) for more details on remote databases.

## Advanced Usage

### Multiple Mount Points

You can mount both host directories and agent databases:

```bash
# Mount agent database at /agent and host directory at /data
agentos run \
  --mount type=bind,src=./data,dst=/data \
  /bin/bash
```

The default agent database (`agent.db`) is always mounted at `/agent`.

### Debugging with Strace

Use `--strace` to see all intercepted system calls:

```bash
agentos run --strace python3 script.py
```

This shows detailed information about every filesystem operation, useful for debugging and understanding agent behavior.

### Snapshotting Agent State

Since the entire filesystem is a single SQLite file, snapshotting is trivial:

```bash
# Create a snapshot
cp agent.db agent-snapshot-$(date +%s).db

# Restore from snapshot
cp agent-snapshot-1234567890.db agent.db
```

### Querying Agent Data

You can query the agent database directly with SQLite:

```bash
sqlite3 agent.db "SELECT * FROM fs_inode WHERE mode & 0170000 = 0100000"
```

Or use the SQL interface from your application to analyze agent behavior, search files, track tool usage, etc.

See the [Agent Filesystem Specification](SPEC.md) for the complete schema.

## Use Cases

### 1. Agent Development and Testing

- Run agents in isolated environments
- Snapshot state before risky operations
- Replay agent execution from checkpoints
- Debug by querying filesystem history

### 2. Production Agent Deployment

- Monitor agent filesystem access
- Audit all file operations
- Track tool invocations and errors
- Maintain complete operation history

### 3. Multi-Agent Systems

- Each agent gets its own `.db` file
- Share data by mounting common databases
- Analyze agent interactions via SQL queries
- Compare agent behavior across runs

### 4. Agent Training and Fine-tuning

- Capture agent decisions and outcomes
- Query successful vs. failed operations
- Extract training data from agent history
- Analyze tool usage patterns

## Security Considerations

**Sandboxing:**
- AgentOS intercepts filesystem operations but doesn't provide full security isolation
- Programs can still make network calls, execute system commands, etc.
- Use additional security measures (containers, VMs, network policies) for untrusted code

**Database Security:**
- Agent databases are SQLite files - protect them like any sensitive data
- Use file permissions to restrict access
- Encrypt databases at rest if needed
- Be cautious with sensitive data in tool call parameters/results

**Determinism:**
- The Hermit sandbox provides deterministic execution
- Same inputs produce same outputs (useful for testing)
- Some non-deterministic operations may be restricted

## Troubleshooting

### "agent.db already exists"

Use `--force` to overwrite:
```bash
agentos init --force agent.db
```

### Program can't find files in /agent

Make sure you're running the program with `agentos run`:
```bash
agentos run /bin/bash
```

Files written to `/agent` inside the sandbox are stored in `agent.db`, not on the host filesystem.

### SQLite database is locked

Only one process can write to the database at a time. Make sure you don't have multiple `agentos run` instances using the same `agent.db` file.

### Permission denied errors

Check file permissions in the agent filesystem using `agentos fs ls -l` (once implemented) or by querying the `fs_inode` table directly.

## Learn More

- **[Agent Filesystem Specification](SPEC.md)** - Complete technical specification of the filesystem schema
- **[SDK Examples](sdk/examples/)** - Working code examples
- **[README](README.md)** - Project overview and motivation
- **[Hermit Project](https://github.com/facebookexperimental/hermit)** - The underlying sandbox technology

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT
