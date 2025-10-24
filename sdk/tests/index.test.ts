import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { AgentOS } from '../src/index';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

describe('AgentOS Integration Tests', () => {
  let agent: AgentOS;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    // Create temporary directory for test database
    tempDir = mkdtempSync(join(tmpdir(), 'agentos-test-'));
    dbPath = join(tempDir, 'test.db');

    // Initialize AgentOS
    agent = new AgentOS(dbPath);
  });

  afterEach(() => {
    // Clean up temporary directories
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('Initialization', () => {
    it('should successfully initialize with a file path', async () => {
      await agent.ready();
      expect(agent).toBeDefined();
      expect(agent).toBeInstanceOf(AgentOS);
    });

    it('should initialize with in-memory database', async () => {
      const memoryAgent = new AgentOS(':memory:');
      await memoryAgent.ready();
      expect(memoryAgent).toBeDefined();
      expect(memoryAgent).toBeInstanceOf(AgentOS);
    });

    it('should allow multiple instances with different databases', async () => {
      const dbPath2 = join(tempDir, 'test2.db');
      const agent2 = new AgentOS(dbPath2);

      await agent.ready();
      await agent2.ready();

      expect(agent).toBeDefined();
      expect(agent2).toBeDefined();
      expect(agent).not.toBe(agent2);
    });
  });

  describe('Database Persistence', () => {
    it('should persist database file to disk', async () => {
      // Use the agent from beforeEach
      await agent.ready();

      // Check that database file exists
      const fs = require('fs');
      expect(fs.existsSync(dbPath)).toBe(true);
    });

    it('should reuse existing database file', async () => {
      // Create first instance and write data
      const testDbPath = join(tempDir, 'persistence-test.db');
      const agent1 = new AgentOS(testDbPath);
      await agent1.ready();
      await agent1.kv.set('test', 'value1');
      await agent1.close();

      // Create second instance with same path - should be able to read the data
      const agent2 = new AgentOS(testDbPath);
      await agent2.ready();
      const value = await agent2.kv.get('test');

      expect(agent1).toBeDefined();
      expect(agent2).toBeDefined();
      expect(value).toBe('value1');
    });
  });

});
