<p align="center">
  <h1 align="center">AgentFS</h1>
</p>

<p align="center">
  The filesystem for agents.
</p>

<p align="center">
  <a title="Build Status" target="_blank" href="https://github.com/tursodatabase/agentfs/actions/workflows/rust.yml"><img src="https://img.shields.io/github/actions/workflow/status/tursodatabase/agentfs/rust.yml?style=flat-square"></a>
  <a title="Rust" target="_blank" href="https://crates.io/crates/agentfs-sdk"><img alt="Crate" src="https://img.shields.io/crates/v/agentfs-sdk"></a>
  <a title="JavaScript" target="_blank" href="https://www.npmjs.com/package/@tursodatabase/agentfs-sdk"><img alt="NPM" src="https://img.shields.io/npm/v/agentfs-sdk"></a>
  <a title="MIT" target="_blank" href="https://github.com/tursodatabase/agentfs/blob/main/LICENSE.md"><img src="http://img.shields.io/badge/license-MIT-orange.svg?style=flat-square"></a>
</p>
<p align="center">
  <a title="Users's Discord" target="_blank" href="https://tur.so/discord"><img alt="Chat with other users of Turso (and Turso Cloud) on Discord" src="https://img.shields.io/discord/933071162680958986?label=Discord&logo=Discord&style=social&label=Users"></a>
</p>

---

> **⚠️ Warning:** This software is ALPHA; use only for development, testing, and experimentation. We are working to make it production-ready, but do not use it for critical data until it is ready.

## What is AgentFS?

AgentFS is a filesystem explicitly designed for AI agents. Just as traditional filesystems provide file and directory abstractions for applications, AgentFS provides the storage abstractions that AI agents need.

At the heart of AgentFS is the [agent filesystem](SPEC.md), a complete SQLite-based storage system for agents implemented using [Turso](https://github.com/tursodatabase/turso). AgentFS provides three essential interfaces for agent state management: a POSIX-like filesystem for files and directories, a key-value store for agent state and context, and a toolcall audit trail for debugging and analysis. Everything an agent does, every file it creates, every piece of state it stores, every tool it invokes, lives in a single SQLite database file.

AgentFS provides four components:

* **SDK** - [TypeScript](sdk/typescript) and [Rust](sdk/rust) libraries for programmatic filesystem access
* **[CLI](MANUAL.md)** - Command-line interface for managing agent filesystems
* **[Specification](SPEC.md)** - SQLite-based agent filesystem specification
* **Sandbox** - Linux-compatible execution environment with agent filesystem support (_experimental_)

## Getting Started

### Using the CLI

Initialize an agent filesystem:

```bash
$ agentfs init
Created agent filesystem: agent.db
```

Run a program in the sandbox with the agent filesystem mounted at `/agent`:

```bash
$ agentfs run /bin/bash
Welcome to AgentFS!

$ echo "hello from agent" > /agent/hello.txt
$ cat /agent/hello.txt
hello from agent
$ exit
```

Inspect the agent filesystem from outside:

```bash
$ agentfs fs ls
f hello.txt

$ agentfs fs cat hello.txt
hello from agent
```

Read the **[User Manual](MANUAL.md)** for complete documentation.

### Using the SDK

Install the SDK in your project:

```bash
npm install agentfs-sdk
```

Use it in your agent code:

```typescript
import { AgentFS } from 'agentfs-sdk';

const agent = new AgentFS('./agent.db');

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

See the **[examples](examples)** directory for more comprehensive examples.

## Why AgentFS?

**Auditability**: Every file operation, tool call, and state change is recorded in SQLite. Query your agent's complete history with SQL to debug issues, analyze behavior, or meet compliance requirements.

**Reproducibility**: Snapshot an agent's state at any point with `cp agent.db snapshot.db`. Restore it later to reproduce exact execution states, test what-if scenarios, or roll back mistakes.

**Portability**: The entire agent runtime—files, state, history —is stored in a single SQLite file. Move it between machines, check it into version control, or deploy it to any system where Turso runs.

**Simplicity**: No configuration files, no database servers, no distributed systems. Just a single file and a simple API.

**Sandboxing**: Run agents in an isolated Linux environment where filesystem access is controlled and monitored. Perfect for testing untrusted code or enforcing security policies.

## Learn More

- **[User Manual](MANUAL.md)** - Complete guide to using the AgentFS CLI and SDK
- **[Agent Filesystem Specification](SPEC.md)** - Technical specification of the agent filesystem SQLite schema
- **[SDK Examples](examples/)** - Working code examples using AgentFS
- **[Turso database](https://github.com/tursodatabase/turso)** - an in-process SQL database, compatible with SQLite.

## License

MIT
