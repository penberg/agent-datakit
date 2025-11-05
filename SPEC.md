# Agent Filesystem Specification

**Version:** 0.0

## Introduction

The Agent Filesystem Specification defines a SQLite schema for representing agent filesystem state. The specification consists of three main components:

1. **Tool Call Audit Trail**: Captures tool invocations, parameters, and results for debugging, auditing, and performance analysis
2. **Virtual Filesystem**: Stores agent artifacts (files, documents, outputs) using a Unix-like inode design with support for hard links, proper metadata, and efficient file operations
3. **Key-Value Store**: Provides simple get/set operations for agent context, preferences, and structured state that doesn't fit into the filesystem model

All timestamps in this specification use Unix epoch format (seconds since 1970-01-01 00:00:00 UTC).

## Tool Calls

The tool call tracking schema captures tool invocations for debugging, auditing, and analysis.

### Schema

#### Table: `tool_calls`

Stores individual tool invocations with parameters and results. This is an insert-only audit log.

```sql
CREATE TABLE tool_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  parameters TEXT,
  result TEXT,
  error TEXT,
  started_at INTEGER NOT NULL,
  completed_at INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL
)

CREATE INDEX idx_tool_calls_name ON tool_calls(name)
CREATE INDEX idx_tool_calls_started_at ON tool_calls(started_at)
```

**Fields:**

- `id` - Unique tool call identifier
- `name` - Tool name (e.g., 'read_file', 'web_search', 'execute_code')
- `parameters` - JSON-serialized input parameters (NULL if no parameters)
- `result` - JSON-serialized result (NULL if error)
- `error` - Error message (NULL if success)
- `started_at` - Invocation timestamp (Unix timestamp, seconds)
- `completed_at` - Completion timestamp (Unix timestamp, seconds)
- `duration_ms` - Execution duration in milliseconds

### Operations

#### Record Tool Call

```sql
INSERT INTO tool_calls (name, parameters, result, error, started_at, completed_at, duration_ms)
VALUES (?, ?, ?, ?, ?, ?, ?)
```

**Note:** Insert once when the tool call completes. Either `result` or `error` should be set, not both.

#### Query Tool Calls by Name

```sql
SELECT * FROM tool_calls
WHERE name = ?
ORDER BY started_at DESC
```

#### Query Recent Tool Calls

```sql
SELECT * FROM tool_calls
WHERE started_at > ?
ORDER BY started_at DESC
```

#### Analyze Tool Performance

```sql
SELECT
  name,
  COUNT(*) as total_calls,
  SUM(CASE WHEN error IS NULL THEN 1 ELSE 0 END) as successful,
  SUM(CASE WHEN error IS NOT NULL THEN 1 ELSE 0 END) as failed,
  AVG(duration_ms) as avg_duration_ms
FROM tool_calls
GROUP BY name
ORDER BY total_calls DESC
```

### Consistency Rules

1. Exactly one of `result` or `error` SHOULD be non-NULL (mutual exclusion)
2. `completed_at` MUST always be set (no NULL values)
3. `duration_ms` MUST always be set and equal to `(completed_at - started_at) * 1000`
4. Parameters and results MUST be valid JSON strings when present
5. Records MUST NOT be updated or deleted (insert-only audit log)

### Implementation Notes

- This is an insert-only audit log - no updates or deletes
- Insert the record once when the tool call completes
- Set either `result` (on success) or `error` (on failure), but not both
- `parameters`, `result`, and `error` are stored as JSON-serialized strings
- `duration_ms` should be computed as `(completed_at - started_at) * 1000`
- Use indexes for efficient queries by name or time
- Consider periodic archival of old tool call records to a separate table

### Extension Points

Implementations MAY extend the tool call schema with additional functionality:

- Session/conversation grouping (add `session_id` field)
- User attribution (add `user_id` field)
- Cost tracking (add `cost` field for API calls)
- Parent/child relationships for nested tool calls
- Token usage tracking
- Input/output size metrics

Such extensions SHOULD use separate tables to maintain referential integrity.

## Virtual Filesystem

The virtual filesystem provides POSIX-like file operations for agent artifacts. The filesystem separates namespace (paths and names) from data (file content and metadata) using a Unix-like inode design. This enables hard links (multiple paths to the same file), efficient file operations, proper file metadata (permissions, timestamps), and chunked content storage.

### Schema

#### Table: `fs_inode`

Stores file and directory metadata.

```sql
CREATE TABLE fs_inode (
  ino INTEGER PRIMARY KEY AUTOINCREMENT,
  mode INTEGER NOT NULL,
  uid INTEGER NOT NULL DEFAULT 0,
  gid INTEGER NOT NULL DEFAULT 0,
  size INTEGER NOT NULL DEFAULT 0,
  atime INTEGER NOT NULL,
  mtime INTEGER NOT NULL,
  ctime INTEGER NOT NULL
)
```

**Fields:**

- `ino` - Inode number (unique identifier)
- `mode` - File type and permissions (Unix mode bits)
- `uid` - Owner user ID
- `gid` - Owner group ID
- `size` - Total file size in bytes
- `atime` - Last access time (Unix timestamp, seconds)
- `mtime` - Last modification time (Unix timestamp, seconds)
- `ctime` - Creation/change time (Unix timestamp, seconds)

**Mode Encoding:**

The `mode` field combines file type and permissions:

```
File type (upper bits):
  0o170000 - File type mask (S_IFMT)
  0o100000 - Regular file (S_IFREG)
  0o040000 - Directory (S_IFDIR)
  0o120000 - Symbolic link (S_IFLNK)

Permissions (lower 12 bits):
  0o000777 - Permission bits (rwxrwxrwx)

Example:
  0o100644 - Regular file, rw-r--r--
  0o040755 - Directory, rwxr-xr-x
```

**Special Inodes:**

- Inode 1 MUST be the root directory

#### Table: `fs_dentry`

Maps names to inodes (directory entries).

```sql
CREATE TABLE fs_dentry (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  parent_ino INTEGER NOT NULL,
  ino INTEGER NOT NULL,
  UNIQUE(parent_ino, name)
)

CREATE INDEX idx_fs_dentry_parent ON fs_dentry(parent_ino, name)
```

**Fields:**

- `id` - Internal entry ID
- `name` - Basename (filename or directory name)
- `parent_ino` - Parent directory inode number
- `ino` - Inode this entry points to

**Constraints:**

- `UNIQUE(parent_ino, name)` - No duplicate names in a directory

**Notes:**

- Root directory (ino=1) has no dentry (no parent)
- Multiple dentries MAY point to the same inode (hard links)
- Link count = `SELECT COUNT(*) FROM fs_dentry WHERE ino = ?`

#### Table: `fs_data`

Stores file content in chunks.

```sql
CREATE TABLE fs_data (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ino INTEGER NOT NULL,
  offset INTEGER NOT NULL,
  size INTEGER NOT NULL,
  data BLOB NOT NULL
)

CREATE INDEX idx_fs_data_ino_offset ON fs_data(ino, offset)
```

**Fields:**

- `id` - Internal chunk ID
- `ino` - Inode number
- `offset` - Byte offset in file where chunk starts
- `size` - Chunk size in bytes
- `data` - Binary content (BLOB)

**Notes:**

- Directories MUST NOT have data chunks
- Chunks MUST be ordered by offset when reading
- Implementations MAY store files as single chunks or multiple chunks

#### Table: `fs_symlink`

Stores symbolic link targets.

```sql
CREATE TABLE fs_symlink (
  ino INTEGER PRIMARY KEY,
  target TEXT NOT NULL
)
```

**Fields:**

- `ino` - Inode number of the symlink
- `target` - Target path (may be absolute or relative)

### Operations

#### Path Resolution

To resolve a path to an inode:

1. Start at root inode (ino=1)
2. Split path by `/` and filter empty components
3. For each component:
   ```sql
   SELECT ino FROM fs_dentry WHERE parent_ino = ? AND name = ?
   ```
4. Return final inode or NULL if any component not found

#### Creating a File

1. Resolve parent directory path to inode
2. Insert inode:
   ```sql
   INSERT INTO fs_inode (mode, uid, gid, size, atime, mtime, ctime)
   VALUES (?, ?, ?, 0, ?, ?, ?)
   RETURNING ino
   ```
3. Insert directory entry:
   ```sql
   INSERT INTO fs_dentry (name, parent_ino, ino)
   VALUES (?, ?, ?)
   ```
4. Insert data:
   ```sql
   INSERT INTO fs_data (ino, offset, size, data)
   VALUES (?, 0, ?, ?)
   ```
5. Update inode size:
   ```sql
   UPDATE fs_inode SET size = ?, mtime = ? WHERE ino = ?
   ```

#### Reading a File

1. Resolve path to inode
2. Fetch all chunks:
   ```sql
   SELECT data FROM fs_data WHERE ino = ? ORDER BY offset ASC
   ```
3. Concatenate chunks in order
4. Update access time:
   ```sql
   UPDATE fs_inode SET atime = ? WHERE ino = ?
   ```

#### Listing a Directory

1. Resolve directory path to inode
2. Query entries:
   ```sql
   SELECT name FROM fs_dentry WHERE parent_ino = ? ORDER BY name ASC
   ```

#### Deleting a File

1. Resolve path to get inode and parent
2. Delete directory entry:
   ```sql
   DELETE FROM fs_dentry WHERE parent_ino = ? AND name = ?
   ```
3. Check if last link:
   ```sql
   SELECT COUNT(*) FROM fs_dentry WHERE ino = ?
   ```
4. If count = 0, delete inode (CASCADE deletes data):
   ```sql
   DELETE FROM fs_inode WHERE ino = ?
   ```

#### Creating a Hard Link

1. Resolve source path to get inode
2. Resolve destination parent to get parent_ino
3. Insert new directory entry:
   ```sql
   INSERT INTO fs_dentry (name, parent_ino, ino)
   VALUES (?, ?, ?)
   ```

#### Reading File Metadata (stat)

1. Resolve path to inode
2. Query inode:
   ```sql
   SELECT ino, mode, uid, gid, size, atime, mtime, ctime
   FROM fs_inode WHERE ino = ?
   ```
3. Count links:
   ```sql
   SELECT COUNT(*) as nlink FROM fs_dentry WHERE ino = ?
   ```

### Initialization

When creating a new agent database, initialize the filesystem root directory:

```sql
INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
VALUES (1, 16877, 0, 0, 0, unixepoch(), unixepoch(), unixepoch())
```

Where `16877` = `0o040755` (directory with rwxr-xr-x permissions)

### Consistency Rules

1. Root inode (ino=1) MUST always exist
2. Every dentry MUST reference a valid inode
3. Every dentry MUST reference a valid parent inode
4. No directory MAY contain duplicate names
5. Directories MUST have mode with S_IFDIR bit set
6. Regular files MUST have mode with S_IFREG bit set
7. File size MUST match total size of all data chunks
8. Every inode MUST have at least one dentry (except root)

### Implementation Notes

- Use `RETURNING` clause to safely get auto-generated inode numbers
- Parent directories are created implicitly as needed
- Empty files have an inode but no data chunks
- Symlink resolution is implementation-defined (not part of schema)
- Use transactions for multi-step operations to maintain consistency

### Extension Points

Implementations MAY extend the filesystem schema with additional functionality:

- Extended attributes table
- File ACLs and advanced permissions
- Quota tracking per user/group
- Version history and snapshots
- Content deduplication
- Compression metadata
- File checksums/hashes

Such extensions SHOULD use separate tables to maintain referential integrity.

## Key-Value Data

The key-value store provides simple get/set operations for agent context and state.

### Schema

#### Table: `kv_store`

Stores arbitrary key-value pairs with automatic timestamping.

```sql
CREATE TABLE kv_store (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  created_at INTEGER DEFAULT (unixepoch()),
  updated_at INTEGER DEFAULT (unixepoch())
)

CREATE INDEX idx_kv_store_created_at ON kv_store(created_at)
```

**Fields:**

- `key` - Unique key identifier
- `value` - JSON-serialized value
- `created_at` - Creation timestamp (Unix timestamp, seconds)
- `updated_at` - Last update timestamp (Unix timestamp, seconds)

### Operations

#### Set a Value

```sql
INSERT INTO kv_store (key, value, updated_at)
VALUES (?, ?, unixepoch())
ON CONFLICT(key) DO UPDATE SET
  value = excluded.value,
  updated_at = unixepoch()
```

#### Get a Value

```sql
SELECT value FROM kv_store WHERE key = ?
```

#### Delete a Value

```sql
DELETE FROM kv_store WHERE key = ?
```

#### List All Keys

```sql
SELECT key, created_at, updated_at FROM kv_store ORDER BY key ASC
```

### Consistency Rules

1. Keys MUST be unique (enforced by PRIMARY KEY)
2. Values MUST be valid JSON strings
3. Timestamps MUST use Unix epoch format (seconds)

### Implementation Notes

- Values are stored as JSON strings; serialize before storing, deserialize after retrieving
- Use `ON CONFLICT` clause for upsert operations
- Indexes on `created_at` support temporal queries
- Updates automatically refresh the `updated_at` timestamp
- Keys can use any naming convention (e.g., namespaced: `user:preferences`, `session:state`)

### Extension Points

Implementations MAY extend the key-value store schema with additional functionality:

- Namespaced keys with hierarchy support
- Value versioning/history
- TTL (time-to-live) for automatic expiration
- Value size limits and quotas

Such extensions SHOULD use separate tables to maintain referential integrity.
