import { AgentFS } from 'agentfs-sdk';

let instance: AgentFS | null = null;

export async function getAgentFS(): Promise<AgentFS> {
  if (!instance) {
    const dbPath = process.env.AGENTFS_DB || 'agentfs.db';
    instance = await AgentFS.create(dbPath);
  }
  return instance;
}


