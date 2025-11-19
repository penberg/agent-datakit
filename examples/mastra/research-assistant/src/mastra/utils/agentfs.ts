import { AgentFS } from 'agentfs-sdk';

let instance: AgentFS | null = null;

export async function getAgentFS(): Promise<AgentFS> {
  if (!instance) {
    const id = process.env.AGENTFS_ID || 'research-assistant';
    instance = await AgentFS.open({ id });
  }
  return instance;
}


