#!/usr/bin/env node
import 'dotenv/config';
import { mastra } from '../src/mastra/index.js';
import { getAgentFS } from '../src/mastra/utils/agentfs.js';

async function main() {
  const args = process.argv.slice(2);
  const question = args.join(' ').trim();

  if (!question) {
    console.error('Usage: npm run ask -- "<your research question>"');
    process.exit(1);
  }

  try {
    // Initialize agentfs
    const agentfs = await getAgentFS();

    // Get the agent
    const agent = mastra.getAgent('researchAgent');
    if (!agent) {
      console.error('Research agent not available.');
      process.exit(1);
    }

    // Simply ask the agent - it has all the logic and tools it needs
    const response = await agent.generate([
      {
        role: 'user',
        content: question,
      },
    ]);

    // Output the agent's response
    console.log(response.text);

  } catch (err: any) {
    console.error('Error:', err?.message || err);
    process.exit(1);
  }
}

main();


