import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { AgentFS } from '../src/index';
import { existsSync, rmSync } from 'fs';

describe('AgentFS Integration Tests', () => {
  let agent: AgentFS;
  const testId = 'test-agent';

  beforeEach(async () => {
    // Initialize AgentFS with a test id
    agent = await AgentFS.open({ id: testId });
  });

  afterEach(async () => {
    // Close the agent
    await agent.close();

    // Clean up test database file
    const dbPath = `.agentfs/${testId}.db`;
    try {
      if (existsSync(dbPath)) {
        rmSync(dbPath, { force: true });
      }
      // Clean up SQLite WAL files if they exist
      if (existsSync(`${dbPath}-shm`)) {
        rmSync(`${dbPath}-shm`, { force: true });
      }
      if (existsSync(`${dbPath}-wal`)) {
        rmSync(`${dbPath}-wal`, { force: true });
      }
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('Initialization', () => {
    it('should successfully initialize with an id', async () => {
      expect(agent).toBeDefined();
      expect(agent).toBeInstanceOf(AgentFS);
    });

    it('should initialize with ephemeral in-memory database', async () => {
      const memoryAgent = await AgentFS.open();
      expect(memoryAgent).toBeDefined();
      expect(memoryAgent).toBeInstanceOf(AgentFS);
      await memoryAgent.close();
    });

    it('should allow multiple instances with different ids', async () => {
      const agent2 = await AgentFS.open({ id: 'test-agent-2' });

      expect(agent).toBeDefined();
      expect(agent2).toBeDefined();
      expect(agent).not.toBe(agent2);

      await agent2.close();
      // Clean up second agent's database
      const dbPath2 = '.agentfs/test-agent-2.db';
      if (existsSync(dbPath2)) {
        rmSync(dbPath2, { force: true });
      }
    });
  });

  describe('Database Persistence', () => {
    it('should persist database file to .agentfs directory', async () => {
      // Check that database file exists in .agentfs directory
      const dbPath = `.agentfs/${testId}.db`;
      expect(existsSync(dbPath)).toBe(true);
    });

    it('should reuse existing database file with same id', async () => {
      // Create first instance and write data
      const persistenceTestId = 'persistence-test';
      const agent1 = await AgentFS.open({ id: persistenceTestId });
      await agent1.kv.set('test', 'value1');
      await agent1.close();

      // Create second instance with same id - should be able to read the data
      const agent2 = await AgentFS.open({ id: persistenceTestId });
      const value = await agent2.kv.get('test');

      expect(agent1).toBeDefined();
      expect(agent2).toBeDefined();
      expect(value).toBe('value1');

      await agent2.close();

      // Clean up
      const dbPath = `.agentfs/${persistenceTestId}.db`;
      if (existsSync(dbPath)) {
        rmSync(dbPath, { force: true });
      }
    });
  });

});
