<p align="center">
  <h1 align="center">AgentOS</h1>
</p>

<p align="center">
  The operating system for agents
</p>

---

> **⚠️ Warning:** This software is ALPHA; use only for development, testing, and experimentation. We are working to make it production-ready, but do not use it for critical data until it is ready.

## What is AgentOS?

AgentOS is an operating system designed specifically for running AI agents. Just as traditional operating systems provide processes, filesystems, and system calls for applications, AgentOS provides the runtime environment and storage abstractions that AI agents need.

At the heart of AgentOS is the [agent filesystem](SPEC.md) - a complete SQLite-based storage system for agents implemented using [Turso](https://github.com/tursodatabase/turso). It combines three essential components: a POSIX-like virtual filesystem for files and directories, a key-value store for agent state and context, and an audit trail tool for debugging and analysis. Everything an agent does - every file it creates, every piece of state it stores, every tool it invokes - lives in a single SQLite database file.

## The Agent Filesystem

The agent filesystem stores three types of data in SQLite:

1. **Files and Directories**: A virtual filesystem with Unix-like inodes, directory entries, and file content
2. **Key-Value Store**: Simple storage for agent context, preferences, and state
3. **Tool Call Audit Trail**: Complete history of tool invocations, parameters, and results

This design provides unique capabilities:

- **Single File**: Your agent's entire state is one `.db` file - copy it, version it, share it
- **Snapshots**: `cp agent.db backup.db` creates a complete snapshot instantly
- **Auditability**: Query everything with SQL - find all files created in the last hour, analyze tool usage patterns, debug agent behavior
- **Portability**: Works everywhere SQLite works - Linux, macOS, Windows, embedded devices, cloud
- **ACID Guarantees**: Transactions ensure consistency even during crashes

## Components

AgentOS provides three main components:

1. **[AgentOS CLI](MANUAL.md)**: Command-line tool for managing agents and running them in sandboxed environments
2. **AgentOS SDK**: TypeScript/JavaScript SDK for building agents (see [MANUAL.md](MANUAL.md#agentos-sdk) for full documentation)
3. **[Agent Filesystem Specification](SPEC.md)**: Complete technical specification of the SQLite schema

## Quick Start

### Using the CLI

Initialize an agent filesystem:

```bash
$ agentos init
Created agent filesystem: agent.db
```

Run a program in the sandbox with the agent filesystem mounted at `/agent`:

```bash
$ agentos run /bin/bash
Welcome to AgentOS!

$ echo "hello from agent" > /agent/hello.txt
$ cat /agent/hello.txt
hello from agent
$ exit
```

Inspect the agent filesystem from outside:

```bash
$ agentos fs ls
f hello.txt

$ agentos fs cat hello.txt
hello from agent
```

Read the **[User Manual](MANUAL.md)** for complete documentation.

### Using the SDK

Install the SDK in your project:

```bash
npm install agentos-sdk
```

Use it in your agent code:

```typescript
import { AgentOS } from 'agentos-sdk';

const agent = new AgentOS('./agent.db');

// Key-value operations
await agent.kv.set('user:preferences', { theme: 'dark' });
const prefs = await agent.kv.get('user:preferences');

// Filesystem operations
await agent.fs.writeFile('/output/report.pdf', pdfBuffer);
const files = await agent.fs.readdir('/output');

// Tool call tracking
await agent.tools.record(
  'web_search',
  Date.now() / 1000,
  Date.now() / 1000 + 1.5,
  { query: 'AI' },
  { results: [...] }
);
```

See the **[SDK documentation in MANUAL.md](MANUAL.md#agentos-sdk)** and **[examples](sdk/examples/)** for more details.

## Why AgentOS?

**Auditability**: Every file operation, tool call, and state change is recorded in SQLite. Query your agent's complete history with SQL to debug issues, analyze behavior, or meet compliance requirements.

**Reproducibility**: Snapshot an agent's state at any point with `cp agent.db snapshot.db`. Restore it later to reproduce exact execution states, test what-if scenarios, or roll back mistakes.

**Portability**: The entire agent runtime—files, state, history —is stored in a single SQLite file. Move it between machines, check it into version control, or deploy it to any system where Turso runs.

**Simplicity**: No configuration files, no database servers, no distributed systems. Just a single file and a simple API.

**Sandboxing**: Run agents in an isolated Linux environment where filesystem access is controlled and monitored. Perfect for testing untrusted code or enforcing security policies.

## Learn More

- **[User Manual](MANUAL.md)** - Complete guide to using the AgentOS CLI and SDK
- **[Agent Filesystem Specification](SPEC.md)** - Technical specification of the SQLite schema
- **[SDK Examples](sdk/examples/)** - Working code examples
- **[Turso database](https://github.com/tursodatabase/turso)** - an in-process SQL database, compatible with SQLite.

## License

MIT
