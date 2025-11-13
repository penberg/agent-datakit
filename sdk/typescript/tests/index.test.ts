import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { AgentFS } from '../src/index';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

describe('AgentFS Integration Tests', () => {
  let agent: AgentFS;
  let tempDir: string;
  let dbPath: string;

  beforeEach(async () => {
    // Create temporary directory for test database
    tempDir = mkdtempSync(join(tmpdir(), 'agentfs-test-'));
    dbPath = join(tempDir, 'test.db');

    // Initialize AgentFS
    agent = await AgentFS.open(dbPath);
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
      expect(agent).toBeDefined();
      expect(agent).toBeInstanceOf(AgentFS);
    });

    it('should initialize with in-memory database', async () => {
      const memoryAgent = await AgentFS.open(':memory:');
      expect(memoryAgent).toBeDefined();
      expect(memoryAgent).toBeInstanceOf(AgentFS);
    });

    it('should allow multiple instances with different databases', async () => {
      const dbPath2 = join(tempDir, 'test2.db');
      const agent2 = await AgentFS.open(dbPath2);

      expect(agent).toBeDefined();
      expect(agent2).toBeDefined();
      expect(agent).not.toBe(agent2);
    });
  });

  describe('Database Persistence', () => {
    it('should persist database file to disk', async () => {
      // Use the agent from beforeEach
      // Check that database file exists
      const fs = require('fs');
      expect(fs.existsSync(dbPath)).toBe(true);
    });

    it('should reuse existing database file', async () => {
      // Create first instance and write data
      const testDbPath = join(tempDir, 'persistence-test.db');
      const agent1 = await AgentFS.open(testDbPath);
      await agent1.kv.set('test', 'value1');
      await agent1.close();

      // Create second instance with same path - should be able to read the data
      const agent2 = await AgentFS.open(testDbPath);
      const value = await agent2.kv.get('test');

      expect(agent1).toBeDefined();
      expect(agent2).toBeDefined();
      expect(value).toBe('value1');
    });
  });

});
